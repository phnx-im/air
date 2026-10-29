// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{
        RoomPolicyIdentity,
        keys::{LeafSigningKey, UserSigningKey},
    },
    crypto::aead::keys::{EncryptedUserProfileKey, GroupStateEarKey, IdentityLinkWrapperKey},
    identifiers::{QsReference, UserId},
    messages::client_ds_out::{CreateGroupParamsOut, CreatePqGroupParamsOut},
    mls_group_config::{
        APQ_CIPHERSUITE, GROUP_DATA_EXTENSION_TYPE, MAX_PAST_EPOCHS,
        default_group_required_extensions, default_leaf_node_capabilities,
        default_sender_ratchet_configuration, self_group_leaf_node_capabilities,
    },
};
use airprotos::client::{
    app_data::{ClientAppData, GroupAppData},
    group::GroupData,
};
use anyhow::{Result, anyhow};
use apqmls::{ApqMlsGroup, authentication::ApqCredentialWithKey};
use mimi_room_policy::{RoomPolicy, VerifiedRoomState};
use openmls::{
    components::vc_derivation_info::VC_COMPONENT_ID,
    group::{GroupContext, VcDerivationEpochRetentionPolicy},
    prelude::{
        Credential, CredentialType, CredentialWithKey, Extension, Extensions, GroupId, MlsGroup,
        MlsMessageOut, OpenMlsProvider, PURE_PLAINTEXT_WIRE_FORMAT_POLICY, UnknownExtension,
    },
    treesync::RatchetTree,
};

use crate::{
    chats::GroupDataExt,
    db::access::WriteConnection,
    groups::{
        Group, commit::update::client_reference_extension, openmls_provider::AirOpenMlsProvider,
    },
};

pub(crate) struct PartialCreateGroupParams {
    pub(crate) group_id: GroupId,
    ratchet_tree: RatchetTree,
    group_info: MlsMessageOut,
    pub(crate) room_state: VerifiedRoomState,
    pq: Option<PartialPqCreateGroupParams>,
}

pub(crate) struct PartialPqCreateGroupParams {
    group_id: GroupId,
    ratchet_tree: RatchetTree,
    group_info: MlsMessageOut,
}

impl PartialCreateGroupParams {
    pub(crate) fn into_params(
        self,
        creator_client_reference: QsReference,
        encrypted_user_profile_key: EncryptedUserProfileKey,
    ) -> CreateGroupParamsOut {
        let pq = self.pq.map(|pq| CreatePqGroupParamsOut {
            group_id: pq.group_id,
            ratchet_tree: pq.ratchet_tree,
            group_info: pq.group_info,
        });
        CreateGroupParamsOut {
            group_id: self.group_id,
            ratchet_tree: self.ratchet_tree,
            encrypted_user_profile_key,
            creator_client_reference,
            group_info: self.group_info,
            room_state: self.room_state,
            pq,
            creator_user_credential: None,
            group_bootstrap: None,
        }
    }
}

/// Group context contents of a new group
pub(crate) enum NewGroupContext {
    /// Profile in the legacy group data extension
    LegacyChat(GroupData),
    /// Profile in the group profile component
    Chat(GroupData),
    /// Self group: profile in the group profile component, SafeAAD for the VC component
    SelfGroup(GroupData),
}

impl NewGroupContext {
    fn app_data(&self) -> GroupAppData {
        let is_self_group = matches!(self, Self::SelfGroup(_));
        GroupAppData {
            is_self_group,
            safe_aad_components: is_self_group.then(|| vec![VC_COMPONENT_ID]),
            profile: None,
            tombstone: None,
        }
    }

