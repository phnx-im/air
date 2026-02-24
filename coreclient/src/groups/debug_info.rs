// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Debug info for groups.

use std::collections::HashMap;

use aircommon::{
    credentials::VerifiableClientCredential,
    identifiers::{QualifiedGroupId, UserId},
    mls_group_config::{
        FRIENDSHIP_PACKAGE_PROPOSAL_TYPE, GROUP_DATA_EXTENSION_TYPE,
        QS_CLIENT_REFERENCE_EXTENSION_TYPE, SUPPORTED_PROTOCOL_VERSIONS,
    },
};
use anyhow::Context as _;
use openmls::prelude::{
    Capabilities, Ciphersuite, ExtensionType, ProposalType, RequiredCapabilitiesExtension,
};

use crate::{ChatId, UserProfile, clients::CoreUser, groups::Group};

impl CoreUser {
    /// Returns debug info for a group
    pub async fn chat_debug_info(&self, chat_id: ChatId) -> anyhow::Result<GroupDebugInfo> {
        let mut connection = self.pool().acquire().await?;
        let group = Group::load_with_chat_id(&mut connection, chat_id)
            .await?
            .context("Group not found")?;
        GroupDebugInfo::from_group(self, &group).await
    }
}

#[derive(Debug, Clone)]
pub struct GroupDebugInfo {
    pub group_id: String,
    pub epoch: u64,
    pub ciphersuite: String,
    pub versions: Vec<String>,
    pub own_leaf_index: u32,
    pub self_updated_at: Option<String>,
    pub pending_proposals: usize,
    pub has_pending_commit: bool,
    pub required_capabilities: Option<RequiredDebugCapabilities>,
    pub members: HashMap<u32, DebugCapabilities>,
}

#[derive(Debug, Clone)]
pub struct RequiredDebugCapabilities {
    pub extension_types: Vec<String>,
    pub proposal_types: Vec<String>,
    pub credential_types: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DebugCapabilities {
    pub user_id: String,
    pub display_name: String,
    pub versions: Vec<String>,
    pub ciphersuites: Vec<String>,
    pub extensions: Vec<String>,
    pub proposals: Vec<String>,
}

impl GroupDebugInfo {
    async fn from_group(core_user: &CoreUser, group: &Group) -> anyhow::Result<Self> {
        let group_id = QualifiedGroupId::try_from(group.group_id())?.to_string();
        let epoch = group.mls_group().epoch().as_u64();
        let ciphersuite = group.mls_group().ciphersuite().to_string();
        let versions = SUPPORTED_PROTOCOL_VERSIONS
            .iter()
            .map(|v| v.to_string())
            .collect();
        let own_leaf_index = group.mls_group().own_leaf_index().u32();
        let self_updated_at = group.self_updated_at.map(|dt| dt.to_rfc3339());
        let pending_proposals = group.mls_group().pending_proposals().count();
        let has_pending_commit = group.mls_group().pending_commit().is_some();
        let required_capabilities = group
            .mls_group()
            .extensions()
            .required_capabilities()
            .map(RequiredDebugCapabilities::from_extension);

        let mut members = HashMap::new();
        for member in group.mls_group().members() {
            let credential = VerifiableClientCredential::try_from(member.credential.clone())?;
            let leaf_node = group
                .mls_group()
                .public_group()
                .leaf(member.index)
                .context("No leaf node for member")?;
            let user_id = credential.user_id();
            let user_profile = core_user.user_profile(user_id).await;
            members.insert(
                member.index.u32(),
                DebugCapabilities::from_capabilities(
                    user_id,
                    user_profile,
                    leaf_node.capabilities(),
                ),
            );
        }

        Ok(Self {
            group_id,
            epoch,
            ciphersuite,
            versions,
            own_leaf_index,
            self_updated_at,
            pending_proposals,
            has_pending_commit,
            required_capabilities,
            members,
        })
    }
}

impl RequiredDebugCapabilities {
    fn from_extension(extension: &RequiredCapabilitiesExtension) -> Self {
        Self {
            extension_types: extension
                .extension_types()
                .iter()
                .map(format_extension_type)
                .collect(),
            proposal_types: extension
                .proposal_types()
                .iter()
                .map(format_proposal_type)
                .collect(),
            credential_types: extension
                .credential_types()
                .iter()
                .map(|t| format!("{t:?}"))
                .collect(),
        }
    }
}

impl DebugCapabilities {
    fn from_capabilities(
        user_id: &UserId,
        user_profile: UserProfile,
        capabilities: &Capabilities,
    ) -> Self {
        Self {
            user_id: format!("{user_id:?}"),
            display_name: user_profile.display_name.to_string(),
            versions: capabilities
                .versions()
                .iter()
                .map(|v| v.to_string())
                .collect(),
            ciphersuites: capabilities
                .ciphersuites()
                .iter()
                .map(|c| {
                    Ciphersuite::try_from(*c)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|_| "unknown".into())
                })
                .collect(),
            extensions: capabilities
                .extensions()
                .iter()
                .map(format_extension_type)
                .collect(),
            proposals: capabilities
                .proposals()
                .iter()
                .map(format_proposal_type)
                .collect(),
        }
    }
}

fn format_extension_type(t: &ExtensionType) -> String {
    match t {
        ExtensionType::Unknown(QS_CLIENT_REFERENCE_EXTENSION_TYPE) => {
            format!("QsClientReference({QS_CLIENT_REFERENCE_EXTENSION_TYPE:#06x})")
        }
        ExtensionType::Unknown(GROUP_DATA_EXTENSION_TYPE) => {
            format!("GroupData({GROUP_DATA_EXTENSION_TYPE:#06x})")
        }
        ExtensionType::Unknown(n) => format!("Unknown({n:#06x})"),
        _ => format!("{t:?}"),
    }
}

fn format_proposal_type(t: &ProposalType) -> String {
    match t {
        ProposalType::Custom(FRIENDSHIP_PACKAGE_PROPOSAL_TYPE) => {
            format!("FriendshipPackage({FRIENDSHIP_PACKAGE_PROPOSAL_TYPE:#06x})")
        }
        ProposalType::Custom(n) => format!("Custom({n:#06x})"),
        _ => format!("{t:?}"),
    }
}
