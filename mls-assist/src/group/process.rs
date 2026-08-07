// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use apqmls::{
    messages::{ApqGroupInfo, ApqProtocolMessage},
    public_group::ApqPublicGroupMut,
};
use openmls::{
    component::ComponentData,
    group::{AppDataDictionaryUpdater, AppDataUpdates, ResolveAppDataCommitError},
    messages::proposals::{AppDataUpdateOperation, AppDataUpdateProposal},
    prelude::{ContentType, Credential, OpenMlsCrypto, ProtocolMessage, Verifiable},
};

use crate::group::{apq::ApqGroupRef, errors::ProcessApqAssistedMessageError};

use super::{errors::LibraryError, *};

impl Group {
    /// Returns a [`ProcessedMessage`] for inspection.
    pub fn process_assisted_message<CryptoProvider: OpenMlsCrypto>(
        &self,
        provider: &CryptoProvider,
        assisted_message: AssistedMessageIn,
    ) -> Result<ProcessedAssistedMessagePlus, ProcessAssistedMessageError> {
        let (commit, assisted_group_info) = match assisted_message.mls_message {
            ProtocolMessage::PrivateMessage(private_message) => {
                // We can't process private messages using the PublicGroup, so
                // we just forward them.
                let processed_assisted_message =
                    ProcessedAssistedMessage::PrivateMessage(private_message);
                let message_plus = ProcessedAssistedMessagePlus {
                    processed_assisted_message,
                    serialized_mls_message: assisted_message.serialized_mls_message,
                };
                return Ok(message_plus);
            }
            ProtocolMessage::PublicMessage(pm) => {
                match pm.content_type() {
                    ContentType::Application => {
                        // Public messages can't be application messages.
                        return Err(ProcessAssistedMessageError::InvalidAssistedMessage);
                    }
                    ContentType::Proposal => {
                        // Proposals are fed to the PublicGroup s.t. they are
                        // put into the ProposalStore. Otherwise we don't do
                        // anything with them.
                        let processed_message = self.public_group.process_message(provider, *pm)?;
                        let processed_message = resolve_app_data_commit_public(
                            &self.public_group,
                            provider,
                            processed_message,
                        )?;
                        let processed_assisted_message =
                            ProcessedAssistedMessage::NonCommit(processed_message);
                        let message_plus = ProcessedAssistedMessagePlus {
                            processed_assisted_message,
                            serialized_mls_message: assisted_message.serialized_mls_message,
                        };
                        return Ok(message_plus);
                    }
                    ContentType::Commit => {
                        // If it's a commit, we make sure there is a group info present.
                        let assisted_group_info = match assisted_message.group_info_option {
                            Some(agi) => agi,
                            None => {
                                return Err(ProcessAssistedMessageError::InvalidAssistedMessage);
                            }
                        };
                        (pm, assisted_group_info)
                    }
                }
            }
        };
        // First process the message, then verify that the group info
        // checks out.
        let processed_message = self
            .public_group
            .process_message(provider, ProtocolMessage::PublicMessage(commit.clone()))?;
        let processed_message =
            resolve_app_data_commit_public(&self.public_group, provider, processed_message)?;
        let confirmation_tag = commit
            .confirmation_tag()
            .ok_or(LibraryError::LibraryError)?
            .clone();
        let validation_params =
            ValidateGroupInfoParams::from_processed_commit(confirmation_tag, &processed_message)?;
        let group_info: GroupInfo =
            self.validate_group_info(provider, validation_params, assisted_group_info)?;
        let processed_assisted_message =
            ProcessedAssistedMessage::Commit(processed_message, Box::new(group_info));
        let message_plus = ProcessedAssistedMessagePlus {
            processed_assisted_message,
            serialized_mls_message: assisted_message.serialized_mls_message,
        };
        Ok(message_plus)
    }
}

enum AssistedSender {
    Member(LeafNodeIndex),
    External(SignaturePublicKey),
}

