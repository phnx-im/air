// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains structs and enums that represent messages that are
//! passed internally within the backend.

use aircommon::{
    identifiers::QsReference,
    messages::client_ds::{DsEventMessage, QsQueueMessagePayload},
    time::TimeStamp,
    virtual_client::KeyPackageBatchId,
};
use mls_assist::openmls::components::vc_derivation_info::KeyPackageUpload;
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

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
    pub broadcast_to_all_client_queues: TlsBool,
    pub virtual_client_hint: Option<QsVirtualClientHint>,
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
pub enum QsVirtualClientHint {
    /// DS -> QS hint to promote a staged batch of key packages to live.
    ///
    /// This is a backend-internal hint, distinct from the draft's client-to-client
    /// `VirtualClientAction` SafeAAD struct.
    #[tls_codec(discriminant = 1)]
    PromoteStagedKeyPackages(KeyPackageBatchId),
}

impl From<KeyPackageUpload> for QsVirtualClientHint {
    fn from(value: KeyPackageUpload) -> Self {
        Self::PromoteStagedKeyPackages(KeyPackageBatchId {
            epoch_id: value.epoch_id,
            leaf_index: value.leaf_index,
            generation: value.generation,
        })
    }
}
