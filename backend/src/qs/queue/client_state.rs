// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! States of listening clients, relayed to their sibling clients.
//!
//! The state is an opaque blob kept next to the client's listener. It lives
//! until the listen session that reported it ends or is replaced as well as
//! when the client's queue is deleted.

use std::time::Instant;

use aircommon::{identifiers::QsClientId, time::TimeStamp};
use airprotos::queue_service::v1::{
    ListenResponse, SiblingClientState, SiblingClientStateEncryptedBlob, SiblingClientStateRemoved,
    SiblingClientStateUpdated, listen_response, sibling_client_state,
};
use tracing::debug;
use uuid::Uuid;

use crate::qs::queue::Queues;

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

impl Queues {
    /// Returns the epoch of the next state change of `client_id`, starting at
    /// 1.
    ///
    /// Callers holding a listener entry must lock it before the epoch entry,
    /// like everywhere else.
    pub(super) fn next_client_state_epoch(&self, client_id: QsClientId) -> u64 {
        let mut epoch = self.client_state_epochs.entry(client_id).or_default();
        *epoch += 1;
        *epoch
    }

    /// Stores the state of `client_id` if `session_id` is its current listen
    /// session, and relays it to the listening siblings.
    pub(crate) fn update_client_state(
        &self,
        client_id: QsClientId,
        session_id: Uuid,
        encrypted_blob: Vec<u8>,
    ) {
        let Some(mut context) = self
            .listeners
            .get_mut(&client_id)
            // important: only get the context for the same session
            .filter(|context| context.session_id == session_id)
        else {
            return;
        };

        let state = ClientState {
            epoch: self.next_client_state_epoch(client_id),
            received_at: Instant::now(),
            encrypted_blob,
        };
        let updated = state.updated(client_id);
        context.client_state = Some(state);
        // Release the entry before fanning out, since a sibling can be in the
        // same shard of the map.
        let siblings = context.siblings.clone();
        drop(context);

        self.fan_out_client_state(&siblings, sibling_client_state::Change::Updated(updated));
    }

    /// Tells the siblings that the state of `client_id` is gone and should be
    /// discarded.
    pub(super) fn report_client_state_removed(
        &self,
        client_id: QsClientId,
        siblings: &[QsClientId],
        epoch: u64,
    ) {
        let removed = SiblingClientStateRemoved {
            client_id: Some(client_id.into()),
            epoch,
            updated_at: Some(TimeStamp::now().into()),
        };
        self.fan_out_client_state(siblings, sibling_client_state::Change::Removed(removed));
    }

    /// Clears the state of `client_id` if `session_id` is its current listen
    /// session, or regardless of the session if `None`.
    ///
    /// Returns the siblings of `client_id` and the epoch of the change, if
    /// there was a state.
    fn take_client_state(
        &self,
        client_id: QsClientId,
        session_id: Option<Uuid>,
    ) -> Option<(Vec<QsClientId>, u64)> {
        let mut context = self.listeners.get_mut(&client_id)?;
        if session_id.is_some_and(|session_id| session_id != context.session_id) {
            return None;
        }
        context.client_state.take()?;
        Some((
            context.siblings.clone(),
            self.next_client_state_epoch(client_id),
        ))
    }

    /// Clears the state of `client_id` reported in `session_id` and fans out
    /// the change to its siblings.
    pub(crate) fn clear_session_client_state(&self, client_id: QsClientId, session_id: Uuid) {
        if let Some((siblings, epoch)) = self.take_client_state(client_id, Some(session_id)) {
            self.report_client_state_removed(client_id, &siblings, epoch);
        }
    }

    /// Clears the state of `client_id` reported in any session and fans out
    /// the change to its siblings.
    ///
    /// Used when the client is deleted, so its epoch is dropped as well.
    pub(crate) fn clear_client_state(&self, client_id: QsClientId) {
        if let Some((siblings, epoch)) = self.take_client_state(client_id, None) {
            self.report_client_state_removed(client_id, &siblings, epoch);
        }
        self.client_state_epochs.remove(&client_id);
    }

