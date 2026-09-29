use std::{collections::HashMap, sync::Arc, time::Instant};

use aircommon::identifiers::QsClientId;
use airprotos::queue_service::v1::{
    ListenResponse, SiblingClientState, SiblingClientStateEncryptedBlob, SiblingClientStateRemoved,
    SiblingClientStateUpdated, listen_response, sibling_client_state,
};
use tokio::sync::mpsc;
use tracing::debug;

/// Clients of a user, removed from `registry` when dropped
#[derive(Debug, Default)]
pub(super) struct UserClients {
    /// Epoch and current listen session per client
    clients: parking_lot::Mutex<HashMap<QsClientId, ClientEntry>>,
}

#[derive(Debug, Default)]
struct Epoch(u64);

impl Epoch {
    fn advance(&mut self) -> Self {
        self.0 += 1;
        Self(self.0)
    }

    fn to_session_id(&self) -> SessionId {
        SessionId(self.0)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) struct SessionId(u64);

#[derive(Debug, Default)]
struct ClientEntry {
    /// Last epoch of the client, kept across its listen sessions
    epoch: Epoch,
    session: Option<ClientSessionData>,
}

#[derive(Debug)]
struct ClientSessionData {
    /// Distinguishes different sessions of the same client (e.g. after reconnects)
    session_id: SessionId,
    /// Clone of the `ListenerContext` sender for fan-out
    payload_tx: mpsc::Sender<ListenResponse>,
    /// Last state reported in this session
    state: Option<ClientState>,
}

/// State reported by a client in its current listen session
#[derive(Debug)]
pub(super) struct ClientState {
    epoch: Epoch,
    received_at: Instant,
    encrypted_blob: Vec<u8>,
}

/// Listen session of a client, ended when dropped
#[derive(Debug)]
pub(crate) struct ClientSession {
    clients: Arc<UserClients>,
    client_id: QsClientId,
    pub(super) session_id: SessionId,
}

impl ClientSession {
    /// Stores the state of the client and relays it to the listening siblings.
    pub(crate) fn update_state(&self, encrypted_blob: Vec<u8>) {
        self.clients
            .update(self.client_id, self.session_id, encrypted_blob);
    }
}

impl Drop for ClientSession {
    fn drop(&mut self) {
        self.clients.remove(self.client_id, self.session_id);
    }
}

impl UserClients {
    /// Starts a listen session of `client_id`, replacing its previous one.
    pub(super) fn join(
        self: Arc<Self>,
        client_id: QsClientId,
        payload_tx: mpsc::Sender<ListenResponse>,
    ) -> ClientSession {
        let mut clients = self.clients.lock();
        let entry = clients.entry(client_id).or_default();
        let epoch = entry.epoch.advance();
        let session_id = epoch.to_session_id();
        let replaced = entry.session.replace(ClientSessionData {
            session_id,
            payload_tx,
            state: None,
        });
        if replaced.is_some_and(|session| session.state.is_some()) {
            fan_out_removed(&clients, client_id, epoch);
        }
        drop(clients);
        ClientSession {
            clients: self,
            client_id,
            session_id,
        }
    }

    /// Stores the state of `client_id` if `session_id` is its current listen
    /// session, and relays it to the listening siblings.
    fn update(&self, client_id: QsClientId, session_id: SessionId, encrypted_blob: Vec<u8>) {
        let mut clients = self.clients.lock();
        let Some(ClientEntry {
            epoch,
            session: Some(session),
        }) = clients.get_mut(&client_id)
        else {
            return;
        };
        // important: only update the state of the same session
        if session.session_id != session_id {
            return;
        }

        let state = ClientState {
            epoch: epoch.advance(),
            received_at: Instant::now(),
            encrypted_blob,
        };
        let updated = state.to_updated(client_id);
        session.state = Some(state);

        fan_out(
            &clients,
            client_id,
            sibling_client_state::Change::Updated(updated),
        );
    }

