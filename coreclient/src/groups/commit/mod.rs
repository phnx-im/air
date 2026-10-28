// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod delete;
pub(crate) mod invite;
pub(crate) mod leave;
pub(crate) mod merge;
pub(crate) mod remove;
pub(crate) mod update;

use anyhow::{Context, Result};
use openmls::{
    group::{CreateCommitError, ProposalValidationError},
    prelude::GroupId,
    treesync::errors::LeafNodeValidationError,
};

use crate::{clients::own_client_info::OwnClientInfo, db::access::ReadConnection, groups::Group};

impl Group {
    /// The emulation group a commit replacing our leaf has to derive from, or
    /// `None` if this leaf is not shared with sibling emulator clients.
    ///
    /// The emulation group is the self group. openmls resolves the concrete
    /// derivation epoch from it, always taking the newest one.
    pub(super) async fn resolve_vc_emulation_group(
        &self,
        connection: impl ReadConnection,
    ) -> Result<Option<GroupId>> {
        if !self.own_leaf_is_virtual_client() {
            return Ok(None);
        }
        let group_id = OwnClientInfo::load_self_group_id(connection)
            .await?
            .context("no self group to derive the emulation epoch from")?;
        Ok(Some(group_id))
    }
}

pub(super) fn to_capabilities_mismatch(
    error: CreateCommitError,
) -> anyhow::Result<LeafNodeValidationError> {
    use LeafNodeValidationError::*;
    match error {
        CreateCommitError::LeafNodeValidation(error)
        | CreateCommitError::ProposalValidationError(
            ProposalValidationError::LeafNodeValidation(error),
        ) if matches!(
            error,
            UnsupportedExtensions
                | UnsupportedProposals
                | UnsupportedCredentials
                | CiphersuiteNotInCapabilities
                | CredentialNotInCapabilities
                | ExtensionsNotInCapabilities
                | LeafNodeCredentialNotSupportedByMember
                | MemberCredentialNotSupportedByLeafNode,
        ) =>
        {
            Ok(error)
        }
        other => Err(other.into()),
    }
}
