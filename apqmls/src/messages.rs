// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::{
    group::GroupEpoch,
    prelude::{
        Ciphersuite, Credential, KeyPackage, KeyPackageIn, MlsMessageBodyIn, MlsMessageIn,
        MlsMessageOut, OpenMlsCrypto, OpenMlsSignaturePublicKey, ProtocolMessage, ProtocolVersion,
        PublicMessageIn, RatchetTreeIn, SignatureError, Verifiable as _, Welcome,
        group_info::{GroupInfo, VerifiableGroupInfo},
    },
    treesync::RatchetTree,
};
use serde::{Deserialize, Serialize};
use tls_codec::{
    Deserialize as _, Serialize as _, TlsDeserialize, TlsDeserializeBytes, TlsSerialize, TlsSize,
};

use crate::{ApqGroupId, authentication::ApqVerifyingKey, extension::PqtMode};

/// An incoming message for processing by an [`crate::ApqMlsGroup`].
#[derive(Debug, Clone, TlsDeserialize, TlsDeserializeBytes, TlsSize)]
pub struct ApqMlsMessageIn {
    pub(crate) t_message: MlsMessageIn,
    pub(crate) pq_message: MlsMessageIn,
}

impl ApqMlsMessageIn {
    pub fn t_message(&self) -> &MlsMessageIn {
        &self.t_message
    }

    pub fn pq_message(&self) -> &MlsMessageIn {
        &self.pq_message
    }

    pub fn into_welcome(self) -> Option<ApqWelcome> {
        let MlsMessageBodyIn::Welcome(t_welcome) = self.t_message.extract() else {
            return None;
        };
        let MlsMessageBodyIn::Welcome(pq_welcome) = self.pq_message.extract() else {
            return None;
        };
        Some(ApqWelcome {
            t_welcome,
            pq_welcome,
        })
    }

    pub fn into_key_package(self) -> Option<ApqKeyPackageIn> {
        let MlsMessageBodyIn::KeyPackage(t_key_package) = self.t_message.extract() else {
            return None;
        };
        let MlsMessageBodyIn::KeyPackage(pq_key_package) = self.pq_message.extract() else {
            return None;
        };
        Some(ApqKeyPackageIn {
            t_key_package,
            pq_key_package,
        })
    }

    pub fn into_verifiable_group_info(self) -> Option<VerifiableApqGroupInfo> {
        let MlsMessageBodyIn::GroupInfo(t_group_info) = self.t_message.extract() else {
            return None;
        };
        let MlsMessageBodyIn::GroupInfo(pq_group_info) = self.pq_message.extract() else {
            return None;
        };
        Some(VerifiableApqGroupInfo {
            t_group_info,
            pq_group_info,
        })
    }

    pub fn into_protocol_message(self) -> Option<ApqProtocolMessage> {
        let t_protocol_message = self.t_message.try_into_protocol_message().ok()?;
        let pq_protocol_message = self.pq_message.try_into_protocol_message().ok()?;
        Some(ApqProtocolMessage {
            t_protocol_message,
            pq_protocol_message,
        })
    }
}

pub struct ApqProtocolMessage {
    pub(crate) t_protocol_message: ProtocolMessage,
    pub(crate) pq_protocol_message: ProtocolMessage,
}

impl ApqProtocolMessage {
    pub fn new(t_protocol_message: ProtocolMessage, pq_protocol_message: ProtocolMessage) -> Self {
        Self {
            t_protocol_message,
            pq_protocol_message,
        }
    }

    pub fn group_id(&self) -> ApqGroupId {
        ApqGroupId {
            t_group_id: self.t_protocol_message.group_id().clone(),
            pq_group_id: self.pq_protocol_message.group_id().clone(),
        }
    }

    pub fn t_epoch(&self) -> GroupEpoch {
        self.t_protocol_message.epoch()
    }

    pub fn pq_epoch(&self) -> GroupEpoch {
        self.pq_protocol_message.epoch()
    }
}

/// An outgoing message from an [`crate::ApqMlsGroup`].
#[derive(Debug, Clone, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct ApqMlsMessageOut {
    pub(crate) t_message: MlsMessageOut,
    pub(crate) pq_message: MlsMessageOut,
}

