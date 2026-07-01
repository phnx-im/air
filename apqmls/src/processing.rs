// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;

use openmls::{
    component::ComponentData,
    group::{
        AppDataUpdates, GroupEpoch, GroupId, MlsGroup, ProcessMessageError, PublicGroup,
        PublicProcessMessageError, StagedCommit,
    },
    prelude::{
        AppDataUpdateOperation, Ciphersuite, Credential, LeafNodeIndex, OpenMlsCrypto,
        ProcessedMessage, ProcessedMessageContent, Proposal, ProposalIn, ProposalOrRefIn,
        ProposalType, Sender, UnverifiedMessage,
    },
    schedule::{PreSharedKeyId, Psk, psk::ApplicationPsk},
    storage::OpenMlsProvider,
};
use thiserror::Error;

use crate::{
    ApqMlsGroup, ApqMlsGroupMut,
    extension::{APQMLS_COMPONENT_ID, ApqInfo},
    messages::ApqProtocolMessage,
    psk::{ApqPskError, store_psk},
    public_group::ApqPublicGroupMut,
    secret::Secret,
};

/// A bundle consisting of the processed messages of both the traditional and the PQ group.
pub struct ApqProcessedMessage {
    pub t_message: ProcessedMessage,
    pub pq_message: ProcessedMessage,
}

/// A bundle consisting of the staged commits of both the traditional and the
/// PQ group.
pub struct ApqStagedCommit {
    pub t_staged_commit: StagedCommit,
    pub pq_staged_commit: StagedCommit,
}

impl ApqProcessedMessage {
    pub fn into_staged_commit(self) -> Option<ApqStagedCommit> {
        let t_staged_commit = match self.t_message.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => *staged_commit,
            _ => return None,
        };
        let pq_staged_commit = match self.pq_message.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => *staged_commit,
            _ => return None,
        };
        Some(ApqStagedCommit {
            t_staged_commit,
            pq_staged_commit,
        })
    }
}

