// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use aircommon::{
    credentials::{LeafCredential, RoomPolicyIdentity},
    identifiers::UserId,
    utils::removed_client,
};
use anyhow::{Result, ensure};
use mimi_room_policy::{MimiProposal, RoleIndex, VerifiedRoomState};
use openmls::prelude::{LeafNodeIndex, MlsGroup, Proposal, QueuedProposal, Sender, StagedCommit};
use tracing::warn;

use crate::groups::Group;

impl Group {
    /// Returns a list of (remover, removed) UserId pairs for pending remove proposals.
    pub(crate) fn pending_removes(&self) -> Vec<(UserId, UserId)> {
        self.compile_removed_list(self.mls_group().pending_proposals(), |index| {
            self.user_id_at_index(index)
        })
    }

    /// Whether the proposal store holds a self-remove proposal from our own leaf.
    pub(crate) fn has_pending_own_self_remove(&self) -> bool {
        let own_index = self.mls_group().own_leaf_index();
        self.mls_group().pending_proposals().any(|p| {
            if let Proposal::SelfRemove = p.proposal()
                && let Sender::Member(index) = p.sender()
            {
                *index == own_index
            } else {
                false
            }
        })
    }

    pub(super) fn staged_commit_removes(
        &self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Vec<(RoomPolicyIdentity, RoomPolicyIdentity)> {
        let Some(staged_commit) = staged_commit.or_else(|| self.mls_group().pending_commit())
        else {
            return Vec::new();
        };
        self.compile_removed_list(staged_commit.queued_proposals(), |index| {
            self.room_identity_at_index(index)
        })
    }

    /// Collects (remover, removed) pairs from remove proposals, resolving each leaf index with
    /// `resolve`. UI paths resolve to user ids, room-state paths to room-policy identities.
    fn compile_removed_list<'a, T>(
        &self,
        removes: impl Iterator<Item = &'a QueuedProposal>,
        resolve: impl Fn(LeafNodeIndex) -> Option<T>,
    ) -> Vec<(T, T)> {
        let mut pending_removes = Vec::new();

        for proposal in removes {
            let Sender::Member(remover) = proposal.sender() else {
                // We don't support external senders yet.
                continue;
            };
            let Some(remover) = resolve(*remover) else {
                continue;
            };
            if let Some(removed_client_index) = removed_client(proposal)
                && let Some(removed) = resolve(removed_client_index)
            {
                pending_removes.push((remover, removed));
            }
        }
        pending_removes
    }

    fn user_id_at_index(&self, index: LeafNodeIndex) -> Option<UserId> {
        self.mls_group().member_at(index).and_then(|m| {
            LeafCredential::from_credential(&m.credential)
                .map(|c| c.user_id(self.own_user_id()).clone())
                .ok()
        })
    }

    fn room_identity_at_index(&self, index: LeafNodeIndex) -> Option<RoomPolicyIdentity> {
        self.mls_group().member_at(index).and_then(|m| {
            LeafCredential::from_credential(&m.credential)
                .ok()
                .map(|c| c.room_policy_identity())
        })
    }

    /// Returns a list of (adder, added) room-policy identity pairs for pending add proposals.
    pub(crate) fn pending_adds(
        &self,
        staged_commit: Option<&'_ StagedCommit>,
    ) -> Vec<(RoomPolicyIdentity, RoomPolicyIdentity)> {
        let staged_commit = staged_commit.or_else(|| self.mls_group().pending_commit());
        let mut pending_adds = Vec::new();
        let Some(pending_commit) = staged_commit else {
            return pending_adds;
        };
        for proposal in pending_commit.add_proposals() {
            let Sender::Member(adder_index) = proposal.sender() else {
                // We don't support external senders yet.
                continue;
            };
            let Some(adder) = self.room_identity_at_index(*adder_index) else {
                continue;
            };
            let Ok(added_credential) = LeafCredential::from_credential(
                proposal
                    .add_proposal()
                    .key_package()
                    .leaf_node()
                    .credential(),
            ) else {
                continue;
            };
            pending_adds.push((adder, added_credential.room_policy_identity()));
        }
        pending_adds
    }