    fn add_to_t_extensions(self, extensions: &mut Extensions<GroupContext>) -> anyhow::Result<()> {
        let mut app_data = self.app_data();
        let group_data = match self {
            Self::Chat(data) | Self::SelfGroup(data) => {
                app_data.profile = Some(data.into_component());
                None
            }
            Self::LegacyChat(data) => Some(data.encode()?),
        };
        extensions.add(app_data.to_extension()?)?;
        if let Some(group_data) = group_data {
            extensions.add(Extension::Unknown(
                GROUP_DATA_EXTENSION_TYPE,
                UnknownExtension(group_data),
            ))?;
        }
        Ok(())
    }

    fn add_to_pq_extensions(
        &self,
        extensions: &mut Extensions<GroupContext>,
    ) -> anyhow::Result<()> {
        extensions.add(self.app_data().to_extension()?)?;
        Ok(())
    }
}

impl Group {
    /// Create a group.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_group(
        mut connection: impl WriteConnection,
        signer: &UserSigningKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        group_id: GroupId,
        context: NewGroupContext,
        vc_group_id: Option<&GroupId>,
    ) -> Result<(Self, PartialCreateGroupParams)> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let group_state_ear_key = GroupStateEarKey::random()?;

        let required_capabilities =
            Extension::RequiredCapabilities(default_group_required_extensions());

        let mut gc_extensions = Extensions::empty();
        gc_extensions.add(required_capabilities)?;
        context.add_to_t_extensions(&mut gc_extensions)?;

        let credential_with_key = CredentialWithKey {
            credential: signer.credential().try_into()?,
            signature_key: signer.credential().verifying_key().clone().into(),
        };

        let leaf_node_extensions = if vc_group_id.is_some() {
            ClientAppData::current_virtual_client().leaf_node_extensions()
        } else {
            ClientAppData::current().leaf_node_extensions()
        };
        let mut builder = MlsGroup::builder()
            .with_group_id(group_id.clone())
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(gc_extensions)
            .with_leaf_node_extensions(leaf_node_extensions)?
            .sender_ratchet_configuration(default_sender_ratchet_configuration())
            .max_past_epochs(MAX_PAST_EPOCHS)
            // Not an emulation group, but keeps the stored configs uniform.
            .set_vc_derivation_epoch_retention_policy(VcDerivationEpochRetentionPolicy::KeepAll)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY);
        if let Some(vc_group_id) = vc_group_id {
            builder = builder.vc_emulation(vc_group_id);
        }
        let mls_group = builder
            .build(&provider, signer, credential_with_key)
            .map_err(|e| anyhow!("Error while creating group: {:?}", e))?;

        let creator_identity = RoomPolicyIdentity::User(signer.credential().user_id().clone());
        let room_state = VerifiedRoomState::new(
            creator_identity.to_bytes()?,
            RoomPolicy::default_trusted_private(),
        )?;

        let params = PartialCreateGroupParams {
            group_id: group_id.clone(),
            ratchet_tree: mls_group.export_ratchet_tree(),
            group_info: mls_group.export_group_info(provider.crypto(), signer, true)?,
            room_state: room_state.clone(),
            pq: None,
        };

        let group = Self::new(
            identity_link_wrapper_key,
            group_state_ear_key,
            mls_group,
            None,
            room_state,
            signer.credential().user_id().clone(),
        );

        Ok((group, params))
    }
}

