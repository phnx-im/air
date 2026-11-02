// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod application_message;
pub(crate) mod client_auth_info;
pub(crate) mod client_references;
pub(crate) mod commit;
pub(crate) mod compatibility;
pub(crate) mod connection_psk;
pub(crate) mod create;
pub(crate) mod credentials;
pub(crate) mod debug_info;
// TODO: Allowing dead code here for now. We'll need diffs when we start
// rotating keys.
#[allow(dead_code)]
pub(crate) mod diff;
pub(crate) mod ds_teardown;
pub(crate) mod error;
pub(crate) mod group_bootstrap;
pub(crate) mod group_data;
pub(crate) mod join;
pub(crate) mod openmls_provider;
pub(crate) mod persistence;
pub(crate) mod process;
pub(crate) mod room_state;
pub(crate) mod self_group;
pub(crate) mod self_group_message_key;
#[cfg(feature = "test_utils")]
mod test_utils;
pub(crate) mod vc_epoch_retention;

pub(crate) use commit::{invite::PreparedInvitee, update::client_reference_extension};
pub(crate) use create::{NewGroupContext, PartialCreateGroupParams};
pub(crate) use ds_teardown::{handle_group_closed_on_ds, handle_group_not_found_on_ds};
pub(crate) use error::*;
pub(crate) use join::{
    DecryptedProfileInfos, ProfileInfo, external::ConnectionGroupJoin, welcome::JoinSigners,
};
pub(crate) use persistence::VerifiedGroup;

use aircommon::{
    credentials::LeafCredential,
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::UserId,
    time::TimeStamp,
};
use airprotos::client::app_data::ClientAppData;
use anyhow::{Context, Result, ensure};
use apqmls::extension::ApqInfo;
use mimi_room_policy::VerifiedRoomState;
use openmls::prelude::{GroupId, LeafNodeIndex, MlsGroup};
use tracing::error;

use crate::{
    ChatId,
    db::access::WriteConnection,
    groups::{application_message::SendMessageCollisionKey, diff::StagedGroupDiff},
};

/// Part of the [`Group`] that is only used for the post-quantum group.
#[derive(Debug)]
pub(crate) struct PqGroup {
    pub(crate) mls_group: MlsGroup,
    pub(crate) self_updated_at: Option<TimeStamp>,
}

impl PqGroup {
    pub(crate) fn group_id(&self) -> &GroupId {
        self.mls_group.group_id()
    }
}

/// A chat group as seen by this client.
///
/// `Group` bundles MLS state (`mls_group` and, for APQ groups, `pq`) with the
/// Air-level secrets and policy state that ride alongside MLS.
///
#[derive(Debug)]
pub(crate) struct Group {
    identity_link_wrapper_key: IdentityLinkWrapperKey,
    group_state_ear_key: GroupStateEarKey,
    mls_group: MlsGroup,
    /// `room_state` is the Air/MIMI room policy state. It must be advanced
    /// together with MLS proposal/commit application (see
    /// [`Group::apply_staged_operations_to_room_state`] and
    /// [`Group::merge_pending_commit`]).
    room_state: VerifiedRoomState,
    pending_diff: Option<StagedGroupDiff>, // Currently unused, but we're keeping it for later
    /// The time at which the user self-updated their key material in this group the last time
    pub(crate) self_updated_at: Option<TimeStamp>,
    pq: Option<PqGroup>,
    /// Set when a commit send fails non-transiently. Cleared on merge or discard.
    pending_commit_failed: bool,
    /// Symmetric key used as the PRK for collision-detection tag derivation.
    ///
    /// Set by the application on every group epoch change.
    send_message_collision_key: Option<SendMessageCollisionKey>,
    /// The user id of this client. Used to resolve the owner of self-group leaves, which carry no
    /// user identity of their own.
    own_user_id: UserId,
}

impl Group {
    pub(crate) fn is_apq(&self) -> bool {
        self.pq.is_some()
    }

    pub(crate) fn mls_group(&self) -> &MlsGroup {
        &self.mls_group
    }

    pub(crate) fn mls_group_mut(&mut self) -> &mut MlsGroup {
        &mut self.mls_group
    }

    pub(crate) fn pq(&self) -> Option<&PqGroup> {
        self.pq.as_ref()
    }

