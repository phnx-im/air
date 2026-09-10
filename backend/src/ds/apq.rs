// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structural checks shared by the DS handlers of APQ external commits.

use aircommon::credentials::LeafCredential;
use mls_assist::{
    group::ApqProcessedAssistedMessage,
    openmls::prelude::{LeafNodeIndex, ProcessedMessageContent, Sender, StagedCommit},
};
use tracing::error;

use crate::errors::ResyncClientError;

use super::{group_state::DsGroupState, join_connection_group::JoinConnectionGroupError};

/// An APQ external commit that passed the checks in [`DsGroupState::validate_apq_external_commit`].
pub(super) struct ApqExternalCommit<'a> {
    pub(super) t_staged_commit: &'a StagedCommit,
    pub(super) pq_staged_commit: &'a StagedCommit,
    /// AAD of the T leg. The PQ leg carries the same bytes.
    pub(super) aad: &'a [u8],
    /// Credential of the T update-path leaf. The PQ leaf is bound to it by the
    /// shared signature key.
    pub(super) new_credential: LeafCredential,
    /// Leaf index the committer lands at, equal on both legs.
    pub(super) new_sender_index: LeafNodeIndex,
}

/// The commit failed one of the checks in
/// [`DsGroupState::validate_apq_external_commit`].
#[derive(Debug)]
pub(super) struct InvalidApqExternalCommit;

impl From<InvalidApqExternalCommit> for ResyncClientError {
    fn from(_: InvalidApqExternalCommit) -> Self {
        Self::InvalidMessage
    }
}

impl From<InvalidApqExternalCommit> for JoinConnectionGroupError {
    fn from(_: InvalidApqExternalCommit) -> Self {
        Self::InvalidMessage("invalid APQ external commit")
    }
}

impl DsGroupState {
    /// Checks that `processed` is an external commit advancing both legs
    /// consistently: a staged commit from a new member on each leg, the
    /// self-group flag unchanged, and the T and PQ update paths signed with the
    /// same key and landing at the same leaf index.
    pub(super) fn validate_apq_external_commit<'a>(
        t: &Self,
        pq: &Self,
        processed: &'a ApqProcessedAssistedMessage,
    ) -> Result<ApqExternalCommit<'a>, InvalidApqExternalCommit> {
        let t_processed_message = &processed.processed_message.t_message;
        let pq_processed_message = &processed.processed_message.pq_message;

        let (
            ProcessedMessageContent::StagedCommitMessage(t_staged_commit),
            ProcessedMessageContent::StagedCommitMessage(pq_staged_commit),
        ) = (
            t_processed_message.content(),
            pq_processed_message.content(),
        )
        else {
            error!("Invalid message content; expected staged commit");
            return Err(InvalidApqExternalCommit);
        };
        let (t_staged_commit, pq_staged_commit) =
            (t_staged_commit.as_ref(), pq_staged_commit.as_ref());

        if !t.self_group_flag_unchanged(t_staged_commit)
            || !pq.self_group_flag_unchanged(pq_staged_commit)
        {
            error!("Commit would toggle the self-group flag");
            return Err(InvalidApqExternalCommit);
        }

        let (Sender::NewMemberCommit, Sender::NewMemberCommit) =
            (t_processed_message.sender(), pq_processed_message.sender())
        else {
            error!("Invalid sender; expected new member commit");
            return Err(InvalidApqExternalCommit);
        };

        // Bind the two legs at the new leaf: the T and PQ update paths must be
        // signed with the same signature key.
        let t_new_leaf = t_staged_commit.update_path_leaf_node().ok_or_else(|| {
            error!("T update path leaf node not found");
            InvalidApqExternalCommit
        })?;
        let pq_new_leaf = pq_staged_commit.update_path_leaf_node().ok_or_else(|| {
            error!("PQ update path leaf node not found");
            InvalidApqExternalCommit
        })?;
        if t_new_leaf.signature_key() != pq_new_leaf.signature_key() {
            error!("T and PQ update path signature keys do not match");
            return Err(InvalidApqExternalCommit);
        }
        let new_credential =
            LeafCredential::from_credential(t_new_leaf.credential()).map_err(|error| {
                error!(%error, "Update path leaf credential is invalid");
                InvalidApqExternalCommit
            })?;

        let new_sender_index =
            t.group()
                .ext_commit_sender_index(t_staged_commit)
                .map_err(|error| {
                    error!(%error, "Error getting T sender index");
                    InvalidApqExternalCommit
                })?;
        let pq_new_sender_index = pq
            .group()
            .ext_commit_sender_index(pq_staged_commit)
            .map_err(|error| {
                error!(%error, "Error getting PQ sender index");
                InvalidApqExternalCommit
            })?;
        if new_sender_index != pq_new_sender_index {
            error!("T and PQ sender indices do not match");
            return Err(InvalidApqExternalCommit);
        }

        Ok(ApqExternalCommit {
            t_staged_commit,
            pq_staged_commit,
            aad: t_processed_message.tail_aad(),
            new_credential,
            new_sender_index,
        })
    }
}
