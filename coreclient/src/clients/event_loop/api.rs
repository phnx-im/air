// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The public API of the event loop exposed by the [`CoreUser`].
//!
//! Translates async methods into message passing.

use airapiclient::qs_api::QsListenResponder;
use aircommon::identifiers::UserHandle;
use airprotos::{auth_service::v1::HandleQueueMessage, queue_service::v1::QueueEvent};

use crate::{
    ChatId,
    clients::{
        CoreUser,
        event_loop::{ClientOperation, RemoteQueueEvent},
        process::process_qs::QsProcessEventResult,
    },
};

impl CoreUser {
    /// Process a queue message received from the AS handle queue.
    ///
    /// Returns the [`ChatId`] of any newly created chat.
    pub async fn process_handle_queue_message(
        &self,
        user_handle: UserHandle,
        handle_queue_message: HandleQueueMessage,
    ) -> anyhow::Result<ChatId> {
        let (message, response) =
            RemoteQueueEvent::handle_queue_message(user_handle, handle_queue_message);
        self.inner
            .event_loop_sender
            .send_remote_queue_event(message)
            .await;
        response.await.map_err(Into::into)
    }

    /// Process a queue event received from the QS queue.
    ///
    /// Returns the [`QsProcessEventResult`] of the event which disambiguates whether the event was
    /// fully processed or partially processed, ignored or accumulated.
    pub async fn process_qs_event(
        &self,
        event: QueueEvent,
    ) -> anyhow::Result<QsProcessEventResult> {
        let (event, response) = RemoteQueueEvent::qs_event(event);
        let _ = self
            .inner
            .event_loop_sender
            .send_remote_queue_event(event)
            .await;
        response.await.map_err(Into::into)
    }

    /// Replace the QS listen responder.
    ///
    /// This is used to replace the QS listen responder after a new QS listen connection was
    /// established.
    pub async fn replace_qs_listen_responder(&self, responder: QsListenResponder) {
        let _ = self
            .inner
            .event_loop_sender
            .send_client_operation(ClientOperation::ReplaceQsListenResponder(responder))
            .await;
    }
}
