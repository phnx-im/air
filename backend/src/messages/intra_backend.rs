// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs and enums that represent messages that are
//! passed internally within the backend.

use aircommon::{
    identifiers::QsReference,
    messages::client_ds::{DsEventMessage, QsQueueMessagePayload},
    time::TimeStamp,
};
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes};

// === DS to QS ===

pub type QsInputMessage = DsFanOutMessage;

#[derive(Clone, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum TlsBool {
    True = 1,
    False = 0,
}

impl From<bool> for TlsBool {
    fn from(value: bool) -> Self {
        if value { TlsBool::True } else { TlsBool::False }
    }
}

impl From<TlsBool> for bool {
    fn from(value: TlsBool) -> Self {
        matches!(value, TlsBool::True)
    }
}

#[derive(Clone, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct DsFanOutMessage {
    pub payload: DsFanOutPayload,
    pub client_reference: QsReference,
    pub suppress_notifications: TlsBool,
    pub virtual_client_action: Option<VirtualClientAction>,
}

#[derive(Clone, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum DsFanOutPayload {
    QueueMessage(QsQueueMessagePayload),
    EventMessage(DsEventMessage),
}

impl DsFanOutPayload {
    pub(crate) fn timestamp(&self) -> TimeStamp {
        match self {
            DsFanOutPayload::QueueMessage(payload) => payload.timestamp,
            DsFanOutPayload::EventMessage(payload) => payload.timestamp,
        }
    }
}

impl<T: Into<QsQueueMessagePayload>> From<T> for DsFanOutPayload {
    fn from(value: T) -> Self {
        Self::QueueMessage(value.into())
    }
}

#[derive(Clone, TlsSerialize, TlsDeserializeBytes, TlsSize)]
#[repr(u8)]
pub enum VirtualClientAction {
    /// Per [Draft] §5.5.1
    ///
    /// [Draft]: https://datatracker.ietf.org/doc/draft-kohbrok-mls-virtual-clients/
    #[tls_codec(discriminant = 1)]
    PromoteStagedKeyPackages { epoch_id: VLBytes, random: VLBytes },
    // Future: ExternalJoin { group_id: VLBytes } per draft §5.5.2  (discriminant = 2)
}