impl ApqMlsMessageOut {
    pub fn split(self) -> (MlsMessageOut, MlsMessageOut) {
        (self.t_message, self.pq_message)
    }
}

impl TryFrom<ApqMlsMessageOut> for ApqMlsMessageIn {
    type Error = tls_codec::Error;

    fn try_from(value: ApqMlsMessageOut) -> Result<Self, Self::Error> {
        let serialied_t_message = value.t_message.tls_serialize_detached()?;
        let serialized_pq_message = value.pq_message.tls_serialize_detached()?;
        let t_message_in = MlsMessageIn::tls_deserialize_exact(&serialied_t_message)?;
        let pq_message_in = MlsMessageIn::tls_deserialize_exact(&serialized_pq_message)?;
        Ok(ApqMlsMessageIn {
            t_message: t_message_in,
            pq_message: pq_message_in,
        })
    }
}

/// A welcome message for joining an [`crate::ApqMlsGroup`].
#[derive(Debug, Clone, TlsSerialize, TlsDeserializeBytes, TlsSize, Serialize, Deserialize)]
pub struct ApqWelcome {
    pub(crate) t_welcome: Welcome,
    pub(crate) pq_welcome: Welcome,
}

impl ApqWelcome {
    pub fn new(t_welcome: Welcome, pq_welcome: Welcome) -> Self {
        Self {
            t_welcome,
            pq_welcome,
        }
    }

    pub fn split(self) -> (Welcome, Welcome) {
        let Self {
            t_welcome,
            pq_welcome,
        } = self;
        (t_welcome, pq_welcome)
    }
}

impl From<ApqWelcome> for ApqMlsMessageOut {
    fn from(value: ApqWelcome) -> Self {
        ApqMlsMessageOut {
            t_message: MlsMessageOut::from_welcome(value.t_welcome, ProtocolVersion::default()),
            pq_message: MlsMessageOut::from_welcome(value.pq_welcome, ProtocolVersion::default()),
        }
    }
}

/// A ratchet tree for an [`crate::ApqMlsGroup`].
#[derive(Clone)]
pub struct ApqRatchetTree {
    pub(crate) t_ratchet_tree: RatchetTree,
    pub(crate) pq_ratchet_tree: RatchetTree,
}

impl From<ApqRatchetTree> for ApqRatchetTreeIn {
    fn from(value: ApqRatchetTree) -> Self {
        ApqRatchetTreeIn {
            t_ratchet_tree: value.t_ratchet_tree.into(),
            pq_ratchet_tree: value.pq_ratchet_tree.into(),
        }
    }
}

/// An unverified ratchet tree for an [`crate::ApqMlsGroup`].
pub struct ApqRatchetTreeIn {
    pub(crate) t_ratchet_tree: RatchetTreeIn,
    pub(crate) pq_ratchet_tree: RatchetTreeIn,
}

impl ApqRatchetTreeIn {
    pub fn new(t_ratchet_tree: RatchetTreeIn, pq_ratchet_tree: RatchetTreeIn) -> Self {
        Self {
            t_ratchet_tree,
            pq_ratchet_tree,
        }
    }

    pub fn split(self) -> (RatchetTreeIn, RatchetTreeIn) {
        (self.t_ratchet_tree, self.pq_ratchet_tree)
    }
}

/// A key package to add members to an [`crate::ApqMlsGroup`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApqKeyPackage {
    pub(crate) t_key_package: KeyPackage,
    pub(crate) pq_key_package: KeyPackage,
}

impl ApqKeyPackage {
    pub fn new(t_key_package: KeyPackage, pq_key_package: KeyPackage) -> Self {
        Self {
            t_key_package,
            pq_key_package,
        }
    }

    pub fn t_key_package(&self) -> &KeyPackage {
        &self.t_key_package
    }

    pub fn pq_key_package(&self) -> &KeyPackage {
        &self.pq_key_package
    }

