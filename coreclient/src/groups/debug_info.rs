// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Debug info for groups.

use std::collections::HashMap;

use aircommon::{
    credentials::VerifiableClientCredential,
    identifiers::{QualifiedGroupId, UserId},
    mls_group_config::{
        AIR_COMPONENT_ID, FRIENDSHIP_PACKAGE_PROPOSAL_TYPE, GROUP_DATA_EXTENSION_TYPE,
        QS_CLIENT_REFERENCE_EXTENSION_TYPE, SUPPORTED_PROTOCOL_VERSIONS,
    },
};
use airprotos::client::group::{EncryptedGroupTitle, ExternalGroupProfile, GroupData};
use anyhow::Context as _;
use hex::ToHex as _;
use mls_assist::components::ComponentsList;
use openmls::{
    component::ComponentType,
    extensions::AppDataDictionary,
    prelude::{Ciphersuite, ExtensionType, ProposalType, RequiredCapabilitiesExtension},
};
use tls_codec::DeserializeBytes as _;

use crate::{
    ChatId, UserProfile,
    chats::GroupDataExt,
    clients::CoreUser,
    groups::{Group, GroupDataBytes},
};

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
    pub group_data: Option<GroupDataDebugInfo>,
}

#[derive(Debug, Clone)]
pub struct GroupDataDebugInfo {
    pub title: String,
    pub has_picture: bool,
    pub encrypted_title: Option<EncryptedGroupTitleDebugInfo>,
    pub external_group_profile: Option<ExternalGroupProfileDebugInfo>,
}

#[derive(Debug, Clone)]
pub struct EncryptedGroupTitleDebugInfo {
    pub ciphertext: String,
    pub nonce: String,
    pub aad: String,
}

#[derive(Debug, Clone)]
pub struct ExternalGroupProfileDebugInfo {
    pub object_id: String,
    pub size: u64,
    pub enc_alg: Option<String>,
    pub aad: String,
    pub nonce: String,
    pub hash_alg: String,
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct RequiredDebugCapabilities {
    pub extension_types: Vec<String>,
    pub proposal_types: Vec<String>,
    pub credential_types: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AppDataDebugInfo {
    pub air_components: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DebugCapabilities {
    pub user_id: String,
    pub display_name: String,
    pub versions: Vec<String>,
    pub ciphersuites: Vec<String>,
    pub extensions: Vec<String>,
    pub proposals: Vec<String>,
    pub app_data: Option<AppDataDebugInfo>,
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
        let group_data = group
            .mls_group()
            .extensions()
            .unknown(GROUP_DATA_EXTENSION_TYPE)
            .and_then(|ext| GroupData::decode(&GroupDataBytes::from(ext.0.clone())).ok())
            .map(|gd| GroupDataDebugInfo {
                title: gd.title,
                has_picture: gd.picture.is_some(),
                encrypted_title: gd.encrypted_title.map(EncryptedGroupTitleDebugInfo::from),
                external_group_profile: gd
                    .external_group_profile
                    .map(ExternalGroupProfileDebugInfo::from),
            });

        let mut members = HashMap::new();
        for member in group.mls_group().members() {
            let credential = VerifiableClientCredential::from_basic_credential(&member.credential)?;
            let leaf_node = group
                .mls_group()
                .public_group()
                .leaf(member.index)
                .context("No leaf node for member")?;
            let user_id = credential.user_id();
            let user_profile = core_user.user_profile(user_id).await;
            members.insert(
                member.index.u32(),
                DebugCapabilities::from_leaf_node(user_id, user_profile, leaf_node),
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
            group_data,
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
    fn from_leaf_node(
        user_id: &UserId,
        user_profile: UserProfile,
        leaf_node: &openmls::prelude::LeafNode,
    ) -> Self {
        let capabilities = leaf_node.capabilities();
        let app_data = leaf_node
            .extensions()
            .app_data_dictionary()
            .map(|ext| AppDataDebugInfo::from_app_data_dictionary(ext.dictionary()));
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
            app_data,
        }
    }
}

impl AppDataDebugInfo {
    fn from_app_data_dictionary(dict: &AppDataDictionary) -> Self {
        let air_components = dict
            .get(&ComponentType::AppComponents.into())
            .and_then(|data| ComponentsList::tls_deserialize_exact_bytes(data).ok())
            .map(|list| {
                list.component_ids
                    .iter()
                    .map(|id| {
                        if *id == AIR_COMPONENT_ID {
                            format!("Air({id:#06x})")
                        } else {
                            format!("{id:#06x}")
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { air_components }
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

impl From<EncryptedGroupTitle> for EncryptedGroupTitleDebugInfo {
    fn from(
        EncryptedGroupTitle {
            ciphertext,
            nonce,
            aad,
        }: EncryptedGroupTitle,
    ) -> Self {
        Self {
            ciphertext: ciphertext.encode_hex(),
            nonce: nonce.encode_hex(),
            aad: aad.encode_hex(),
        }
    }
}

impl From<ExternalGroupProfile> for ExternalGroupProfileDebugInfo {
    fn from(
        ExternalGroupProfile {
            object_id,
            size,
            enc_alg,
            nonce,
            aad,
            hash_alg,
            content_hash,
        }: ExternalGroupProfile,
    ) -> Self {
        Self {
            object_id: object_id.to_string(),
            size,
            enc_alg: enc_alg.map(|a| format!("{a:?}")),
            nonce: nonce.encode_hex(),
            aad: aad.encode_hex(),
            hash_alg: format!("{hash_alg:?}"),
            content_hash: content_hash.encode_hex(),
        }
    }
}