/// Errors that can occur when processing a message with an [`ApqMlsGroup`].
#[derive(Debug, Error)]
pub enum ApqProcessMessageError<StorageError> {
    #[error("Failed to process message: {0}")]
    Processing(#[from] ProcessMessageError<StorageError>),
    #[error(transparent)]
    Psk(#[from] ApqPskError<StorageError>),
    #[error(transparent)]
    Validation(#[from] ApqProcessMessageValidationError),
}

#[derive(Debug, Error, PartialEq, Clone)]
pub enum ApqProcessPublicMessageError {
    #[error(transparent)]
    Processing(#[from] PublicProcessMessageError),
    #[error(transparent)]
    Validation(#[from] ApqProcessMessageValidationError),
}

#[derive(Debug, Error, PartialEq, Eq, Clone, Copy)]
pub enum ApqProcessMessageValidationError {
    #[error("The message type is invalid for processing.")]
    InvalidMessageType,
    #[error("The MLS messages don't match.")]
    MismatchedMessages,
    #[error("APQInfo extension is missing or invalid in commit message.")]
    MissingApqInfo,
    #[error("APQInfo extension content is invalid.")]
    InvalidApqInfo,
}

#[derive(Eq)]
enum MessageType<F: Fn(&Credential, &Credential) -> bool> {
    Proposal(ProposalContent<F>),
    Commit(CommitContent<F>),
}

impl<F: Fn(&Credential, &Credential) -> bool> Debug for MessageType<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageType::Proposal(proposal) => f
                .debug_struct("Proposal")
                .field("proposal_type", &proposal.proposal_type)
                .field("credential", &proposal.credential)
                .field("leaf_index", &proposal.leaf_index)
                .finish(),
            MessageType::Commit(commit) => f
                .debug_struct("Commit")
                .field("adds", &commit.adds)
                .field("removes", &commit.removes)
                .field("updates", &commit.updates)
                .finish(),
        }
    }
}

impl<F: Fn(&Credential, &Credential) -> bool> MessageType<F> {
    fn new(processed_message: &ProcessedMessageContent, compare: F) -> Option<Self> {
        match processed_message {
            ProcessedMessageContent::ApplicationMessage(_) => None,
            ProcessedMessageContent::ProposalMessage(queued_proposal) => {
                let proposal = queued_proposal.proposal();
                let proposal_type = proposal.proposal_type();
                let (credential, leaf_index) = match proposal {
                    Proposal::Add(add_proposal) => (
                        Some(add_proposal.key_package().leaf_node().credential().clone()),
                        None,
                    ),
                    Proposal::Update(update_proposal) => {
                        (Some(update_proposal.leaf_node().credential().clone()), None)
                    }
                    Proposal::Remove(remove_proposal) => (None, Some(remove_proposal.removed())),
                    _ => (None, None),
                };
                Some(MessageType::Proposal(ProposalContent {
                    proposal_type,
                    credential,
                    leaf_index,
                    compare,
                }))
            }
            ProcessedMessageContent::ExternalJoinProposalMessage(queued_proposal) => {
                let proposal = queued_proposal.proposal();
                let proposal_type = proposal.proposal_type();
                let credential = if let Proposal::Add(add_proposal) = proposal {
                    Some(add_proposal.key_package().leaf_node().credential().clone())
                } else {
                    None
                };
                Some(MessageType::Proposal(ProposalContent {
                    proposal_type,
                    credential,
                    leaf_index: None,
                    compare,
                }))
            }
            ProcessedMessageContent::StagedCommitMessage(staged_commit) => {
                let adds = staged_commit
                    .add_proposals()
                    .map(|p| {
                        p.add_proposal()
                            .key_package()
                            .leaf_node()
                            .credential()
                            .clone()
                    })
                    .collect();
                let removes = staged_commit
                    .remove_proposals()
                    .map(|p| p.remove_proposal().removed())
                    .collect();
                let updates = staged_commit
                    .update_proposals()
                    .map(|p| p.update_proposal().leaf_node().credential().clone())
                    .collect();
                let path_credential = staged_commit
                    .update_path_leaf_node()
                    .map(|node| node.credential().clone());
                Some(MessageType::Commit(CommitContent {
                    path_credential,
                    adds,
                    removes,
                    updates,
                    compare,
                }))
            }
            // XXX: IMPLEMENT!
            ProcessedMessageContent::OwnPendingCommit => None,
        }
    }
}

#[derive(Debug, Eq)]
struct ProposalContent<F: Fn(&Credential, &Credential) -> bool> {
    proposal_type: ProposalType,
    credential: Option<Credential>,
    leaf_index: Option<LeafNodeIndex>,
    compare: F,
}

impl<F: Fn(&Credential, &Credential) -> bool> PartialEq for ProposalContent<F> {
    fn eq(&self, other: &Self) -> bool {
        let same_credential = match (&self.credential, &other.credential) {
            (Some(a), Some(b)) => (self.compare)(a, b),
            (None, None) => true,
            _ => false,
        };
        self.proposal_type == other.proposal_type
            && self.leaf_index == other.leaf_index
            && same_credential
    }
}

impl<F: Fn(&Credential, &Credential) -> bool> PartialEq for MessageType<F> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MessageType::Proposal(a), MessageType::Proposal(b)) => a == b,
            (MessageType::Commit(a), MessageType::Commit(b)) => a == b,
            _ => false,
        }
    }
}

#[derive(Debug, Eq)]
struct CommitContent<F: Fn(&Credential, &Credential) -> bool> {
    path_credential: Option<Credential>,
    adds: Vec<Credential>,
    removes: Vec<LeafNodeIndex>,
    updates: Vec<Credential>,
    compare: F,
}