    /// Returns the states of the listening siblings of `client_id`.
    pub(crate) fn sibling_client_states(&self, client_id: QsClientId) -> Vec<ListenResponse> {
        // TODO(gabriel): maybe a better data-structure would help us avoid doing that?
        let siblings = self
            .listeners
            .get(&client_id)
            .map(|context| context.siblings.clone())
            .unwrap_or_default();

        siblings
            .into_iter()
            .filter_map(|id| {
                let context = self.listeners.get(&id)?;
                Some(context.client_state.as_ref()?.updated(id))
            })
            .map(|updated| client_state_response(sibling_client_state::Change::Updated(updated)))
            .collect()
    }

    fn fan_out_client_state(&self, siblings: &[QsClientId], change: sibling_client_state::Change) {
        let response = client_state_response(change);
        for &sibling_id in siblings {
            if !self.try_send_response(sibling_id, response.clone()) {
                debug!(?sibling_id, "client state not relayed to sibling");
            }
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
        let a_session = Uuid::new_v4();
        let mut a_stream = pin!(queues.listen(a_session, a.client_id, None, 0).await?);
        let mut b_stream = pin!(queues.listen(Uuid::new_v4(), b.client_id, None, 0).await?);
        let mut other_stream = pin!(
            queues
                .listen(Uuid::new_v4(), other.client_id, None, 0)
                .await?
        );

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
        let stale_session = Uuid::new_v4();
        queues.update_client_state(a.client_id, stale_session, b"stale".to_vec());
        queues.clear_session_client_state(a.client_id, stale_session);
        assert_no_client_state(&mut b_stream).await;

        queues.clear_session_client_state(a.client_id, a_session);
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
        let a_session = Uuid::new_v4();
        let _a_stream = queues.listen(a_session, a.client_id, None, 0).await?;
        let b_session = Uuid::new_v4();
        let mut b_stream = pin!(queues.listen(b_session, b.client_id, None, 0).await?);

        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        let updated = updated_of(next_client_state(&mut b_stream).await);
        assert_eq!(updated.epoch, 1);

        let new_a_session = Uuid::new_v4();
        let mut a_stream = pin!(queues.listen(new_a_session, a.client_id, None, 0).await?);
        let removed = removed_of(next_client_state(&mut b_stream).await);
        assert_eq!(client_id_of(removed.client_id), a.client_id);
        assert_eq!(removed.epoch, 2);

        // The evicted session ending afterwards does not repeat it.
        queues.clear_session_client_state(a.client_id, a_session);
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
        let mut b_stream = pin!(queues.listen(Uuid::new_v4(), b.client_id, None, 0).await?);

        let a_session = Uuid::new_v4();
        let a_stream = queues.listen(a_session, a.client_id, None, 0).await?;
        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        assert_eq!(updated_of(next_client_state(&mut b_stream).await).epoch, 1);
        queues.clear_session_client_state(a.client_id, a_session);
        assert_eq!(removed_of(next_client_state(&mut b_stream).await).epoch, 2);

        // The ended listener of a is swept when c starts listening.
        drop(a_stream);
        let c = store_random_client_record(&pool, user.user_id).await?;
        let _c_stream = queues.listen(Uuid::new_v4(), c.client_id, None, 0).await?;

        let a_session = Uuid::new_v4();
        let _a_stream = queues.listen(a_session, a.client_id, None, 0).await?;
        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        assert_eq!(updated_of(next_client_state(&mut b_stream).await).epoch, 3);

        Ok(())
    }

    #[sqlx::test]
    async fn fan_out_reaches_siblings_created_after_listening(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let a = store_random_client_record(&pool, user.user_id).await?;

        let queues = Queues::new(pool.clone(), CancellationToken::new()).await?;
        let a_session = Uuid::new_v4();
        let _a_stream = queues.listen(a_session, a.client_id, None, 0).await?;

        let b = store_random_client_record(&pool, user.user_id).await?;
        let mut b_stream = pin!(queues.listen(Uuid::new_v4(), b.client_id, None, 0).await?);

        queues.update_client_state(a.client_id, a_session, b"state".to_vec());
        let updated = updated_of(next_client_state(&mut b_stream).await);
        assert_eq!(client_id_of(updated.client_id), a.client_id);
        assert_eq!(updated.blob.unwrap().encrypted_blob, b"state");

        Ok(())
    }
}