impl ApqGroupRef<'_> {
    /// Process incoming APQ assisted message.
    ///
    /// Similar to [`Group::process_assisted_message`], but for APQ.
    pub fn process_apq_assisted_message<CryptoProvider: OpenMlsCrypto>(
        &mut self,
        crypto: &CryptoProvider,
        t_assisted_message: AssistedMessageIn,
        pq_assisted_message: AssistedMessageIn,
        sender_equivalence: impl Fn(&Credential, &Credential) -> bool,
    ) -> Result<ApqProcessedAssistedMessagePlus, ProcessApqAssistedMessageError> {
        // APQ only supports public commits
        let extract_public_commit =
            |AssistedMessageIn {
                 mls_message,
                 serialized_mls_message,
                 group_info_option,
             }: AssistedMessageIn| match mls_message {
                ProtocolMessage::PrivateMessage(_) => None,
                ProtocolMessage::PublicMessage(pm) => match pm.content_type() {
                    ContentType::Application => None,
                    ContentType::Proposal => None,
                    ContentType::Commit => {
                        // Group info is required for public commits
                        Some((pm, serialized_mls_message, group_info_option?))
                    }
                },
            };

        let Some((pq_commit, pq_serialized_message, pq_group_info)) =
            extract_public_commit(pq_assisted_message)
        else {
            return Err(ProcessApqAssistedMessageError::InvalidAssistedMessage);
        };

        let Some((t_commit, t_serialized_message, t_group_info)) =
            extract_public_commit(t_assisted_message)
        else {
            return Err(ProcessApqAssistedMessageError::InvalidAssistedMessage);
        };

        let t_confirmation_tag = t_commit
            .confirmation_tag()
            .ok_or(LibraryError::LibraryError)?
            .clone();
        let pq_confirmation_tag = pq_commit
            .confirmation_tag()
            .ok_or(LibraryError::LibraryError)?
            .clone();

        let apq_message = ApqProtocolMessage::new(
            ProtocolMessage::PublicMessage(t_commit),
            ProtocolMessage::PublicMessage(pq_commit),
        );

        let mut apq_group = ApqPublicGroupMut::from_groups(
            &mut self.t_group.public_group,
            &mut self.pq_group.public_group,
        );
        let apq_processed = apq_group.process_message(crypto, apq_message, sender_equivalence)?;

        let t_validation_params = ValidateGroupInfoParams::from_processed_commit(
            t_confirmation_tag,
            &apq_processed.t_message,
        )?;
        let t_group_info =
            self.t_group
                .validate_group_info(crypto, t_validation_params, t_group_info)?;

        let pq_validation_params = ValidateGroupInfoParams::from_processed_commit(
            pq_confirmation_tag,
            &apq_processed.pq_message,
        )?;
        let pq_group_info =
            self.pq_group
                .validate_group_info(crypto, pq_validation_params, pq_group_info)?;

        let group_info = ApqGroupInfo::new(t_group_info, pq_group_info);

        let processed_assisted_message = ApqProcessedAssistedMessage {
            processed_message: apq_processed,
            group_info,
        };

        let serialized_apq_message =
            SerializedMlsMessage::combine_apq(t_serialized_message, pq_serialized_message);
        Ok(ApqProcessedAssistedMessagePlus {
            processed_assisted_message,
            serialized_apq_message,
        })
    }
}

struct ValidateGroupInfoParams<'a> {
    assisted_sender: AssistedSender,
    staged_commit: &'a StagedCommit,
    confirmation_tag: ConfirmationTag,
}

impl<'a> ValidateGroupInfoParams<'a> {
    fn from_processed_commit(
        confirmation_tag: ConfirmationTag,
        processed: &'a ProcessedMessage,
    ) -> Result<Self, LibraryError> {
        let ProcessedMessageContent::StagedCommitMessage(staged_commit) = processed.content()
        else {
            // Mismatching message type
            return Err(LibraryError::LibraryError);
        };

        let assisted_sender = match processed.sender() {
            Sender::Member(leaf_index) => AssistedSender::Member(*leaf_index),
            Sender::External(_) | Sender::NewMemberProposal => {
                return Err(LibraryError::LibraryError);
            }
            Sender::NewMemberCommit => {
                // If it's a new member commit, we can figure out the signature key of the
                // sender by looking at the add proposal.
                let Some(external_add) = staged_commit.update_path_leaf_node() else {
                    return Err(LibraryError::LibraryError);
                };
                let signature_key = external_add.signature_key().clone();
                AssistedSender::External(signature_key)
            }
        };

        Ok(Self {
            staged_commit,
            assisted_sender,
            confirmation_tag,
        })
    }
}

