// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Utility functions for client and backend.

use mls_assist::openmls::{
    group::{QueuedProposal, StagedCommit},
    prelude::{LeafNodeIndex, Proposal, Sender},
};

/// Returns the removed client indices from a staged commit.
pub fn removed_clients(staged_commit: &StagedCommit) -> Vec<LeafNodeIndex> {
    staged_commit
        .queued_proposals()
        .filter_map(removed_client)
        .collect()
}

/// Returns the removed client index from a proposal, if it is a remove or self-remove proposal.
///
/// Returns `None` if the proposal is of a different type.
pub fn removed_client(p: &QueuedProposal) -> Option<LeafNodeIndex> {
    match p.proposal() {
        Proposal::Remove(remove) => Some(remove.removed()),
        Proposal::SelfRemove => {
            let Sender::Member(leaf_node_index) = p.sender() else {
                return None;
            };
            Some(*leaf_node_index)
        }
        _ => None,
    }
}
