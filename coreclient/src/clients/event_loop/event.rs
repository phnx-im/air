// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Event types used for message passing between the `CoreUser` and the `EventLoop`.

use std::convert::Infallible;

use airapiclient::qs_api::QsListenResponder;
use aircommon::identifiers::Username;
use airprotos::{auth_service::v1::HandleQueueMessage, queue_service::v1::QueueEvent};

use crate::{
    ChatId,
    clients::{
        event_loop::{
            responder,
            response::{Responder, Response},
        },
        process::process_qs::QsProcessEventResult,
    },
};

/// Incoming event from a remote queue.
///
/// The remote queue is either the QS queue or the AS username queue.
pub(super) enum RemoteQueueEvent {
    Qs {
        event: QueueEvent,
        responder: Responder<QsProcessEventResult, Infallible>,
    },
    Username {
        username: Username,
        message: HandleQueueMessage,
        responder: Responder<ChatId, Infallible>,
    },
}

impl RemoteQueueEvent {
    /// Helper function for creating a [`RemoteQueueEvent::Qs`] message.
    pub(super) fn qs_event(
        event: QueueEvent,
    ) -> (Self, Response<QsProcessEventResult, Infallible>) {
        let (responder, response) = responder();
        let message = Self::Qs { event, responder };
        (message, response)
    }

    /// Helper function for creating a [`RemoteQueueEvent::Username`] message.
    pub(super) fn username_queue_message(
        username: Username,
        message: HandleQueueMessage,
    ) -> (Self, Response<ChatId, Infallible>) {
        let (responder, response) = responder();
        let message = Self::Username {
            username,
            message,
            responder,
        };
        (message, response)
    }
}

/// Incoming event from the client.
pub enum ClientOperation {
    ReplaceQsListenResponder(QsListenResponder),
}