    pub fn mode(&self) -> PqtMode {
        match self.pq_key_package.ciphersuite() {
            Ciphersuite::MLS_256_MLKEM1024_AES256GCM_SHA512_MLDSA87
            | Ciphersuite::MLS_256_MLKEM1024_AES256GCM_SHA384_MLDSA87
            | Ciphersuite::MLS_192_MLKEM768_AES256GCM_SHA384_MLDSA65 => PqtMode::ConfAndAuth,
            _ => PqtMode::ConfOnly,
        }
    }

    /// Returns the credential of the T [`KeyPackage`].
    pub fn t_credential(&self) -> &Credential {
        self.t_key_package.leaf_node().credential()
    }

    /// Returns the credential of the PQ [`KeyPackage`].
    pub fn pq_credential(&self) -> &Credential {
        self.pq_key_package.leaf_node().credential()
    }
}

impl From<ApqKeyPackage> for ApqMlsMessageOut {
    fn from(value: ApqKeyPackage) -> Self {
        ApqMlsMessageOut {
            t_message: MlsMessageOut::from(value.t_key_package),
            pq_message: MlsMessageOut::from(value.pq_key_package),
        }
    }
}

/// An unverified key package for adding members to an [`crate::ApqMlsGroup`].
pub struct ApqKeyPackageIn {
    pub(crate) t_key_package: KeyPackageIn,
    pub(crate) pq_key_package: KeyPackageIn,
}

impl ApqKeyPackageIn {
    pub fn new(t_key_package: KeyPackageIn, pq_key_package: KeyPackageIn) -> Self {
        Self {
            t_key_package,
            pq_key_package,
        }
    }

    pub fn unwrap_verified(self) -> ApqKeyPackage {
        ApqKeyPackage {
            t_key_package: self.t_key_package.into_unchecked(),
            pq_key_package: self.pq_key_package.into_unchecked(),
        }
    }
}

/// The group info of an [`crate::ApqMlsGroup`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApqGroupInfo {
    pub(crate) t_group_info: GroupInfo,
    pub(crate) pq_group_info: GroupInfo,
}

impl ApqGroupInfo {
    pub fn new(t_group_info: GroupInfo, pq_group_info: GroupInfo) -> Self {
        Self {
            t_group_info,
            pq_group_info,
        }
    }

    pub fn into_parts(self) -> (GroupInfo, GroupInfo) {
        (self.t_group_info, self.pq_group_info)
    }
}

impl From<ApqGroupInfo> for ApqMlsMessageOut {
    fn from(value: ApqGroupInfo) -> Self {
        ApqMlsMessageOut {
            t_message: MlsMessageOut::from(value.t_group_info),
            pq_message: MlsMessageOut::from(value.pq_group_info),
        }
    }
}

/// A verifiable group info for an [`crate::ApqMlsGroup`].
pub struct VerifiableApqGroupInfo {
    pub(crate) t_group_info: VerifiableGroupInfo,
    pub(crate) pq_group_info: VerifiableGroupInfo,
}

impl VerifiableApqGroupInfo {
    /// Verifies the group info and returns the contained [`ApqGroupInfo`].
    pub fn verify(
        self,
        provider: &impl OpenMlsCrypto,
        verifying_key: &ApqVerifyingKey,
    ) -> Result<ApqGroupInfo, SignatureError> {
        let t_verifying_key = OpenMlsSignaturePublicKey::from_signature_key(
            verifying_key.t_verifying_key.clone(),
            self.t_group_info.ciphersuite().signature_algorithm(),
        );
        let pq_verifying_key = OpenMlsSignaturePublicKey::from_signature_key(
            verifying_key.pq_verifying_key.clone(),
            self.pq_group_info.ciphersuite().signature_algorithm(),
        );
        Ok(ApqGroupInfo {
            t_group_info: self.t_group_info.verify(provider, &t_verifying_key)?,
            pq_group_info: self.pq_group_info.verify(provider, &pq_verifying_key)?,
        })
    }
}

pub struct ApqProposalIn {
    pub(crate) t_proposal: PublicMessageIn,
    pub(crate) pq_proposal: PublicMessageIn,
}

impl ApqProposalIn {
    pub fn new(t_proposal: PublicMessageIn, pq_proposal: PublicMessageIn) -> Self {
        Self {
            t_proposal,
            pq_proposal,
        }
    }
}
