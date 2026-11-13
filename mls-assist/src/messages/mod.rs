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

    /// The wrapped [`MlsMessageOut`], i.e. the message the DS fans out to the
    /// other group members (without the assisted group info).
    pub fn mls_message(&self) -> &MlsMessageOut {
        &self.mls_message
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

/// The T and the PQ message of an APQ assisted message.
#[derive(Debug)]
pub struct ApqAssistedMessageIn {
    pub t_message: AssistedMessageIn,
    pub pq_message: AssistedMessageIn,
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

    /// Whether this is a [`PrivateMessage`] with application content.
    ///
    /// The content type of a [`PrivateMessage`] is in its unencrypted header,
    /// so this needs no group state.
    pub fn is_private_application_message(&self) -> bool {
        match &self.mls_message {
            ProtocolMessage::PrivateMessage(pm) => pm.content_type() == ContentType::Application,
            ProtocolMessage::PublicMessage(_) => false,
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

    const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

    struct Client {
        provider: OpenMlsRustCrypto,
        signer: SignatureKeyPair,
        credential_with_key: CredentialWithKey,
    }

    impl Client {
        fn new(identity: &[u8]) -> Self {
            let signer = SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).unwrap();
            let credential_with_key = CredentialWithKey {
                credential: BasicCredential::new(identity.to_vec()).into(),
                signature_key: signer.public().into(),
            };
            Self {
                provider: OpenMlsRustCrypto::default(),
                signer,
                credential_with_key,
            }
        }

        fn key_package(&self) -> KeyPackage {
            KeyPackage::builder()
                .build(
                    CIPHERSUITE,
                    &self.provider,
                    &self.signer,
                    self.credential_with_key.clone(),
                )
                .unwrap()
                .into_key_package()
        }

        fn group(&self, wire_format_policy: WireFormatPolicy) -> MlsGroup {
            MlsGroup::builder()
                .ciphersuite(CIPHERSUITE)
                .with_wire_format_policy(wire_format_policy)
                .build(
                    &self.provider,
                    &self.signer,
                    self.credential_with_key.clone(),
                )
                .unwrap()
        }

        /// A self-update commit to `group` with its group info.
        fn commit(&self, group: &mut MlsGroup) -> (MlsMessageOut, MlsMessageOut) {
            let (commit, _welcome, group_info) = group
                .commit_builder()
                .force_self_update(true)
                .load_psks(self.provider.storage())
                .unwrap()
                .create_group_info(true)
                .build(
                    self.provider.rand(),
                    self.provider.crypto(),
                    &self.signer,
                    |_| true,
                )
                .unwrap()
                .stage_commit(&self.provider)
                .unwrap()
                .into_contents();
            (commit, group_info.unwrap().into())
        }
    }

    fn serialized_key_package(identity: &[u8]) -> Vec<u8> {
        MlsMessageOut::from(Client::new(identity).key_package())
            .tls_serialize_detached()
            .unwrap()
    }

    fn assisted(message: MlsMessageOut, group_info: Option<MlsMessageOut>) -> AssistedMessageIn {
        let bytes = AssistedMessageOut::new(message, group_info)
            .tls_serialize_detached()
            .unwrap();
        AssistedMessageIn::tls_deserialize_exact_bytes(&bytes).unwrap()
    }

    #[test]
    fn a_private_application_message_is_one() {
        let alice = Client::new(b"alice");
        let mut group = alice.group(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY);
        let message = group
            .create_unconfirmed_message(&alice.provider, &alice.signer, b"hello")
            .unwrap()
            .message;
        assert!(assisted(message, None).is_private_application_message());
    }

    #[test]
    fn a_private_commit_is_no_application_message() {
        let alice = Client::new(b"alice");
        let mut group = alice.group(PURE_CIPHERTEXT_WIRE_FORMAT_POLICY);
        let (commit, _group_info) = alice.commit(&mut group);
        assert!(matches!(
            commit.body(),
            MlsMessageBodyOut::PrivateMessage(_)
        ));
        assert!(!assisted(commit, None).is_private_application_message());
    }

    #[test]
    fn a_public_commit_is_no_application_message() {
        let alice = Client::new(b"alice");
        let mut group = alice.group(PURE_PLAINTEXT_WIRE_FORMAT_POLICY);
        let (commit, group_info) = alice.commit(&mut group);
        assert!(!assisted(commit, Some(group_info)).is_private_application_message());
    }

    #[test]
    fn a_new_member_proposal_is_no_application_message() {
        let alice = Client::new(b"alice");
        let group = alice.group(PURE_PLAINTEXT_WIRE_FORMAT_POLICY);
        let bob = Client::new(b"bob");
        let proposal =
            JoinProposal::new::<<OpenMlsRustCrypto as OpenMlsProvider>::StorageProvider>(
                bob.key_package(),
                group.group_id().clone(),
                group.epoch(),
                &bob.signer,
            )
            .unwrap();
        let proposal = assisted(proposal, None);
        assert_eq!(proposal.sender(), Some(&Sender::NewMemberProposal));
        assert!(!proposal.is_private_application_message());
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
