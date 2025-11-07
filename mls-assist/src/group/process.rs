// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::prelude::{ContentType, OpenMlsCrypto, ProtocolMessage, Verifiable};

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
        let sender = processed_message.sender().clone();
        let confirmation_tag = commit
            .confirmation_tag()
            .ok_or(LibraryError::LibraryError)?
            .clone();
        let ProcessedMessageContent::StagedCommitMessage(staged_commit) =
            processed_message.content()
        else {
            return Err(ProcessAssistedMessageError::LibraryError(
                LibraryError::LibraryError, // Mismatching message type
            ));
        };
        let assisted_sender = match sender {
            Sender::Member(leaf_index) => AssistedSender::Member(leaf_index),
            Sender::NewMemberCommit => {
                // If it's a new member commit, we can figure out the signature
                // key of the sender by looking at the add proposal.
                let Some(external_add) = staged_commit.update_path_leaf_node() else {
                    return Err(ProcessAssistedMessageError::UnknownSender);
                };
                let signature_key = external_add.signature_key().clone();
                AssistedSender::External(signature_key)
            }
            Sender::External(_) | Sender::NewMemberProposal => {
                return Err(ProcessAssistedMessageError::LibraryError(
                    LibraryError::LibraryError, // Invalid sender after validation.
                ));
            }
        };
        let group_info: GroupInfo = self.validate_group_info(
            provider,
            assisted_sender,
            staged_commit,
            confirmation_tag,
            assisted_group_info,
        )?;
        // This is really only relevant for the "Full" group info case above.
        if group_info.group_context() != staged_commit.group_context() {
            return Err(ProcessAssistedMessageError::InconsistentGroupContext);
        }
        let processed_assisted_message =
            ProcessedAssistedMessage::Commit(processed_message, group_info);
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

// Helper functions
impl Group {
    fn validate_group_info<CryptoProvider: OpenMlsCrypto>(
        &self,
        provider: &CryptoProvider,
        sender: AssistedSender,
        staged_commit: &StagedCommit,
        confirmation_tag: ConfirmationTag,
        assisted_group_info: AssistedGroupInfoIn,
    ) -> Result<GroupInfo, ProcessAssistedMessageError> {
        let signature_scheme = self.group_info().group_context().ciphersuite().into();
        let (sender_index, sender_pk) = match sender {
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
                    .ok_or(ProcessAssistedMessageError::UnknownSender)?;
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

        verifiable_group_info
            .verify(provider, &sender_pk)
            .map_err(|_| ProcessAssistedMessageError::InvalidGroupInfoSignature)
    }
}
