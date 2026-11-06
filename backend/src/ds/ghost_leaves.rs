// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Detection of leaves that a group's tree holds although the DS removed them.
//!
//! OpenMLS before 60c7dbb0 (GHSA-94v4-rg25-x625) brought back leaves that an
//! external commit removed, if the removals truncated the tree and the joiner's
//! leaf regrew it. The DS dropped the member profiles of those leaves, so they
//! show up as occupied leaves without a profile. A resurrected remove-prior leaf
//! shares its signature key with the resynced client's new leaf. A resurrected
//! self-remove leaf has a unique key.

use std::collections::{BTreeSet, HashMap};

use mls_assist::openmls::prelude::LeafNodeIndex;
use tracing::warn;

use crate::pseudonym::Pseudonym;

use super::group_state::DsGroupState;

/// Occupied leaves of a group's tree that have no member profile on the DS.
#[derive(Debug, Default, PartialEq, Eq)]
struct UnprofiledLeaves {
    /// Leaves whose signature key another leaf shares.
    duplicate_key: Vec<LeafNodeIndex>,
    /// Leaves whose signature key is unique in the tree.
    unique_key: Vec<LeafNodeIndex>,
}

impl UnprofiledLeaves {
    /// Finds the occupied `leaves`, given as index and signature key, that
    /// aren't in `profiled`. Both lists are sorted by index.
    fn new(leaves: &[(LeafNodeIndex, Vec<u8>)], profiled: &BTreeSet<LeafNodeIndex>) -> Self {
        let mut key_counts: HashMap<&[u8], usize> = HashMap::new();
        for (_, signature_key) in leaves {
            *key_counts.entry(signature_key.as_slice()).or_default() += 1;
        }

        let mut unprofiled = Self::default();
        for (index, signature_key) in leaves {
            if profiled.contains(index) {
                continue;
            }
            if key_counts[signature_key.as_slice()] > 1 {
                unprofiled.duplicate_key.push(*index);
            } else {
                unprofiled.unique_key.push(*index);
            }
        }
        unprofiled.duplicate_key.sort_unstable();
        unprofiled.unique_key.sort_unstable();
        unprofiled
    }

    fn is_empty(&self) -> bool {
        self.duplicate_key.is_empty() && self.unique_key.is_empty()
    }
}

/// Returns the `removed` leaves that are still occupied after a commit merged,
/// apart from `joiner`, the index the commit added a leaf at.
fn resurrected_leaves(
    removed: impl IntoIterator<Item = LeafNodeIndex>,
    joiner: LeafNodeIndex,
    is_occupied: impl Fn(LeafNodeIndex) -> bool,
) -> Vec<LeafNodeIndex> {
    let mut resurrected: Vec<_> = removed
        .into_iter()
        .filter(|&index| index != joiner && is_occupied(index))
        .collect();
    resurrected.sort_unstable();
    resurrected.dedup();
    resurrected
}

fn as_u32(indices: &[LeafNodeIndex]) -> Vec<u32> {
    indices.iter().map(|index| index.u32()).collect()
}

impl DsGroupState {
    fn unprofiled_leaves(&self) -> UnprofiledLeaves {
        let leaves: Vec<_> = self
            .group()
            .members()
            .map(|member| (member.index, member.signature_key))
            .collect();
        let profiled: BTreeSet<_> = self.member_profiles.keys().copied().collect();
        UnprofiledLeaves::new(&leaves, &profiled)
    }

    fn is_pq_leg(&self) -> bool {
        self.apq_info().is_some_and(|info| {
            &info.pq_session_group_id == self.group().group_info().group_context().group_id()
        })
    }

    /// Logs a warning if the tree has occupied leaves without a member profile.
    ///
    /// Does nothing for the PQ leg of an APQ group, which keeps no profiles.
    pub(super) fn warn_about_unprofiled_leaves(&self, group: Pseudonym) {
        if self.is_pq_leg() {
            return;
        }
        let unprofiled = self.unprofiled_leaves();
        if unprofiled.is_empty() {
            return;
        }
        warn!(
            %group,
            epoch = self.group().epoch().as_u64(),
            leaf_count = self.group().members().count(),
            duplicate_key = ?as_u32(&unprofiled.duplicate_key),
            unique_key = ?as_u32(&unprofiled.unique_key),
            "Group tree has leaves without member profiles"
        );
    }