impl<F: Fn(&Credential, &Credential) -> bool> PartialEq for CommitContent<F> {
    fn eq(&self, other: &Self) -> bool {
        let same_path_credential = match (&self.path_credential, &other.path_credential) {
            (Some(a), Some(b)) => (self.compare)(a, b),
            (None, None) => true,
            _ => false,
        };
        same_path_credential
            && self.removes == other.removes
            && self.adds.len() == other.adds.len()
            && self.updates.len() == other.updates.len()
            && self
                .adds
                .iter()
                .zip(&other.adds)
                .all(|(a, b)| (self.compare)(a, b))
            && self
                .updates
                .iter()
                .zip(&other.updates)
                .all(|(a, b)| (self.compare)(a, b))
    }
}

#[derive(Eq)]
struct MessageInfo<F: Fn(&Credential, &Credential) -> bool> {
    msg_type: MessageType<F>,
    sender: Sender,
}

impl<F: Fn(&Credential, &Credential) -> bool> MessageInfo<F> {
    fn new(
        content: &ProcessedMessageContent,
        sender: Sender,
        sender_equivalence: F,
    ) -> Result<Self, ApqProcessMessageValidationError>
    where
        F: Fn(&Credential, &Credential) -> bool,
    {
        let msg_type = MessageType::new(content, sender_equivalence)
            .ok_or(ApqProcessMessageValidationError::InvalidMessageType)?;
        Ok(Self { msg_type, sender })
    }
}

impl<F: Fn(&Credential, &Credential) -> bool> Debug for MessageInfo<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageInfo")
            .field("msg_type", &self.msg_type)
            .field("sender", &self.sender)
            .finish()
    }
}

impl<F: Fn(&Credential, &Credential) -> bool> PartialEq for MessageInfo<F> {
    fn eq(&self, other: &Self) -> bool {
        self.msg_type == other.msg_type && self.sender == other.sender
    }
}

impl ApqMlsGroup {
    /// See [`ApqMlsGroupMut::process_message`].
    pub fn process_message<F, Provider: OpenMlsProvider>(
        &mut self,
        provider: &Provider,
        message: impl Into<ApqProtocolMessage>,
        sender_equivalence: F,
    ) -> Result<ApqProcessedMessage, ApqProcessMessageError<Provider::StorageError>>
    where
        F: Fn(&Credential, &Credential) -> bool,
    {
        self.as_mut()
            .process_message(provider, message, sender_equivalence)
    }
}

