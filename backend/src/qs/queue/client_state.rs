// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! States of listening clients, relayed to their sibling clients.
//!
//! The state is an opaque blob kept in the client's listen session. It lives
//! until the session that reported it ends or is replaced as well as when the
//! client is deleted.

use std::{collections::HashMap, time::Instant};

use aircommon::{identifiers::QsClientId, time::TimeStamp};
use airprotos::queue_service::v1::{
    ListenResponse, SiblingClientState, SiblingClientStateEncryptedBlob, SiblingClientStateRemoved,
    SiblingClientStateUpdated, listen_response, sibling_client_state,
};
use tokio::sync::mpsc;
use tracing::debug;

use crate::qs::queue::{ClientEntry, ClientSession, Queues, UserClients};

pub(crate) const MAX_ENCRYPTED_CLIENT_STATE_SIZE: usize = 256;

/// State reported by a client in its current listen session
#[derive(Debug)]
pub(super) struct ClientState {
    epoch: u64,
    received_at: Instant,
    encrypted_blob: Vec<u8>,
}

impl ClientState {
    fn updated(&self, client_id: QsClientId) -> SiblingClientStateUpdated {
        let age_ms = self.received_at.elapsed().as_millis();
        SiblingClientStateUpdated {
            client_id: Some(client_id.into()),
            epoch: self.epoch,
            age_ms: age_ms.try_into().unwrap_or(u32::MAX),
            blob: Some(SiblingClientStateEncryptedBlob {
                encrypted_blob: self.encrypted_blob.clone(),
            }),
        }
    }
}

impl UserClients {
    /// Starts a listen session of `client_id`, replacing its previous one.
    ///
    /// Returns the id of the new session.
    pub(super) fn start_session(
        &self,
        client_id: QsClientId,
        payload_tx: mpsc::Sender<ListenResponse>,
    ) -> u64 {
        let mut clients = self.lock();
        let entry = clients.entry(client_id).or_default();
        entry.last_session_id += 1;
        let session_id = entry.last_session_id;
        let replaced = entry.session.replace(ClientSession {
            session_id,
            payload_tx,
            state: None,
        });
        if replaced.is_some_and(|session| session.state.is_some()) {
            let epoch = entry.next_epoch();
            fan_out_removed(&clients, client_id, epoch);
        }
        session_id
    }
}

impl Queues {
    /// Stores the state of `client_id` if `session_id` is its current listen
    /// session, and relays it to the listening siblings.
    pub(crate) fn update_client_state(
        &self,
        client_id: QsClientId,
        session_id: u64,
        encrypted_blob: Vec<u8>,
    ) {
        let Some(user_clients) = self.listening_user_clients(client_id) else {
            return;
        };
        let mut clients = user_clients.lock();
        let Some(ClientEntry {
            epoch,
            session: Some(session),
            ..
        }) = clients.get_mut(&client_id)
        else {
            return;
        };
        // important: only update the state of the same session
        if session.session_id != session_id {
            return;
        }

        *epoch += 1;
        let state = ClientState {
            epoch: *epoch,
            received_at: Instant::now(),
            encrypted_blob,
        };
        let updated = state.updated(client_id);
        session.state = Some(state);

        fan_out(
            &clients,
            client_id,
            sibling_client_state::Change::Updated(updated),
        );
    }

    /// Ends the listen session `session_id` of `client_id`, if it is the
    /// current one, and tells the siblings that its state is gone.
    pub(crate) fn end_client_session(&self, client_id: QsClientId, session_id: u64) {
        let Some(user_clients) = self.listening_user_clients(client_id) else {
            return;
        };
        let mut clients = user_clients.lock();
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
        if entry.session.take().and_then(|session| session.state).is_some() {
            let epoch = entry.next_epoch();
            fan_out_removed(&clients, client_id, epoch);
        }
    }

    /// Forgets `client_id` and tells the siblings that its state is gone.
    ///
    /// Used when the client is deleted. If it is not listening, its epoch is
    /// kept until no client of the user listens anymore.
    pub(crate) fn clear_client_state(&self, client_id: QsClientId) {
        let Some(user_clients) = self.listening_user_clients(client_id) else {
            return;
        };
        let mut clients = user_clients.lock();
        let Some(entry) = clients.remove(&client_id) else {
            return;
        };
        if entry.session.and_then(|session| session.state).is_some() {
            fan_out_removed(&clients, client_id, entry.epoch + 1);
        }
    }

