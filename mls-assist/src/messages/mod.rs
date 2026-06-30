// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::tls_codec::{self, TlsDeserialize, TlsSerialize, TlsSize};
use openmls::{
    framing::{ContentType, MlsMessageBodyOut},
    prelude::{
        ConfirmationTag, Extensions, GroupContext, GroupId, KeyPackageRef, LeafNodeIndex,
        MlsMessageOut, ProtocolMessage, Sender, Signature, Welcome,
        group_info::VerifiableGroupInfo,
    },
};
use openmls::{group::GroupEpoch, prelude::group_info::GroupInfo};

#[cfg(doc)]
use openmls::prelude::{PrivateMessage, PublicMessage};
use serde::{Deserialize, Serialize};

pub mod codec;

#[derive(Clone, Debug, TlsSerialize, TlsSize, Serialize, Deserialize)]
pub struct AssistedMessageOut {
    mls_message: MlsMessageOut,
    assisted_group_info_option: Option<AssistedGroupInfo>,
}

impl AssistedMessageOut {
    /// Create a new [`AssistedMessageOut`] from an [`MlsMessageOut`] containing
    /// either a [`PublicMessage`] or a [`PrivateMessage`] and optionally an
    /// [`MlsMessageOut`] containing a [`GroupInfo`].
    pub fn new(mls_message: MlsMessageOut, group_info_option: Option<MlsMessageOut>) -> Self {
        let is_public_commit = matches!(
            mls_message.body(),
            MlsMessageBodyOut::PublicMessage(pm)
                if pm.content_type() == ContentType::Commit
        );
        let assisted_group_info_option = match group_info_option.as_ref().map(|m| m.body()) {
            Some(MlsMessageBodyOut::GroupInfo(gi)) => {
                // Ensure that GroupInfo is only provided for (public) Commit messages.
                debug_assert!(
                    is_public_commit,
                    "GroupInfo should only be provided for Commit messages."
                );
                Some(AssistedGroupInfo {
                    extensions: gi.extensions().clone(),
                    signature: gi.signature().clone(),
                })
            }
            // Second input should be None or GroupInfo
            Some(_) => {
                debug_assert!(false, "Second input must be GroupInfo if provided.");
                None
            }
            // If no GroupInfo is provided, it must be a non-commit message.
            None => {
                debug_assert!(
                    !is_public_commit,
                    "GroupInfo must be provided for Commit messages."
                );
                None
            }
        };
        Self {
            mls_message,
            assisted_group_info_option,
        }
    }

    /// Get the epoch of the MLS message, if it is a PublicMessage or
    /// PrivateMessage.
    pub fn epoch(&self) -> Option<GroupEpoch> {
        match self.mls_message.body() {
            MlsMessageBodyOut::PublicMessage(pm) => Some(pm.epoch()),
            MlsMessageBodyOut::PrivateMessage(pm) => Some(pm.epoch()),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct AssistedMessageIn {
    pub(crate) mls_message: ProtocolMessage,
    pub(crate) serialized_mls_message: SerializedMlsMessage,
    pub(crate) group_info_option: Option<AssistedGroupInfoIn>,
}

#[derive(Debug)]
pub struct SerializedMlsMessage(pub Vec<u8>);

impl SerializedMlsMessage {
    /// Combine an already-serialized T and PQ MLS messages into a the APQ bundle wire format: T
    /// followed by PQ, as [`apqmls::messages::ApqMlsMessageIn`] expects.
    pub fn combine_apq(t: Self, pq: Self) -> Self {
        let mut bytes = t.0;
        bytes.extend_from_slice(&pq.0);
        Self(bytes)
    }
}

impl AssistedMessageIn {
    pub fn into_serialized_mls_message(self) -> SerializedMlsMessage {
        self.serialized_mls_message
    }

    pub fn epoch(&self) -> GroupEpoch {
        self.mls_message.epoch()
    }

    pub fn group_id(&self) -> &GroupId {
        self.mls_message.group_id()
    }

    pub fn sender(&self) -> Option<&Sender> {
        match &self.mls_message {
            ProtocolMessage::PrivateMessage(_) => None,
            ProtocolMessage::PublicMessage(pm) => Some(pm.sender()),
        }
    }
}

#[derive(Debug, TlsSize, Clone, TlsSerialize, Serialize, Deserialize)]
pub struct AssistedGroupInfo {
    extensions: Extensions<GroupInfo>,
    signature: Signature,
}

#[derive(Debug, TlsDeserialize, TlsSize, Clone)]
pub struct AssistedGroupInfoIn {
    extensions: Extensions<GroupInfo>,
    signature: Signature,
}

impl AssistedGroupInfoIn {
    pub fn into_verifiable_group_info(
        self,
        sender_index: LeafNodeIndex,
        group_context: GroupContext,
        confirmation_tag: ConfirmationTag,
    ) -> VerifiableGroupInfo {
        VerifiableGroupInfo::new(
            group_context,
            self.extensions,
            confirmation_tag,
            sender_index,
            self.signature,
        )
    }
}

#[derive(Debug, Clone)]
pub struct AssistedWelcome {
    pub welcome: Welcome,
}

impl AssistedWelcome {
    pub fn joiners(&self) -> impl Iterator<Item = KeyPackageRef> + '_ {
        self.welcome
            .secrets()
            .iter()
            .map(|secret| secret.new_member())
    }
}

#[cfg(test)]
mod tests {
    use apqmls::messages::ApqMlsMessageIn;
    use openmls::prelude::*;
    use openmls_basic_credential::SignatureKeyPair;
    use openmls_rust_crypto::OpenMlsRustCrypto;
    use tls_codec::{DeserializeBytes, Serialize};

    use super::*;

    fn serialized_key_package(identity: &[u8]) -> Vec<u8> {
        let provider = OpenMlsRustCrypto::default();
        let ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
        let signer = SignatureKeyPair::new(ciphersuite.signature_algorithm()).unwrap();
        let credential_with_key = CredentialWithKey {
            credential: BasicCredential::new(identity.to_vec()).into(),
            signature_key: signer.public().into(),
        };
        let key_package = KeyPackage::builder()
            .build(ciphersuite, &provider, &signer, credential_with_key)
            .unwrap()
            .into_key_package();
        MlsMessageOut::from(key_package)
            .tls_serialize_detached()
            .unwrap()
    }

    #[test]
    fn combine_apq_is_t_then_pq_without_framing() {
        let t_bytes = serialized_key_package(b"alice");
        let pq_bytes = serialized_key_package(b"alice");

        let combined = SerializedMlsMessage::combine_apq(
            SerializedMlsMessage(t_bytes.clone()),
            SerializedMlsMessage(pq_bytes.clone()),
        );

        // Order + no framing: exactly the T bytes followed by the PQ bytes.
        assert_eq!(combined.0.len(), t_bytes.len() + pq_bytes.len());
        assert!(combined.0.starts_with(&t_bytes));
        assert!(combined.0.ends_with(&pq_bytes));

        // And the result parses as the APQ bundle, recovering both messages.
        let apq_message = ApqMlsMessageIn::tls_deserialize_exact_bytes(&combined.0)
            .expect("combined bytes must be a valid ApqMlsMessageIn");
        let t_message =
            MlsMessageIn::tls_deserialize_exact_bytes(&t_bytes).expect("MLS message must be valid");
        let pq_message = MlsMessageIn::tls_deserialize_exact_bytes(&pq_bytes)
            .expect("MLS message must be valid");
        assert_eq!(apq_message.t_message(), &t_message);
        assert_eq!(apq_message.pq_message(), &pq_message);
    }
}