    pub(crate) fn verify_role_change_identity(
        &self,
        sender: &RoomPolicyIdentity,
        target: &RoomPolicyIdentity,
        role: RoleIndex,
    ) -> Result<()> {
        let sender = sender.to_bytes()?;
        let target = target.to_bytes()?;
        let result = self
            .room_state
            .can_apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }]);

        Ok(result?)
    }

    pub(crate) fn verify_role_change(
        &self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Result<()> {
        self.verify_role_change_identity(
            &RoomPolicyIdentity::User(sender.clone()),
            &RoomPolicyIdentity::User(target.clone()),
            role,
        )
    }

    pub(crate) fn room_state_change_role_identity(
        &mut self,
        sender: &RoomPolicyIdentity,
        target: &RoomPolicyIdentity,
        role: RoleIndex,
    ) -> Result<()> {
        let sender = sender.to_bytes()?;
        let target = target.to_bytes()?;
        let result = self
            .room_state
            .apply_regular_proposals(&sender, &[MimiProposal::ChangeRole { target, role }]);

        Ok(result?)
    }

    pub(crate) fn room_state_change_role(
        &mut self,
        sender: &UserId,
        target: &UserId,
        role: RoleIndex,
    ) -> Result<()> {
        self.room_state_change_role_identity(
            &RoomPolicyIdentity::User(sender.clone()),
            &RoomPolicyIdentity::User(target.clone()),
            role,
        )
    }

    /// The role of `user_id` in the room state, or `None` if it is not listed.
    pub(crate) fn room_state_role(&self, user_id: &UserId) -> Result<Option<RoleIndex>> {
        let identity = RoomPolicyIdentity::User(user_id.clone()).to_bytes()?;
        Ok(self.room_state.users().get(&identity).cloned())
    }
}

/// Ensure that every user of `room_state` is a member of `mls_group`.
///
/// Members missing from the room state are tolerated: the DS does not add external joiners of
/// connection groups to its room state, clients patch that locally.
pub(super) fn ensure_room_state_users_are_members(
    room_state: &VerifiedRoomState,
    mls_group: &MlsGroup,
) -> anyhow::Result<()> {
    let mut members = HashSet::new();
    for member in mls_group.members() {
        let identity = LeafCredential::from_credential(&member.credential)?
            .room_policy_identity()
            .to_bytes()?;
        members.insert(identity);
    }
    let users = room_state.users();
    ensure!(
        users.keys().all(|identity| members.contains(identity)),
        "room state lists users which are not group members"
    );
    if users.len() < members.len() {
        warn!(
            group_id = ?mls_group.group_id(),
            "room state is missing group members",
        );
    }
    Ok(())
}

/// Record the role change that admits us to a connection group.
///
/// The room state the DS serves us predates our join, so it does not list us
/// yet.
pub(super) fn add_joiner_to_room_state(
    room_state: &mut VerifiedRoomState,
    inviter: &UserId,
    joiner: &UserId,
) -> anyhow::Result<()> {
    let sender = RoomPolicyIdentity::User(inviter.clone()).to_bytes()?;
    let target = RoomPolicyIdentity::User(joiner.clone()).to_bytes()?;
    room_state.apply_regular_proposals(
        &sender,
        &[MimiProposal::ChangeRole {
            target,
            role: RoleIndex::Regular,
        }],
    )?;
    Ok(())
}

/// Ensure that `room_state` lists exactly the members of `mls_group`.
pub(super) fn ensure_room_state_matches_members(
    room_state: &VerifiedRoomState,
    mls_group: &MlsGroup,
) -> anyhow::Result<()> {
    let mut members = HashSet::new();
    for member in mls_group.members() {
        let identity = LeafCredential::from_credential(&member.credential)?
            .room_policy_identity()
            .to_bytes()?;
        members.insert(identity);
    }

    let users: HashSet<_> = room_state.users().keys().cloned().collect();
    ensure!(
        users.is_subset(&members),
        "room state lists users which are not group members"
    );
    ensure!(
        members.is_subset(&users),
        "room state is missing group members"
    );
    Ok(())
}