    /// Returns the states of the listening siblings of `client_id`.
    pub(crate) fn sibling_client_states(&self, client_id: QsClientId) -> Vec<ListenResponse> {
        let Some(user_clients) = self.listening_user_clients(client_id) else {
            return Vec::new();
        };
        let clients = user_clients.lock();
        clients
            .iter()
            .filter(|(id, _)| **id != client_id)
            .filter_map(|(id, entry)| Some(entry.session.as_ref()?.state.as_ref()?.updated(*id)))
            .map(|updated| client_state_response(sibling_client_state::Change::Updated(updated)))
            .collect()
    }
}

/// Tells the listening siblings of `client_id` that its state is gone.
fn fan_out_removed(clients: &HashMap<QsClientId, ClientEntry>, client_id: QsClientId, epoch: u64) {
    let removed = SiblingClientStateRemoved {
        client_id: Some(client_id.into()),
        epoch,
        updated_at: Some(TimeStamp::now().into()),
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
    use std::{pin::pin, time::Duration};

    use futures_util::Stream;
    use sqlx::PgPool;
    use tokio::time::timeout;
    use tokio_stream::StreamExt;
    use tokio_util::sync::CancellationToken;

    use crate::qs::{
        client_record::persistence::tests::store_random_client_record,
        user_record::persistence::tests::store_random_user_record,
    };

    use super::*;

    const STREAM_NEXT_TIMEOUT: Duration = Duration::from_secs(1);
    const NO_EVENT_TIMEOUT: Duration = Duration::from_millis(200);

    async fn next_client_state(
        stream: &mut (impl Stream<Item = Option<ListenResponse>> + Unpin),
    ) -> sibling_client_state::Change {
        loop {
            let response = timeout(STREAM_NEXT_TIMEOUT, stream.next())
                .await
                .expect("timeout waiting for client state")
                .expect("stream ended")
                .expect("missing response");
            if let Some(listen_response::Event::SiblingClientState(SiblingClientState {
                change: Some(change),
            })) = response.event
            {
                return change;
            }
        }
    }

    async fn assert_no_client_state(
        stream: &mut (impl Stream<Item = Option<ListenResponse>> + Unpin),
    ) {
        while let Ok(response) = timeout(NO_EVENT_TIMEOUT, stream.next()).await {
            let response = response.expect("stream ended").expect("missing response");
            assert!(
                !matches!(
                    response.event,
                    Some(listen_response::Event::SiblingClientState(_))
                ),
                "unexpected client state: {response:?}"
            );
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

    #[sqlx::test]
    async fn fan_out_reaches_listening_siblings(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let other_user = store_random_user_record(&pool).await?;
        let a = store_random_client_record(&pool, user.user_id).await?;
        let b = store_random_client_record(&pool, user.user_id).await?;
        let other = store_random_client_record(&pool, other_user.user_id).await?;

        let queues = Queues::new(pool.clone(), CancellationToken::new()).await?;
        let (a_session, a_stream) = queues.listen(a.client_id, None, 0).await?;
        let mut a_stream = pin!(a_stream);
        let (_, b_stream) = queues.listen(b.client_id, None, 0).await?;
        let mut b_stream = pin!(b_stream);
        let (_, other_stream) = queues.listen(other.client_id, None, 0).await?;
        let mut other_stream = pin!(other_stream);

        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        let updated = updated_of(next_client_state(&mut b_stream).await);
        assert_eq!(client_id_of(updated.client_id), a.client_id);
        assert_eq!(updated.blob.unwrap().encrypted_blob, b"state");
        assert_eq!(updated.age_ms, 0);
        assert_no_client_state(&mut a_stream).await;
        assert_no_client_state(&mut other_stream).await;

        // A late listener gets the current states, aged since they were
        // received.
        tokio::time::sleep(Duration::from_millis(10)).await;
        let snapshot = queues.sibling_client_states(b.client_id);
        assert_eq!(snapshot.len(), 1);
        let Some(listen_response::Event::SiblingClientState(SiblingClientState {
            change: Some(change),
        })) = snapshot.into_iter().next().unwrap().event
        else {
            panic!("expected a client state");
        };
        let snapshot = updated_of(change);
        assert_eq!(snapshot.epoch, updated.epoch);
        assert!(snapshot.age_ms >= 10);
        assert!(queues.sibling_client_states(a.client_id).is_empty());

        // Another session neither reports nor clears.
        let stale_session = a_session + 1;
        queues.update_client_state(a.client_id, stale_session, b"stale".to_vec());
        queues.end_client_session(a.client_id, stale_session);
        assert_no_client_state(&mut b_stream).await;

        queues.end_client_session(a.client_id, a_session);
        let removed = removed_of(next_client_state(&mut b_stream).await);
        assert_eq!(client_id_of(removed.client_id), a.client_id);
        assert!(removed.epoch > updated.epoch);
        assert!(queues.sibling_client_states(b.client_id).is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn replacing_the_listener_clears_the_client_state(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let a = store_random_client_record(&pool, user.user_id).await?;
        let b = store_random_client_record(&pool, user.user_id).await?;

        let queues = Queues::new(pool.clone(), CancellationToken::new()).await?;
        let (a_session, _a_stream) = queues.listen(a.client_id, None, 0).await?;
        let (b_session, b_stream) = queues.listen(b.client_id, None, 0).await?;
        let mut b_stream = pin!(b_stream);

        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        let updated = updated_of(next_client_state(&mut b_stream).await);
        assert_eq!(updated.epoch, 1);

        let (new_a_session, a_stream) = queues.listen(a.client_id, None, 0).await?;
        let mut a_stream = pin!(a_stream);
        assert_ne!(new_a_session, a_session);
        let removed = removed_of(next_client_state(&mut b_stream).await);
        assert_eq!(client_id_of(removed.client_id), a.client_id);
        assert_eq!(removed.epoch, 2);

        // The evicted session ending afterwards does not repeat it.
        queues.end_client_session(a.client_id, a_session);
        assert_no_client_state(&mut b_stream).await;

        // The new session continues the epochs of the replaced one.
        queues.update_client_state(a.client_id, new_a_session, b"state".to_vec());
        assert_eq!(updated_of(next_client_state(&mut b_stream).await).epoch, 3);

        // Epochs are per client.
        queues.update_client_state(b.client_id, b_session, b"state".to_vec());
        assert_eq!(updated_of(next_client_state(&mut a_stream).await).epoch, 1);

        Ok(())
    }

    #[sqlx::test]
    async fn epochs_survive_ended_listeners(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let a = store_random_client_record(&pool, user.user_id).await?;
        let b = store_random_client_record(&pool, user.user_id).await?;

        let queues = Queues::new(pool.clone(), CancellationToken::new()).await?;
        let (_, b_stream) = queues.listen(b.client_id, None, 0).await?;
        let mut b_stream = pin!(b_stream);

        let (a_session, a_stream) = queues.listen(a.client_id, None, 0).await?;
        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        assert_eq!(updated_of(next_client_state(&mut b_stream).await).epoch, 1);
        queues.end_client_session(a.client_id, a_session);
        assert_eq!(removed_of(next_client_state(&mut b_stream).await).epoch, 2);

        // The ended listener of a is swept when c starts listening.
        drop(a_stream);
        let c = store_random_client_record(&pool, user.user_id).await?;
        let _c_stream = queues.listen(c.client_id, None, 0).await?;

        let (a_session, _a_stream) = queues.listen(a.client_id, None, 0).await?;
        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        assert_eq!(updated_of(next_client_state(&mut b_stream).await).epoch, 3);

        Ok(())
    }

    #[sqlx::test]
    async fn fan_out_reaches_siblings_created_after_listening(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let a = store_random_client_record(&pool, user.user_id).await?;

        let queues = Queues::new(pool.clone(), CancellationToken::new()).await?;
        let (a_session, _a_stream) = queues.listen(a.client_id, None, 0).await?;

        let b = store_random_client_record(&pool, user.user_id).await?;
        let (_, b_stream) = queues.listen(b.client_id, None, 0).await?;
        let mut b_stream = pin!(b_stream);

        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        let updated = updated_of(next_client_state(&mut b_stream).await);
        assert_eq!(client_id_of(updated.client_id), a.client_id);
        assert_eq!(updated.blob.unwrap().encrypted_blob, b"state");

        Ok(())
    }
}
