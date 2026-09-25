// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Focused chats of listening clients, relayed to their sibling clients.
//!
//! The focused chat is an opaque blob kept next to the client's listener. It
//! lives until the listen session that reported it ends or is replaced as
//! well as when the client's queue is deleted.

use aircommon::{identifiers::QsClientId, time::TimeStamp};
use airprotos::queue_service::v1::{
    ListenResponse, SiblingFocusedChat, SiblingFocusedChatGone, SiblingFocusedChatState,
    listen_response, sibling_focused_chat,
};
use tracing::debug;
use uuid::Uuid;

use crate::qs::queue::Queues;

pub(crate) const MAX_ENCRYPTED_FOCUSED_CHAT_SIZE: usize = 256;

impl Queues {
    /// Stores the focused chat of `client_id` if `session_id` is its current
    /// listen session.
    ///
    /// Returns the siblings of `client_id` and the stored focused chat with its
    /// `updated_at` set, if it was stored.
    pub(crate) fn update_focused_chat(
        &self,
        client_id: QsClientId,
        session_id: Uuid,
        encrypted_focused_chat: Vec<u8>,
    ) {
        let mut focused_chat_state = SiblingFocusedChatState {
            client_id: Some(client_id.into()),
            updated_at: None, // set when stored
            encrypted_focused_chat,
        };

        let Some(mut context) = self
            .listeners
            .get_mut(&client_id)
            // important: only get the context for the same session
            .filter(|context| context.session_id == session_id)
        else {
            return;
        };

        focused_chat_state.updated_at = Some(TimeStamp::now().into());
        context.focused_chat = Some(focused_chat_state.clone());

        self.fan_out_focused_chat(
            &context.siblings,
            sibling_focused_chat::Change::Update(focused_chat_state),
        );
    }

    /// Tell sibling clients that the origin of a focused chat is gone and
    /// its state should be discarded.
    pub(super) fn report_focused_chat_gone(
        &self,
        client_id: QsClientId,
        siblings: &[QsClientId],
        gone_at: TimeStamp,
    ) {
        let gone = SiblingFocusedChatGone {
            client_id: Some(client_id.into()),
            updated_at: Some(gone_at.into()),
        };
        self.fan_out_focused_chat(siblings, sibling_focused_chat::Change::Gone(gone));
    }

    /// Clears the focused chat of `client_id` if `session_id` is its current
    /// listen session, or regardless of the session if `None`.
    ///
    /// Returns the siblings of `client_id` and the time of the change, if there
    /// was a focused chat.
    pub(super) fn clear_focused_chat(
        &self,
        client_id: QsClientId,
        session_id: Option<Uuid>,
    ) -> Option<(Vec<QsClientId>, TimeStamp)> {
        let mut context = self.listeners.get_mut(&client_id)?;
        if session_id.is_some_and(|session_id| session_id != context.session_id) {
            return None;
        }
        context.focused_chat.take()?;
        Some((context.siblings.clone(), TimeStamp::now()))
    }

    /// Clears the focused chat of `client_id` reported in `session_id` and
    /// fans out the change to its siblings.
    pub(crate) fn clear_session_focused_chat(&self, client_id: QsClientId, session_id: Uuid) {
        if let Some((siblings, gone_at)) = self.clear_focused_chat(client_id, Some(session_id)) {
            self.report_focused_chat_gone(client_id, &siblings, gone_at);
        }
    }

    /// Clears the focused chat of `client_id` reported in any session and fans
    /// out the change to its siblings.
    pub(crate) fn clear_client_focused_chat(&self, client_id: QsClientId) {
        if let Some((siblings, gone_at)) = self.clear_focused_chat(client_id, None) {
            self.report_focused_chat_gone(client_id, &siblings, gone_at);
        }
    }

    /// Returns the focused chats of the listening siblings of `client_id`.
    pub(crate) fn sibling_focused_chats(&self, client_id: QsClientId) -> Vec<ListenResponse> {
        // TODO(gabriel): maybe a better data-structure would help us avoid doing that?
        let siblings = self
            .listeners
            .get(&client_id)
            .map(|context| context.siblings.clone())
            .unwrap_or_default();

        siblings
            .into_iter()
            .filter_map(|id| self.listeners.get(&id)?.focused_chat.clone())
            .map(|state| focused_chat_response(sibling_focused_chat::Change::Update(state)))
            .collect()
    }

    fn fan_out_focused_chat(&self, siblings: &[QsClientId], change: sibling_focused_chat::Change) {
        let response = focused_chat_response(change);
        for &sibling_id in siblings {
            if !self.try_send_response(sibling_id, response.clone()) {
                debug!(?sibling_id, "focused chat not relayed to sibling");
            }
        }
    }
}