// Helper functions
impl Group {
    fn validate_group_info<CryptoProvider: OpenMlsCrypto>(
        &self,
        provider: &CryptoProvider,
        ValidateGroupInfoParams {
            assisted_sender,
            staged_commit,
            confirmation_tag,
        }: ValidateGroupInfoParams,
        assisted_group_info: AssistedGroupInfoIn,
    ) -> Result<GroupInfo, GroupInfoValidationError> {
        let signature_scheme = self.group_info().group_context().ciphersuite().into();
        let (sender_index, sender_pk) = match assisted_sender {
            AssistedSender::Member(index) => {
                let sender_pk = self
                    .public_group
                    .members()
                    .find_map(|m| {
                        if m.index == index {
                            Some(m.signature_key)
                        } else {
                            None
                        }
                    })
                    .map(|pk_bytes| {
                        OpenMlsSignaturePublicKey::from_signature_key(
                            pk_bytes.into(),
                            signature_scheme,
                        )
                    })
                    .ok_or(GroupInfoValidationError::UnknownSender)?;
                (index, sender_pk)
            }
            AssistedSender::External(signature_public_key) => {
                let index = self
                    .public_group
                    .ext_commit_sender_index(staged_commit)
                    .map_err(LibraryError::OpenMlsLibraryError)?;
                let openmls_signature_key = OpenMlsSignaturePublicKey::from_signature_key(
                    signature_public_key,
                    signature_scheme,
                );
                (index, openmls_signature_key)
            }
        };
        let verifiable_group_info = assisted_group_info.into_verifiable_group_info(
            sender_index,
            staged_commit.group_context().clone(),
            confirmation_tag,
        );

        let group_info = verifiable_group_info
            .verify(provider, &sender_pk)
            .map_err(|_| GroupInfoValidationError::InvalidGroupInfoSignature)?;

        // This is really only relevant for the "Full" group info case above
        if group_info.group_context() != staged_commit.group_context() {
            return Err(GroupInfoValidationError::InconsistentGroupContext);
        }

        Ok(group_info)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Clone)]
pub enum GroupInfoValidationError {
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
    #[error(transparent)]
    OpenMlsLibraryError(#[from] openmls::prelude::LibraryError),
    #[error("Unknown sender")]
    UnknownSender,
    #[error("Invalid group info signature")]
    InvalidGroupInfoSignature,
    #[error("Group context is inconsistent between assisted group info and staged commit")]
    InconsistentGroupContext,
}

/// Resolves an [`UnresolvedAppDataCommit`] into a [`ProcessedMessage`].
fn resolve_app_data_commit_public<Crypto: OpenMlsCrypto>(
    group: &PublicGroup,
    crypto: &Crypto,
    message: ProcessedMessage,
) -> Result<ProcessedMessage, ResolveAppDataCommitError> {
    let ProcessedMessageContent::UnresolvedAppDataCommit(unresolved) = message.content() else {
        return Ok(message);
    };
    let updates = compute_app_data_updates(
        group.app_data_dictionary_updater(),
        unresolved.app_data_update_proposals(),
    );
    group.resolve_app_data_commit(crypto, message, updates)
}

fn compute_app_data_updates<'a>(
    mut updater: AppDataDictionaryUpdater<'a>,
    proposals: impl Iterator<Item = &'a AppDataUpdateProposal>,
) -> Option<AppDataUpdates> {
    let mut updated = false;
    for proposal in proposals {
        match proposal.operation() {
            AppDataUpdateOperation::Update(data) => {
                updater.set(ComponentData::from_parts(
                    proposal.component_id(),
                    data.clone(),
                ));
            }
            AppDataUpdateOperation::Remove => {
                updater.remove(&proposal.component_id());
            }
        }
        updated = true;
    }
    updated.then(|| updater.changes()).flatten()
}