impl ApqMlsGroupMut<'_> {
    /// Processes an incoming APQMLS message.
    ///
    /// Parses incoming messages from the DS. Checks for syntactic errors and makes some semantic checks
    /// as well. If the input is an encrypted message, it will be decrypted. This processing function
    /// does syntactic and semantic validation of the message. It returns a [`ProcessedMessage`] enum.
    ///
    /// # Errors
    ///
    /// Returns an [`ProcessMessageError`] when the validation checks fail with the exact reason of the
    /// failure.
    pub fn process_message<F, Provider: OpenMlsProvider>(
        &mut self,
        provider: &Provider,
        message: impl Into<ApqProtocolMessage>,
        sender_equivalence: F,
    ) -> Result<ApqProcessedMessage, ApqProcessMessageError<Provider::StorageError>>
    where
        F: Fn(&Credential, &Credential) -> bool,
    {
        let protocol_message: ApqProtocolMessage = message.into();
        // We only export a PSK if we process a PQ message
        let unverified_pq_message = self
            .pq_group
            .unprotect_message(provider, protocol_message.pq_protocol_message)?;
        let pq_updates = extract_app_data_updates(self.pq_group, &unverified_pq_message);
        let mut pq_message = self
            .pq_group
            .process_unverified_message_with_app_data_updates(
                provider,
                unverified_pq_message,
                pq_updates,
            )?;

        let pq_message_info = MessageInfo::new(
            pq_message.content(),
            pq_message.sender().clone(),
            &sender_equivalence,
        )?;

        // If we have a commit message and it is not a self-removal, we need to export the PSK.
        //
        // Self-removal is a special case where PSK injection should be skipped: The T group commit
        // also removes us, so OpenMLS returns early before reaching the key schedule.
        if let ProcessedMessageContent::StagedCommitMessage(staged_commit) = pq_message.content()
            && !staged_commit.self_removed()
        {
            let apq_exporter_bytes = pq_message
                .safe_export_secret(provider.crypto(), APQMLS_COMPONENT_ID)
                .map_err(ApqPskError::ExportFromProcessed)?;

            let apq_exporter: Secret = apq_exporter_bytes.into();

            let apq_psk_id = apq_exporter
                .derive_secret(provider.crypto(), self.t_group.ciphersuite(), "psk_id")
                .map_err(ApqPskError::DerivingPskId)?;
            let apq_psk = apq_exporter
                .derive_secret(provider.crypto(), self.t_group.ciphersuite(), "psk")
                .map_err(ApqPskError::DerivingPskId)?;
            drop(apq_exporter); // Zeroize the secret

            let psk = Psk::Application(ApplicationPsk::new(
                APQMLS_COMPONENT_ID,
                apq_psk_id.as_slice().into(),
            ));
            let id = PreSharedKeyId::new(self.t_group.ciphersuite(), provider.rand(), psk)
                .map_err(ApqPskError::DerivingPskId)?;
            store_psk(provider, id, apq_psk.as_slice())?;
        }

        let unverified_t_message = self
            .t_group
            .unprotect_message(provider, protocol_message.t_protocol_message)?;
        let t_updates = extract_app_data_updates(self.t_group, &unverified_t_message);
        let t_message = self
            .t_group
            .process_unverified_message_with_app_data_updates(
                provider,
                unverified_t_message,
                t_updates,
            )?;

        let t_message_info = MessageInfo::new(
            t_message.content(),
            t_message.sender().clone(),
            &sender_equivalence,
        )?;

        // Make sure that messages match up
        if pq_message_info != t_message_info {
            return Err(ApqProcessMessageValidationError::MismatchedMessages.into());
        }

        let pq_params = ValidationParams::from_mls_group(self.pq_group);
        let t_params = ValidationParams::from_mls_group(self.t_group);
        ValidationParams::validate(pq_params, t_params, &pq_message, &t_message)?;

        Ok(ApqProcessedMessage {
            t_message,
            pq_message,
        })
    }
}

impl ApqPublicGroupMut<'_> {
    /// Processes an incoming public AQPMLS message.
    ///
    /// Validates both messages, checks T/PQ consistency (same operator/sender), ApqInfo
    /// epoch/group-id/ciphersuite invariants). No PSK derivation is performed.
    pub fn process_message<Crypto: OpenMlsCrypto, F>(
        &mut self,
        crypto: &Crypto,
        message: impl Into<ApqProtocolMessage>,
        sender_equivalence: F,
    ) -> Result<ApqProcessedMessage, ApqProcessPublicMessageError>
    where
        F: Fn(&Credential, &Credential) -> bool,
    {
        let protocol_message: ApqProtocolMessage = message.into();

        let pq_message = self
            .pq_public_group
            .process_message_with_app_data_updates(crypto, protocol_message.pq_protocol_message)?;
        let pq_message_info = MessageInfo::new(
            pq_message.content(),
            pq_message.sender().clone(),
            &sender_equivalence,
        )?;

        let t_message = self
            .t_public_group
            .process_message_with_app_data_updates(crypto, protocol_message.t_protocol_message)?;
        let t_message_info = MessageInfo::new(
            t_message.content(),
            t_message.sender().clone(),
            &sender_equivalence,
        )?;

        // Note: no PSK export/store

        // Make sure that messages match up
        if pq_message_info != t_message_info {
            return Err(ApqProcessMessageValidationError::MismatchedMessages.into());
        }

        let pq_params = ValidationParams::from_public_group(self.pq_public_group);
        let t_params = ValidationParams::from_public_group(self.t_public_group);
        ValidationParams::validate(pq_params, t_params, &pq_message, &t_message)?;

        Ok(ApqProcessedMessage {
            t_message,
            pq_message,
        })
    }
}