    /// Returns the PQ group ID of this group, if it is an APQ group.
    ///
    /// The ID is read from the APQMLS component in the T group's context extensions, so it is
    /// available even when the local PQ group state is missing (e.g. after a legacy T-only resync).
    pub(crate) fn pq_group_id(&self) -> Option<GroupId> {
        let info = ApqInfo::from_extensions(self.mls_group.extensions())
            .inspect_err(|error| error!(%error, "Failed to parse APQMLS component"))
            .ok()??;
        Some(info.pq_session_group_id)
    }

    pub(crate) fn pq_mut(&mut self) -> Option<&mut PqGroup> {
        self.pq.as_mut()
    }

    pub(crate) async fn mark_commit_failed(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> sqlx::Result<()> {
        if !self.pending_commit_failed {
            error!(group_id = ?self.group_id(), "Group is desynced");
            self.pending_commit_failed = true;
            self.store_pending_commit_failed(&mut connection).await?;

            if let Some(chat_id) =
                ChatId::load_from_group_id(&mut connection, self.group_id()).await?
            {
                connection.notifier().update(chat_id);
            }
        }

        Ok(())
    }

    pub(crate) async fn clear_commit_failed(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> sqlx::Result<()> {
        if self.pending_commit_failed {
            self.pending_commit_failed = false;
            self.store_pending_commit_failed(&mut connection).await?;

            if let Some(chat_id) =
                ChatId::load_from_group_id(&mut connection, self.group_id()).await?
            {
                connection.notifier().update(chat_id);
            }
        }
        Ok(())
    }

    /// Returns mutable references to the T MLS group and the PQ MLS group
    /// together, for callers that need to pass both into
    /// [`apqmls::commit_builder::CommitBuilder::from_groups`]. Errors for
    /// non-APQ groups.
    pub(crate) fn apq_mls_groups_mut(&mut self) -> Result<(&mut MlsGroup, &mut MlsGroup)> {
        let Self { mls_group, pq, .. } = self;
        let pq = pq.as_mut().context("No PQ group found")?;
        Ok((mls_group, &mut pq.mls_group))
    }

    /// Whether either leg holds a proposal that has not been committed yet.
    pub(crate) fn has_pending_proposals(&self) -> bool {
        self.mls_group.pending_proposals().next().is_some()
            || self
                .pq
                .as_ref()
                .is_some_and(|pq| pq.mls_group.pending_proposals().next().is_some())
    }

    /// Errors if this group (or its PQ counterpart, for APQ groups) has a
    /// pending commit.
    pub(crate) fn ensure_clean(&self) -> Result<()> {
        ensure!(
            self.mls_group.pending_commit().is_none(),
            "Room already had a pending commit"
        );
        if let Some(pq) = self.pq.as_ref() {
            ensure!(
                pq.mls_group.pending_commit().is_none(),
                "PQ Room already had a pending commit"
            );
        }
        Ok(())
    }

    /// Returns the [`ClientAppData`] from the leaf node of the given member, or `None` if the
    /// member is not in the group or its leaf carries no Air component.
    pub(crate) fn member_app_data(&self, user_id: &UserId) -> Option<ClientAppData> {
        let member = self.mls_group.members().find(|m| {
            LeafCredential::from_credential(&m.credential)
                .map(|c| c.user_id(self.own_user_id()) == user_id)
                .unwrap_or(false)
        })?;

        let leaf_node = self.mls_group.public_group().leaf(member.index)?;
        ClientAppData::from_leaf(leaf_node)
    }

    /// Get a reference to the group's group id.
    pub(crate) fn group_id(&self) -> &GroupId {
        self.mls_group().group_id()
    }

    pub(crate) fn own_user_id(&self) -> &UserId {
        &self.own_user_id
    }

    pub(crate) fn group_state_ear_key(&self) -> &GroupStateEarKey {
        &self.group_state_ear_key
    }

    pub(crate) fn identity_link_wrapper_key(&self) -> &IdentityLinkWrapperKey {
        &self.identity_link_wrapper_key
    }

    pub(crate) fn own_index(&self) -> LeafNodeIndex {
        self.mls_group().own_leaf_index()
    }

    /// Whether our leaf in this group is operated by a virtual client, i.e. it is
    /// shared with sibling emulator clients.
    pub(crate) fn own_leaf_is_virtual_client(&self) -> bool {
        self.mls_group()
            .own_leaf_node()
            .is_some_and(ClientAppData::leaf_is_virtual_client)
    }
}
