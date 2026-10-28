// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::mls_group_config::GROUP_DATA_EXTENSION_TYPE;
use airprotos::client::{app_data::GroupAppData, group::GroupData};
use openmls::prelude::{Extension, Proposal, StagedCommit};

use crate::{chats::GroupDataExt, groups::Group};

impl Group {
    pub(crate) fn group_data(&self) -> anyhow::Result<Option<GroupData>> {
        // First try to get the group data from the group profile component.
        if let Some(group_profile) = GroupAppData::group_profile(self.mls_group().extensions()) {
            return Ok(Some(group_profile.into()));
        }
        // Otherwise fall back to the group data extension.
        let Some(bytes) = self.mls_group().extensions().iter().find_map(|e| match e {
            Extension::Unknown(GROUP_DATA_EXTENSION_TYPE, extension) => Some(&extension.0),
            _ => None,
        }) else {
            return Ok(None);
        };
        Ok(Some(GroupData::decode(bytes)?))
    }

    /// Group data changed by `staged_commit` if any.
    ///
    /// If the staged group context carries a group profile component, only a change of that
    /// component counts. A legacy group data extension in the same commit is ignored. Without a
    /// component, the legacy extension of a group context extensions proposal is returned.
    pub(super) fn group_data_from_staged_commit(
        &self,
        staged_commit: &StagedCommit,
    ) -> anyhow::Result<Option<GroupData>> {
        if let Some(staged) =
            GroupAppData::group_profile(staged_commit.group_context().extensions())
        {
            let current = GroupAppData::group_profile(self.mls_group().extensions());
            return Ok((current.as_ref() != Some(&staged)).then(|| staged.into()));
        }

        // Fallback
        staged_commit
            .queued_proposals()
            .find_map(|p| {
                if let Proposal::GroupContextExtensions(extensions) = p.proposal()
                    && let Some(extension) =
                        extensions.extensions().unknown(GROUP_DATA_EXTENSION_TYPE)
                {
                    Some(GroupData::decode(&extension.0))
                } else {
                    None
                }
            })
            .transpose()
            .map_err(From::from)
    }
}
