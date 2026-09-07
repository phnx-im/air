// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::BTreeSet, fmt::Debug};

use openmls::{
    component::ComponentData,
    group::{
        AppDataDictionaryUpdater, AppDataUpdates, GroupEpoch, GroupId, MlsGroup,
        ProcessMessageError, PublicGroup, PublicProcessMessageError, ResolveAppDataCommitError,
        StagedCommit,
    },
    prelude::{
        AppDataUpdateOperation, AppDataUpdateProposal, Ciphersuite, Credential, LeafNodeIndex,
        OpenMlsCrypto, PreSharedKeyProposal, ProcessedMessage, ProcessedMessageContent, Proposal,
        ProposalType, Sender,
    },
    schedule::{PreSharedKeyId, Psk, psk::ApplicationPsk},
    storage::OpenMlsProvider,
};
use thiserror::Error;
use tls_codec::{Deserialize as _, Serialize as _};

use crate::{
    ApqMlsGroup, ApqMlsGroupMut,
    extension::{APQMLS_COMPONENT_ID, ApqInfo, ApqInfoUpdate, ApqInfoUpdateError, ApqInfoUpdates},
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
    #[error(transparent)]
    AppDataUpdate(#[from] ResolveAppDataCommitError),
    #[error(transparent)]
    ApqInfoUpdate(#[from] ApqInfoUpdateError),
}

#[derive(Debug, Error, PartialEq, Clone)]
pub enum ApqProcessPublicMessageError {
    #[error(transparent)]
    Processing(#[from] PublicProcessMessageError),
    #[error(transparent)]
    Validation(#[from] ApqProcessMessageValidationError),
    #[error(transparent)]
    AppDataUpdate(#[from] ResolveAppDataCommitError),
    #[error(transparent)]
    ApqInfoUpdate(#[from] ApqInfoUpdateError),
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
    #[error("The commit modifies an APQInfo field other than the two epochs.")]
    ImmutableApqInfoModified,
    #[error("The T commit of a FULL commit carries no PreSharedKey proposal for the APQ PSK.")]
    MissingApqPsk,
    #[error("The APQ PreSharedKey proposal of the T commit names a different PSK.")]
    ApqPskMismatch,
    #[error("The T commit carries more than one APQ PreSharedKey proposal.")]
    DuplicateApqPsk,
    #[error("The PreSharedKey proposal of the T commit is malformed.")]
    MalformedApqPsk,
}

#[derive(Eq)]
enum MessageType<F: Fn(&Credential, &Credential) -> bool> {
    Proposal(ProposalContent<F>),
    Commit(CommitContent<F>),
    OwnPendingCommit,
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
            MessageType::OwnPendingCommit => f.debug_struct("OwnPendingCommit").finish(),
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
            // Our own commit echoed back. There is no content to inspect; we
            // only record that this is an own-pending-commit so the T/PQ
            // consistency check can confirm both groups agree.
            ProcessedMessageContent::OwnPendingCommit => Some(MessageType::OwnPendingCommit),
            ProcessedMessageContent::OwnPrivateMessage => None,
            ProcessedMessageContent::UnresolvedAppDataCommit(_) => {
                debug_assert!(
                    false,
                    "Unexpected UnresolvedAppDataCommit, should have been resolved before"
                );
                None
            }
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
            (MessageType::OwnPendingCommit, MessageType::OwnPendingCommit) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Eq)]
struct CommitContent<F: Fn(&Credential, &Credential) -> bool> {
    path_credential: Option<Credential>,
    adds: Vec<Credential>,
    // A set, because the staged commit yields the remove proposals in no particular order.
    removes: BTreeSet<LeafNodeIndex>,
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

        let pq_message = self
            .pq_group
            .process_message(provider, protocol_message.pq_protocol_message)?;
        let mut pq_message = resolve_app_data_commit(self.pq_group, provider, pq_message)?;

        let pq_message_info = MessageInfo::new(
            pq_message.content(),
            pq_message.sender().clone(),
            &sender_equivalence,
        )?;

        // The PSK the T commit of this FULL commit must import, if there is one.
        let mut expected_psk = None;

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
            let id = store_psk(provider, id, apq_psk.as_slice())?;
            expected_psk = Some(id.psk().clone());
        }

        let t_message = self
            .t_group
            .process_message(provider, protocol_message.t_protocol_message)?;
        let t_message = resolve_app_data_commit(self.t_group, provider, t_message)?;

        let t_message_info = MessageInfo::new(
            t_message.content(),
            t_message.sender().clone(),
            &sender_equivalence,
        )?;

        // Make sure that messages match up
        if pq_message_info != t_message_info {
            return Err(ApqProcessMessageValidationError::MismatchedMessages.into());
        }

        // The T commit must announce the PSK we just derived from the new PQ
        // epoch.
        if let Some(expected_psk) = &expected_psk
            && let ProcessedMessageContent::StagedCommitMessage(t_staged_commit) =
                t_message.content()
        {
            validate_apq_psk_proposal(t_staged_commit, expected_psk)?;
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
    /// epoch/group-id/ciphersuite invariants).
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
            .process_message(crypto, protocol_message.pq_protocol_message)?;
        let pq_message = resolve_app_data_commit_public(self.pq_public_group, crypto, pq_message)?;
        let pq_message_info = MessageInfo::new(
            pq_message.content(),
            pq_message.sender().clone(),
            &sender_equivalence,
        )?;

        let t_message = self
            .t_public_group
            .process_message(crypto, protocol_message.t_protocol_message)?;
        let t_message = resolve_app_data_commit_public(self.t_public_group, crypto, t_message)?;
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

        // A `PublicGroup` derives no secrets, so unlike a member it cannot
        // compare the PSK ID against the one the new PQ epoch yields.
        if let ProcessedMessageContent::StagedCommitMessage(t_staged_commit) = t_message.content()
            && matches!(
                pq_message.content(),
                ProcessedMessageContent::StagedCommitMessage(_)
            )
        {
            require_apq_psk_proposal(t_staged_commit)?;
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

/// The [`PreSharedKeyId`] a PSK proposal carries.
///
/// OpenMLS has no accessor for it at the pinned revision. On the wire a
/// `PreSharedKeyProposal` is a `PreSharedKeyID` and nothing else, so the TLS
/// encoding of the two is identical. `apq_psk_proposal_roundtrips` guards the
/// assumption.
fn psk_id_of(proposal: &PreSharedKeyProposal) -> Result<PreSharedKeyId, tls_codec::Error> {
    PreSharedKeyId::tls_deserialize_exact(proposal.tls_serialize_detached()?)
}

/// The APQ PSK a commit announces, i.e. the application PSK of the APQMLS
/// component.
///
/// The T commit of a FULL commit announces exactly one. A second one has no
/// meaning in the draft, so it is rejected rather than ignored: otherwise a
/// sender could hide a bogus PSK behind a valid one. Other PSK proposals, such
/// as the external PSKs a connection offer carries, are not APQ PSKs and are
/// passed over.
fn apq_psk(staged_commit: &StagedCommit) -> Result<Psk, ApqProcessMessageValidationError> {
    let mut found: Option<Psk> = None;
    for proposal in staged_commit.psk_proposals() {
        let psk_id = psk_id_of(proposal.psk_proposal())
            .map_err(|_| ApqProcessMessageValidationError::MalformedApqPsk)?;
        let Psk::Application(psk) = psk_id.psk() else {
            continue;
        };
        if psk.component_id() != APQMLS_COMPONENT_ID {
            continue;
        }
        if found.replace(psk_id.psk().clone()).is_some() {
            return Err(ApqProcessMessageValidationError::DuplicateApqPsk);
        }
    }
    found.ok_or(ApqProcessMessageValidationError::MissingApqPsk)
}

/// The T commit of a FULL commit must announce the PSK exported from the new PQ
/// epoch.
fn validate_apq_psk_proposal(
    t_staged_commit: &StagedCommit,
    expected: &Psk,
) -> Result<(), ApqProcessMessageValidationError> {
    if &apq_psk(t_staged_commit)? != expected {
        return Err(ApqProcessMessageValidationError::ApqPskMismatch);
    }
    Ok(())
}

/// Same as [`validate_apq_psk_proposal`], but without comparing the PSK, for
/// callers that cannot derive it themselves.
fn require_apq_psk_proposal(
    t_staged_commit: &StagedCommit,
) -> Result<(), ApqProcessMessageValidationError> {
    apq_psk(t_staged_commit).map(|_| ())
}

struct ValidationParams<'a> {
    epoch: GroupEpoch,
    group_id: &'a GroupId,
    ciphersuite: Ciphersuite,
    /// The [`ApqInfo`] of the group's current epoch.
    apq_info: Option<ApqInfo>,
}

impl<'a> ValidationParams<'a> {
    fn from_mls_group(group: &'a MlsGroup) -> Self {
        Self {
            epoch: group.epoch(),
            group_id: group.group_id(),
            ciphersuite: group.ciphersuite(),
            apq_info: ApqInfo::from_extensions(group.extensions()).ok().flatten(),
        }
    }

    fn from_public_group(group: &'a PublicGroup) -> Self {
        Self {
            epoch: group.group_context().epoch(),
            group_id: group.group_context().group_id(),
            ciphersuite: group.group_context().ciphersuite(),
            apq_info: ApqInfo::from_extensions(group.group_context().extensions())
                .ok()
                .flatten(),
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

            // Every field other than the two epochs is immutable for the
            // lifetime of the session.
            let immutable_fields_unchanged = [
                (&pq_params.apq_info, &pq_apq_info),
                (&t_params.apq_info, &t_apq_info),
            ]
            .into_iter()
            .all(|(current, new)| {
                current
                    .as_ref()
                    .is_none_or(|current| current.matches_except_epochs(new))
            });
            if !immutable_fields_unchanged {
                return Err(ImmutableApqInfoModified);
            }
        }

        Ok(())
    }
}

/// Resolves an [`UnresolvedAppDataCommit`] into a [`ProcessedMessage`].
fn resolve_app_data_commit<Provider: OpenMlsProvider>(
    group: &MlsGroup,
    provider: &Provider,
    message: ProcessedMessage,
) -> Result<ProcessedMessage, ApqProcessMessageError<Provider::StorageError>> {
    let ProcessedMessageContent::UnresolvedAppDataCommit(unresolved) = message.content() else {
        return Ok(message);
    };
    let updates = compute_app_data_updates(
        group.app_data_dictionary_updater(),
        unresolved.app_data_update_proposals(),
    )?;
    group
        .resolve_app_data_commit(provider, message, updates)
        .map_err(Into::into)
}

/// Same as [`resolve_app_data_commit`], but for public groups.
fn resolve_app_data_commit_public<Crypto: OpenMlsCrypto>(
    group: &PublicGroup,
    crypto: &Crypto,
    message: ProcessedMessage,
) -> Result<ProcessedMessage, ApqProcessPublicMessageError> {
    let ProcessedMessageContent::UnresolvedAppDataCommit(unresolved) = message.content() else {
        return Ok(message);
    };
    let updates = compute_app_data_updates(
        group.app_data_dictionary_updater(),
        unresolved.app_data_update_proposals(),
    )?;
    group
        .resolve_app_data_commit(crypto, message, updates)
        .map_err(Into::into)
}

/// Computes the app data dictionary changes of a commit.
///
/// The APQInfo updates are collected first and applied together, because the
/// draft only allows a single `full_update` or a `new_t_epoch` paired with a
/// `new_pq_epoch`. The dictionary entry itself is a bare [`ApqInfo`], only the
/// proposal payload is an [`ApqInfoUpdate`].
///
/// This is the only place that knows that encoding. Every consumer of an
/// `AppDataUpdate` proposal for [`APQMLS_COMPONENT_ID`] must go through here,
/// otherwise two parties resolve the same commit into different dictionaries
/// and their group contexts diverge.
pub fn compute_app_data_updates<'a>(
    mut updater: AppDataDictionaryUpdater<'a>,
    proposals: impl Iterator<Item = &'a AppDataUpdateProposal>,
) -> Result<Option<AppDataUpdates>, ApqInfoUpdateError> {
    let current_apq_info = updater
        .old_value(APQMLS_COMPONENT_ID)
        .map(ApqInfo::tls_deserialize_exact)
        .transpose()
        .map_err(ApqInfoUpdateError::MalformedApqInfo)?;

    let mut apq_info_updates = ApqInfoUpdates::default();
    let mut apq_info_removed = false;
    let mut updated = false;
    for proposal in proposals {
        let is_apq_info = proposal.component_id() == APQMLS_COMPONENT_ID;
        match proposal.operation() {
            AppDataUpdateOperation::Update(data) if is_apq_info => {
                let update = ApqInfoUpdate::tls_deserialize_exact(data)
                    .map_err(ApqInfoUpdateError::MalformedUpdate)?;
                apq_info_updates.add(update)?;
            }
            AppDataUpdateOperation::Update(data) => {
                updater.set(ComponentData::from_parts(
                    proposal.component_id(),
                    data.clone(),
                ));
            }
            AppDataUpdateOperation::Remove => {
                updater.remove(&proposal.component_id());
                apq_info_removed |= is_apq_info;
            }
        }
        updated = true;
    }

    // Removing the APQInfo and updating it in the same commit contradict each
    // other, and neither is one of the two shapes the draft allows.
    if apq_info_removed && !apq_info_updates.is_empty() {
        return Err(ApqInfoUpdateError::RemovalWithUpdate);
    }

    if let Some(new_apq_info) = apq_info_updates.resolve(current_apq_info.as_ref())? {
        updater.set(
            new_apq_info
                .to_component_data()
                .map_err(ApqInfoUpdateError::Serialization)?,
        );
    }

    Ok(updated.then(|| updater.changes()).flatten())
}

#[cfg(test)]
mod tests {
    use openmls::{
        component::ComponentId,
        prelude::{AppDataDictionary, AppDataUpdateProposal},
    };
    use tls_codec::Serialize as _;

    use super::*;
    use crate::extension::tests::test_apq_info;

    const OTHER_COMPONENT_ID: ComponentId = 0x8002;

    /// The dictionary changes a commit results in, keyed by component ID. A
    /// `None` value is a removal.
    type Changes = Vec<(ComponentId, Option<Vec<u8>>)>;

    fn dictionary_with(apq_info: &ApqInfo) -> AppDataDictionary {
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(
            APQMLS_COMPONENT_ID,
            apq_info.tls_serialize_detached().unwrap(),
        );
        dictionary
    }

    fn apq_proposal(update: ApqInfoUpdate) -> AppDataUpdateProposal {
        AppDataUpdateProposal::update(
            APQMLS_COMPONENT_ID,
            update.tls_serialize_detached().unwrap(),
        )
    }

    fn changes(
        dictionary: Option<&AppDataDictionary>,
        proposals: &[AppDataUpdateProposal],
    ) -> Result<Changes, ApqInfoUpdateError> {
        let updates =
            compute_app_data_updates(AppDataDictionaryUpdater::new(dictionary), proposals.iter())?;
        Ok(updates.into_iter().flatten().collect())
    }

    #[test]
    fn full_update_is_stored_as_a_bare_apq_info() {
        let apq_info = test_apq_info();
        let proposals = [apq_info.to_full_update_proposal().unwrap()];
        assert_eq!(
            changes(None, &proposals).unwrap(),
            vec![(
                APQMLS_COMPONENT_ID,
                Some(apq_info.tls_serialize_detached().unwrap())
            )]
        );
    }

    #[test]
    fn epoch_updates_are_applied_to_the_current_apq_info() {
        let mut apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        let proposals = [
            apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9))),
            apq_proposal(ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(10))),
        ];

        apq_info.set_epoch(GroupEpoch::from(9), GroupEpoch::from(10));
        assert_eq!(
            changes(Some(&dictionary), &proposals).unwrap(),
            vec![(
                APQMLS_COMPONENT_ID,
                Some(apq_info.tls_serialize_detached().unwrap())
            )]
        );
    }

    #[test]
    fn epoch_updates_without_an_existing_apq_info_are_rejected() {
        let proposals = [
            apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9))),
            apq_proposal(ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(10))),
        ];
        assert_eq!(
            changes(None, &proposals),
            Err(ApqInfoUpdateError::NoApqInfo)
        );
    }

    #[test]
    fn a_single_epoch_update_is_rejected() {
        let apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        for update in [
            ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9)),
            ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(9)),
        ] {
            let proposals = [apq_proposal(update)];
            assert_eq!(
                changes(Some(&dictionary), &proposals),
                Err(ApqInfoUpdateError::IncompleteEpochUpdate)
            );
        }
    }

    #[test]
    fn duplicate_epoch_updates_are_rejected() {
        let apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        let proposals = [
            apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9))),
            apq_proposal(ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(10))),
            apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(11))),
        ];
        assert_eq!(
            changes(Some(&dictionary), &proposals),
            Err(ApqInfoUpdateError::DuplicateEpochUpdate)
        );
    }

    #[test]
    fn duplicate_full_updates_are_rejected() {
        let apq_info = test_apq_info();
        let proposals = [
            apq_info.to_full_update_proposal().unwrap(),
            apq_info.to_full_update_proposal().unwrap(),
        ];
        assert_eq!(
            changes(None, &proposals),
            Err(ApqInfoUpdateError::DuplicateFullUpdate)
        );
    }

    #[test]
    fn a_full_update_mixed_with_epoch_updates_is_rejected() {
        let apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        let proposals = [
            apq_info.to_full_update_proposal().unwrap(),
            apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9))),
            apq_proposal(ApqInfoUpdate::NewPqEpoch(GroupEpoch::from(10))),
        ];
        assert_eq!(
            changes(Some(&dictionary), &proposals),
            Err(ApqInfoUpdateError::MixedUpdates)
        );
    }

    #[test]
    fn a_full_update_mixed_with_a_single_epoch_update_is_rejected() {
        let apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        let proposals = [
            apq_info.to_full_update_proposal().unwrap(),
            apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9))),
        ];
        assert_eq!(
            changes(Some(&dictionary), &proposals),
            Err(ApqInfoUpdateError::MixedUpdates)
        );
    }

    #[test]
    fn a_full_update_replaces_the_current_apq_info_wholesale() {
        let current = test_apq_info();
        let dictionary = dictionary_with(&current);
        let mut replacement = current.clone();
        replacement.set_epoch(GroupEpoch::from(41), GroupEpoch::from(42));
        let proposals = [replacement.to_full_update_proposal().unwrap()];
        assert_eq!(
            changes(Some(&dictionary), &proposals).unwrap(),
            vec![(
                APQMLS_COMPONENT_ID,
                Some(replacement.tls_serialize_detached().unwrap())
            )]
        );
    }

    #[test]
    fn malformed_update_is_rejected() {
        let proposals = [AppDataUpdateProposal::update(
            APQMLS_COMPONENT_ID,
            vec![0xff],
        )];
        assert!(matches!(
            changes(None, &proposals),
            Err(ApqInfoUpdateError::MalformedUpdate(_))
        ));
    }

    #[test]
    fn malformed_current_apq_info_is_rejected() {
        let mut dictionary = AppDataDictionary::new();
        dictionary.insert(APQMLS_COMPONENT_ID, vec![0xff]);
        let proposals = [apq_proposal(ApqInfoUpdate::NewTEpoch(GroupEpoch::from(9)))];
        assert!(matches!(
            changes(Some(&dictionary), &proposals),
            Err(ApqInfoUpdateError::MalformedApqInfo(_))
        ));
    }

    #[test]
    fn other_components_are_stored_verbatim() {
        let proposals = [AppDataUpdateProposal::update(
            OTHER_COMPONENT_ID,
            b"opaque".to_vec(),
        )];
        assert_eq!(
            changes(None, &proposals).unwrap(),
            vec![(OTHER_COMPONENT_ID, Some(b"opaque".to_vec()))]
        );
    }

    #[test]
    fn removals_are_passed_through() {
        let apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        let proposals = [
            AppDataUpdateProposal::remove(APQMLS_COMPONENT_ID),
            AppDataUpdateProposal::remove(OTHER_COMPONENT_ID),
        ];
        assert_eq!(
            changes(Some(&dictionary), &proposals).unwrap(),
            vec![(APQMLS_COMPONENT_ID, None), (OTHER_COMPONENT_ID, None)]
        );
    }

    #[test]
    fn removing_and_updating_the_apq_info_in_one_commit_is_rejected() {
        let apq_info = test_apq_info();
        let dictionary = dictionary_with(&apq_info);
        let proposals = [
            apq_info.to_full_update_proposal().unwrap(),
            AppDataUpdateProposal::remove(APQMLS_COMPONENT_ID),
        ];
        assert_eq!(
            changes(Some(&dictionary), &proposals),
            Err(ApqInfoUpdateError::RemovalWithUpdate)
        );
    }

    #[test]
    fn no_proposals_means_no_changes() {
        assert!(changes(None, &[]).unwrap().is_empty());
    }

    /// [`psk_id_of`] relies on a `PreSharedKeyProposal` and a `PreSharedKeyID`
    /// having the same TLS encoding.
    #[test]
    fn apq_psk_proposal_roundtrips() {
        let psk = Psk::Application(ApplicationPsk::new(
            APQMLS_COMPONENT_ID,
            b"psk id".to_vec().into(),
        ));
        let psk_id =
            PreSharedKeyId::application(APQMLS_COMPONENT_ID, b"psk id".to_vec(), b"nonce".to_vec());
        assert_eq!(psk_id.psk(), &psk);
        let proposal = PreSharedKeyProposal::new(psk_id.clone());
        assert_eq!(psk_id_of(&proposal).unwrap(), psk_id);
    }
}