    /// Logs a warning if any of the `removed` leaves is still occupied after
    /// an external commit merged. `joiner` is the index of the committer's new
    /// leaf.
    pub(super) fn warn_about_resurrected_leaves(
        &self,
        context: &str,
        removed: impl IntoIterator<Item = LeafNodeIndex>,
        joiner: LeafNodeIndex,
    ) {
        let resurrected =
            resurrected_leaves(removed, joiner, |index| self.group().leaf(index).is_some());
        if resurrected.is_empty() {
            return;
        }
        warn!(
            context,
            epoch = self.group().epoch().as_u64(),
            %joiner,
            resurrected = ?as_u32(&resurrected),
            "Commit left removed leaves in the tree"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(index: u32, key: u8) -> (LeafNodeIndex, Vec<u8>) {
        (LeafNodeIndex::new(index), vec![key])
    }

    fn indices(indices: &[u32]) -> Vec<LeafNodeIndex> {
        indices.iter().copied().map(LeafNodeIndex::new).collect()
    }

    fn profiled(indices: &[u32]) -> BTreeSet<LeafNodeIndex> {
        indices.iter().copied().map(LeafNodeIndex::new).collect()
    }

    #[test]
    fn fully_profiled_tree_has_no_unprofiled_leaves() {
        let leaves = [leaf(0, 1), leaf(1, 2), leaf(3, 3)];

        let unprofiled = UnprofiledLeaves::new(&leaves, &profiled(&[0, 1, 3]));

        assert!(unprofiled.is_empty());
    }

    #[test]
    fn unprofiled_leaf_sharing_a_key_is_a_duplicate_key_leaf() {
        // Client with key 4 resynced from index 3 to index 2, and its old leaf
        // came back.
        let leaves = [leaf(0, 1), leaf(1, 2), leaf(2, 4), leaf(3, 4)];

        let unprofiled = UnprofiledLeaves::new(&leaves, &profiled(&[0, 1, 2]));

        assert_eq!(
            unprofiled,
            UnprofiledLeaves {
                duplicate_key: indices(&[3]),
                unique_key: Vec::new(),
            }
        );
    }

    #[test]
    fn unprofiled_leaf_with_a_unique_key_is_a_unique_key_leaf() {
        // The self-removed client at index 3 came back.
        let leaves = [leaf(0, 1), leaf(1, 2), leaf(2, 4), leaf(3, 3)];

        let unprofiled = UnprofiledLeaves::new(&leaves, &profiled(&[0, 1, 2]));

        assert_eq!(
            unprofiled,
            UnprofiledLeaves {
                duplicate_key: Vec::new(),
                unique_key: indices(&[3]),
            }
        );
    }

    #[test]
    fn profile_of_a_blank_leaf_is_ignored() {
        let leaves = [leaf(0, 1), leaf(1, 2)];

        let unprofiled = UnprofiledLeaves::new(&leaves, &profiled(&[0, 1, 2]));

        assert!(unprofiled.is_empty());
    }

    #[test]
    fn removed_leaves_that_stay_occupied_are_resurrected() {
        let occupied = indices(&[0, 1, 2, 3]);

        let resurrected = resurrected_leaves(indices(&[3, 2]), LeafNodeIndex::new(2), |index| {
            occupied.contains(&index)
        });

        assert_eq!(resurrected, indices(&[3]));
    }

    #[test]
    fn removed_leaves_that_are_blank_are_not_resurrected() {
        let occupied = indices(&[0, 1, 2]);

        let resurrected = resurrected_leaves(indices(&[3, 2]), LeafNodeIndex::new(2), |index| {
            occupied.contains(&index)
        });

        assert!(resurrected.is_empty());
    }
}
