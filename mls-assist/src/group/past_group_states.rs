// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Legacy retained ratchet trees.
//! They're now stored in DS as [`DsWelcomeInfo`].

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use openmls::{
    prelude::{GroupEpoch, SignaturePublicKey},
    treesync::RatchetTree,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct PastGroupState {
    nodes: RatchetTree,
    creation_time: DateTime<Utc>,
    potential_joiners: HashSet<SignaturePublicKey>,
}

impl PastGroupState {
    /// Get the nodes of this group state.
    fn nodes(&self) -> &RatchetTree {
        &self.nodes
    }

    /// Returns true if the given joiner is authorized to obtain this group state.
    fn is_authorized(&self, joiner: &SignaturePublicKey) -> bool {
        self.potential_joiners.contains(joiner)
    }
}

/// One legacy entry, moved out of the group state for migration.
pub struct LegacyPastGroupState {
    pub epoch: GroupEpoch,
    pub ratchet_tree: RatchetTree,
    pub potential_joiners: Vec<SignaturePublicKey>,
    pub creation_time: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Default)]
pub(super) struct PastGroupStates {
    past_group_states: HashMap<GroupEpoch, PastGroupState>,
}

impl PastGroupStates {
    /// Get the nodes of the past group state with the given epoch for the given
    /// joiner. Returns `None` if there is no past group state for that epoch
    /// and the given joiner.
    pub(crate) fn get_for_joiner(
        &self,
        epoch: &GroupEpoch,
        joiner: &SignaturePublicKey,
    ) -> Option<&RatchetTree> {
        self.past_group_states
            .get(epoch)
            .and_then(|past_group_state| {
                // Check if the joiner is authorized to get these nodes.
                if past_group_state.is_authorized(joiner) {
                    Some(past_group_state.nodes())
                } else {
                    None
                }
            })
    }

    pub(super) fn take_all_past_group_states(&mut self) -> Vec<LegacyPastGroupState> {
        self.past_group_states
            .drain()
            .map(|(epoch, state)| LegacyPastGroupState {
                epoch,
                ratchet_tree: state.nodes,
                potential_joiners: state.potential_joiners.into_iter().collect(),
                creation_time: state.creation_time,
            })
            .collect()
    }
}