    /// Ends the listen session `session_id` of `client_id`, if it is the
    /// current one, and tells the siblings that its state is gone.
    pub(super) fn remove(&self, client_id: QsClientId, session_id: SessionId) {
        let mut clients = self.clients.lock();
        let Some(entry) = clients.get_mut(&client_id) else {
            return;
        };
        if entry
            .session
            .as_ref()
            .is_none_or(|session| session.session_id != session_id)
        {
            return;
        }
        // The entry stays to keep the epoch of the client.
        if entry
            .session
            .take()
            .and_then(|session| session.state)
            .is_some()
        {
            // one-shot, we don't store this
            let epoch = entry.epoch.advance();
            fan_out_removed(&clients, client_id, epoch);
        }
    }

    /// Returns the states of the listening siblings of `client_id`.
    pub(crate) fn states(&self, client_id: QsClientId) -> Vec<ListenResponse> {
        self.clients
            .lock()
            .iter()
            .filter(|(id, _)| **id != client_id)
            .filter_map(|(id, entry)| Some(entry.session.as_ref()?.state.as_ref()?.to_updated(*id)))
            .map(|updated| client_state_response(sibling_client_state::Change::Updated(updated)))
            .collect()
    }
}

impl ClientState {
    fn to_updated(&self, client_id: QsClientId) -> SiblingClientStateUpdated {
        let age_ms = self.received_at.elapsed().as_millis();
        SiblingClientStateUpdated {
            client_id: Some(client_id.into()),
            epoch: self.epoch.0,
            age_ms: age_ms.try_into().unwrap_or(u32::MAX),
            blob: Some(SiblingClientStateEncryptedBlob {
                encrypted_blob: self.encrypted_blob.clone(),
            }),
        }
    }
}

/// Tells the listening siblings of `client_id` that its state is gone.
fn fan_out_removed(
    clients: &HashMap<QsClientId, ClientEntry>,
    client_id: QsClientId,
    epoch: Epoch,
) {
    let removed = SiblingClientStateRemoved {
        client_id: Some(client_id.into()),
        epoch: epoch.0,
    };
    fan_out(
        clients,
        client_id,
        sibling_client_state::Change::Removed(removed),
    );
}

/// Sends `change` of `client_id` to its listening siblings without waiting.
fn fan_out(
    clients: &HashMap<QsClientId, ClientEntry>,
    client_id: QsClientId,
    change: sibling_client_state::Change,
) {
    let response = client_state_response(change);
    for (&sibling_id, entry) in clients {
        if sibling_id == client_id {
            continue;
        }
        let Some(session) = &entry.session else {
            continue;
        };
        if session.payload_tx.try_send(response.clone()).is_err() {
            debug!(?sibling_id, "client state not relayed to sibling");
        }
    }
}

fn client_state_response(change: sibling_client_state::Change) -> ListenResponse {
    ListenResponse {
        event: Some(listen_response::Event::SiblingClientState(
            SiblingClientState {
                change: Some(change),
            },
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn random_client_id() -> QsClientId {
        QsClientId::random(&mut rand::rng())
    }

    fn join(
        clients: &Arc<UserClients>,
        client_id: QsClientId,
    ) -> (ClientSession, mpsc::Receiver<ListenResponse>) {
        let (tx, rx) = mpsc::channel(16);
        (clients.clone().join(client_id, tx), rx)
    }

    fn change_of(response: ListenResponse) -> sibling_client_state::Change {
        let Some(listen_response::Event::SiblingClientState(SiblingClientState {
            change: Some(change),
        })) = response.event
        else {
            panic!("expected a client state, got {response:?}");
        };
        change
    }

    fn next_change(rx: &mut mpsc::Receiver<ListenResponse>) -> sibling_client_state::Change {
        change_of(rx.try_recv().expect("missing client state"))
    }

    fn assert_no_change(rx: &mut mpsc::Receiver<ListenResponse>) {
        if let Ok(response) = rx.try_recv() {
            panic!("unexpected client state: {response:?}");
        }
    }

    fn updated_of(change: sibling_client_state::Change) -> SiblingClientStateUpdated {
        let sibling_client_state::Change::Updated(updated) = change else {
            panic!("expected updated, got {change:?}");
        };
        updated
    }

    fn removed_of(change: sibling_client_state::Change) -> SiblingClientStateRemoved {
        let sibling_client_state::Change::Removed(removed) = change else {
            panic!("expected removed, got {change:?}");
        };
        removed
    }

    fn client_id_of(client_id: Option<airprotos::queue_service::v1::QsClientId>) -> QsClientId {
        client_id.unwrap().try_into().unwrap()
    }

    #[test]
    fn fan_out_reaches_listening_siblings() {
        let clients = Arc::new(UserClients::default());
        let a = random_client_id();
        let b = random_client_id();
        let (a_session, mut a_rx) = join(&clients, a);
        let (_b_session, mut b_rx) = join(&clients, b);

        a_session.update_state(b"state".to_vec());
        let updated = updated_of(next_change(&mut b_rx));
        assert_eq!(client_id_of(updated.client_id), a);
        assert_eq!(updated.blob.unwrap().encrypted_blob, b"state");
        assert_eq!(updated.age_ms, 0);
        assert_no_change(&mut a_rx);

        // A late listener gets the current states, aged since they were
        // received.
        std::thread::sleep(Duration::from_millis(10));
        let states = clients.states(b);
        assert_eq!(states.len(), 1);
        let state = updated_of(change_of(states.into_iter().next().unwrap()));
        assert_eq!(state.epoch, updated.epoch);
        assert!(state.age_ms >= 10);
        assert!(clients.states(a).is_empty());

        drop(a_session);
        let removed = removed_of(next_change(&mut b_rx));
        assert_eq!(client_id_of(removed.client_id), a);
        assert!(removed.epoch > updated.epoch);
        assert!(clients.states(b).is_empty());
    }

    #[test]
    fn other_sessions_neither_update_nor_remove() {
        let clients = Arc::new(UserClients::default());
        let a = random_client_id();
        let (a_session, _a_rx) = join(&clients, a);
        let (_b_session, mut b_rx) = join(&clients, random_client_id());
        a_session.update_state(b"state".to_vec());
        next_change(&mut b_rx);

        let stale_session = SessionId(a_session.session_id.0 + 1);
        clients.update(a, stale_session, b"stale".to_vec());
        clients.remove(a, stale_session);
        assert_no_change(&mut b_rx);
    }

    #[test]
    fn rejoining_clears_the_client_state() {
        let clients = Arc::new(UserClients::default());
        let a = random_client_id();
        let b = random_client_id();
        let (a_session, _a_rx) = join(&clients, a);
        let (_b_session, mut b_rx) = join(&clients, b);

        a_session.update_state(b"state".to_vec());
        let updated = updated_of(next_change(&mut b_rx));

        let (new_a_session, _new_a_rx) = join(&clients, a);
        assert_ne!(new_a_session.session_id, a_session.session_id);
        let removed = removed_of(next_change(&mut b_rx));
        assert_eq!(client_id_of(removed.client_id), a);
        assert!(removed.epoch > updated.epoch);
        assert!(clients.states(b).is_empty());

        // The replaced session ending afterwards neither repeats the removal
        // nor ends the new session.
        drop(a_session);
        assert_no_change(&mut b_rx);

        new_a_session.update_state(b"state".to_vec());
        assert!(updated_of(next_change(&mut b_rx)).epoch > removed.epoch);
        assert_eq!(clients.states(b).len(), 1);
    }

    #[test]
    fn sessions_without_state_are_not_relayed() {
        let clients = Arc::new(UserClients::default());
        let a = random_client_id();
        let (_b_session, mut b_rx) = join(&clients, random_client_id());

        let (a_session, _a_rx) = join(&clients, a);
        let (new_a_session, _new_a_rx) = join(&clients, a);
        drop(a_session);
        drop(new_a_session);
        assert_no_change(&mut b_rx);
    }
}