fn focused_chat_response(change: sibling_focused_chat::Change) -> ListenResponse {
    ListenResponse {
        event: Some(listen_response::Event::SiblingFocusedChat(
            SiblingFocusedChat {
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

    async fn next_focused_chat(
        stream: &mut (impl Stream<Item = Option<ListenResponse>> + Unpin),
    ) -> sibling_focused_chat::Change {
        loop {
            let response = timeout(STREAM_NEXT_TIMEOUT, stream.next())
                .await
                .expect("timeout waiting for focused chat")
                .expect("stream ended")
                .expect("missing response");
            if let Some(listen_response::Event::SiblingFocusedChat(SiblingFocusedChat {
                change: Some(change),
            })) = response.event
            {
                return change;
            }
        }
    }

    async fn assert_no_focused_chat(
        stream: &mut (impl Stream<Item = Option<ListenResponse>> + Unpin),
    ) {
        while let Ok(response) = timeout(NO_EVENT_TIMEOUT, stream.next()).await {
            let response = response.expect("stream ended").expect("missing response");
            assert!(
                !matches!(
                    response.event,
                    Some(listen_response::Event::SiblingFocusedChat(_))
                ),
                "unexpected focused chat: {response:?}"
            );
        }
    }

    fn update_of(change: sibling_focused_chat::Change) -> (QsClientId, Vec<u8>) {
        let sibling_focused_chat::Change::Update(update) = change else {
            panic!("expected update, got {change:?}");
        };
        let client_id = update.client_id.unwrap().try_into().unwrap();
        (client_id, update.encrypted_focused_chat)
    }

    fn gone_of(change: sibling_focused_chat::Change) -> QsClientId {
        let sibling_focused_chat::Change::Gone(gone) = change else {
            panic!("expected gone, got {change:?}");
        };
        gone.client_id.unwrap().try_into().unwrap()
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

        queues.update_focused_chat(a.client_id, a_session, b"focused".to_vec());
        assert_eq!(
            update_of(next_focused_chat(&mut b_stream).await),
            (a.client_id, b"focused".to_vec())
        );
        assert_no_focused_chat(&mut a_stream).await;
        assert_no_focused_chat(&mut other_stream).await;

        // A late listener gets the current focused chats.
        assert_eq!(queues.sibling_focused_chats(b.client_id).len(), 1);
        assert!(queues.sibling_focused_chats(a.client_id).is_empty());

        // Another session neither reports nor clears.
        let stale_session = Uuid::new_v4();
        queues.update_focused_chat(a.client_id, stale_session, b"stale".to_vec());
        queues.clear_session_focused_chat(a.client_id, stale_session);
        assert_no_focused_chat(&mut b_stream).await;

        queues.clear_session_focused_chat(a.client_id, a_session);
        assert_eq!(gone_of(next_focused_chat(&mut b_stream).await), a.client_id);
        assert!(queues.sibling_focused_chats(b.client_id).is_empty());

        Ok(())
    }

    #[sqlx::test]
    async fn replacing_the_listener_clears_the_focused_chat(pool: PgPool) -> anyhow::Result<()> {
        let user = store_random_user_record(&pool).await?;
        let a = store_random_client_record(&pool, user.user_id).await?;
        let b = store_random_client_record(&pool, user.user_id).await?;

        let queues = Queues::new(pool.clone(), CancellationToken::new()).await?;
        let a_session = Uuid::new_v4();
        let _a_stream = queues.listen(a_session, a.client_id, None, 0).await?;
        let mut b_stream = pin!(queues.listen(Uuid::new_v4(), b.client_id, None, 0).await?);

        queues.update_focused_chat(a.client_id, a_session, b"focused".to_vec());
        let sibling_focused_chat::Change::Update(update) = next_focused_chat(&mut b_stream).await
        else {
            panic!("expected update");
        };

        let _a_stream = queues.listen(Uuid::new_v4(), a.client_id, None, 0).await?;
        let sibling_focused_chat::Change::Gone(gone) = next_focused_chat(&mut b_stream).await
        else {
            panic!("expected gone");
        };
        let gone_at = TimeStamp::from(gone.updated_at.unwrap());
        let updated_at = TimeStamp::from(update.updated_at.unwrap());
        assert!(gone_at.as_ref() > updated_at.as_ref());
        assert_eq!(
            gone_of(sibling_focused_chat::Change::Gone(gone)),
            a.client_id
        );

        // The evicted session ending afterwards does not repeat it.
        queues.clear_session_focused_chat(a.client_id, a_session);
        assert_no_focused_chat(&mut b_stream).await;

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

        queues.update_focused_chat(a.client_id, a_session, b"focused".to_vec());
        assert_eq!(
            update_of(next_focused_chat(&mut b_stream).await),
            (a.client_id, b"focused".to_vec())
        );

        Ok(())
    }
}