struct ValidationParams<'a> {
    epoch: GroupEpoch,
    group_id: &'a GroupId,
    ciphersuite: Ciphersuite,
}

impl<'a> ValidationParams<'a> {
    fn from_mls_group(group: &'a MlsGroup) -> Self {
        Self {
            epoch: group.epoch(),
            group_id: group.group_id(),
            ciphersuite: group.ciphersuite(),
        }
    }

    fn from_public_group(group: &'a PublicGroup) -> Self {
        Self {
            epoch: group.group_context().epoch(),
            group_id: group.group_context().group_id(),
            ciphersuite: group.group_context().ciphersuite(),
        }
    }

    fn validate(
        pq_params: Self,
        t_params: Self,
        pq_message: &ProcessedMessage,
        t_message: &ProcessedMessage,
    ) -> Result<(), ApqProcessMessageValidationError> {
        use ApqProcessMessageValidationError::*;

        // If both are commits, the [`ApqInfo`] component must be in line with the info of both groups
        if let ProcessedMessageContent::StagedCommitMessage(pq_staged_commit) = pq_message.content()
            && let ProcessedMessageContent::StagedCommitMessage(t_staged_commit) =
                t_message.content()
        {
            let pq_apq_info =
                ApqInfo::from_extensions(pq_staged_commit.group_context().extensions())
                    .map_err(|_| InvalidApqInfo)?
                    .ok_or(MissingApqInfo)?;
            let t_apq_info = ApqInfo::from_extensions(t_staged_commit.group_context().extensions())
                .map_err(|_| InvalidApqInfo)?
                .ok_or(MissingApqInfo)?;

            // ApqInfo contents must match
            let apq_info_match = pq_apq_info == t_apq_info;

            // Epochs must be in line with the groups
            let epochs_match = pq_apq_info.pq_epoch == pq_staged_commit.group_context().epoch()
                && t_apq_info.t_epoch == t_staged_commit.group_context().epoch();

            // New epochs must be one higher than the current ones
            let epochs_are_incremented = pq_apq_info.pq_epoch.as_u64()
                == pq_params.epoch.as_u64() + 1
                && t_apq_info.t_epoch.as_u64() == t_params.epoch.as_u64() + 1;

            // Group IDs must be in line with the groups
            let group_ids_match = pq_apq_info.pq_session_group_id == *pq_params.group_id
                && t_apq_info.t_session_group_id == *t_params.group_id;

            // Ciphersuites must be in line with the groups
            let ciphersuites_match = pq_apq_info.pq_cipher_suite == pq_params.ciphersuite
                && t_apq_info.t_cipher_suite == t_params.ciphersuite;

            if !apq_info_match
                || !epochs_match
                || !epochs_are_incremented
                || !group_ids_match
                || !ciphersuites_match
            {
                return Err(InvalidApqInfo);
            }
        }

        Ok(())
    }
}

fn extract_app_data_updates(
    group: &MlsGroup,
    unverified: &UnverifiedMessage,
) -> Option<AppDataUpdates> {
    let mut updater = group.app_data_dictionary_updater();
    let mut updated = false;
    for proposal in unverified.committed_proposals()? {
        if let ProposalOrRefIn::Proposal(p) = proposal
            && let ProposalIn::AppDataUpdate(p) = &**p
        {
            match p.operation() {
                AppDataUpdateOperation::Update(data) => {
                    updater.set(ComponentData::from_parts(p.component_id(), data.clone()));
                }
                AppDataUpdateOperation::Remove => {
                    updater.remove(&p.component_id());
                }
            }
            updated = true;
        }
    }
    updated.then(|| updater.changes()).flatten()
}