impl Group {
    /// `vc_group_id` names the emulation group when the creator acts as a
    /// virtual client. Both halves then derive their creator leaf and epoch-0
    /// secret from that group's newest derivation epoch. It is mutually
    /// exclusive with creating the group as an emulation group itself, which is
    /// what the self group does.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_apq_group(
        mut connection: impl WriteConnection,
        signer: &LeafSigningKey,
        own_user_id: UserId,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        t_group_id: GroupId,
        pq_group_id: GroupId,
        context: NewGroupContext,
        vc_group_id: Option<&GroupId>,
        creator_client_reference: Option<&QsReference>,
    ) -> anyhow::Result<(Self, PartialCreateGroupParams)> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());

        let group_state_ear_key = GroupStateEarKey::random()?;

        let required_capabilities =
            Extension::RequiredCapabilities(default_group_required_extensions());

        // APQ groups automatically add an app data dictionary extension (to required
        // capabilities), so we can safely add it here for all APQ groups.
        let mut pq_gc_extensions = Extensions::empty();
        pq_gc_extensions.add(required_capabilities.clone())?;
        context.add_to_pq_extensions(&mut pq_gc_extensions)?;

        let mut t_gc_extensions = Extensions::empty();
        t_gc_extensions.add(required_capabilities)?;
        context.add_to_t_extensions(&mut t_gc_extensions)?;

        // The leaf signature key is the signer's own key.
        let t_credential = CredentialWithKey {
            credential: signer.mls_credential()?,
            signature_key: signer.verifying_key().clone().into(),
        };
        // Skip storing the same credential twice
        let pq_credential = CredentialWithKey {
            credential: Credential::new(CredentialType::Basic, Vec::new()),
            signature_key: signer.verifying_key().clone().into(),
        };
        let apq_credential_with_key = ApqCredentialWithKey {
            t_credential,
            pq_credential,
        };

        let capabilities = match signer {
            LeafSigningKey::User(_) => default_leaf_node_capabilities(),
            LeafSigningKey::SelfGroup(_) => self_group_leaf_node_capabilities(),
        };

        let mut leaf_node_extensions = ClientAppData::current().leaf_node_extensions();
        if let Some(client_reference) = creator_client_reference {
            leaf_node_extensions.add(client_reference_extension(client_reference)?)?;
        }

        let mut builder = ApqMlsGroup::builder()
            .with_group_ids(t_group_id, pq_group_id)
            .with_ciphersuite(APQ_CIPHERSUITE)
            .with_capabilities(capabilities)
            .with_leaf_node_extensions(leaf_node_extensions.clone(), leaf_node_extensions)?
            .with_group_context_extensions(t_gc_extensions, pq_gc_extensions)?
            .sender_ratchet_configuration(default_sender_ratchet_configuration())
            .max_past_epochs(MAX_PAST_EPOCHS)
            // Air prunes derivation epochs on a wall-clock window instead, see
            // `VC_DERIVATION_EPOCH_RETENTION_WINDOW`.
            .vc_derivation_epoch_retention_policy(VcDerivationEpochRetentionPolicy::KeepAll)
            // The self group is the emulation group of the virtual client, so its
            // initial epoch already has to be a derivation epoch.
            .emulation_group(matches!(signer, LeafSigningKey::SelfGroup(_)))
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY);
        if let Some(vc_group_id) = vc_group_id {
            // Both leaves carry the virtual-client marker, as they do on the
            // external-commit join path.
            let leaf_extensions = ClientAppData::current_virtual_client().leaf_node_extensions();
            builder = builder
                .with_leaf_node_extensions(leaf_extensions.clone(), leaf_extensions)?
                .vc_emulation(vc_group_id);
        }
        let (t_group, pq_group) = builder
            .build(&provider, signer, apq_credential_with_key)?
            .into_groups();

        let room_state = VerifiedRoomState::new(
            signer.room_policy_identity().to_bytes()?,
            RoomPolicy::default_trusted_private(),
        )?;

        let params = PartialCreateGroupParams {
            group_id: t_group.group_id().clone(),
            ratchet_tree: t_group.export_ratchet_tree(),
            group_info: t_group.export_group_info(provider.crypto(), signer, true)?,
            room_state: room_state.clone(),
            pq: Some(PartialPqCreateGroupParams {
                group_id: pq_group.group_id().clone(),
                ratchet_tree: pq_group.export_ratchet_tree(),
                group_info: pq_group.export_group_info(provider.crypto(), signer, true)?,
            }),
        };

        let group = Self::new(
            identity_link_wrapper_key,
            group_state_ear_key,
            t_group,
            Some(pq_group),
            room_state,
            own_user_id,
        );

        Ok((group, params))
    }
}
