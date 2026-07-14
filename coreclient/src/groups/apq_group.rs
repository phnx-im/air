// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    mls_group_config::{
        APQ_CIPHERSUITE, GROUP_DATA_EXTENSION_TYPE, MAX_PAST_EPOCHS,
        default_group_context_app_data_dictionary_extension, default_group_required_extensions,
        default_leaf_node_capabilities, default_sender_ratchet_configuration,
    },
    time::TimeStamp,
};
use airprotos::client::component::AirComponent;
use anyhow::Context;
use apqmls::{ApqMlsGroup, authentication::ApqCredentialWithKey};
use mimi_room_policy::{RoomPolicy, VerifiedRoomState};
use openmls::{
    component::ComponentId,
    components::vc_derivation_info::EpochId,
    group::{GroupId, MlsGroup, PURE_PLAINTEXT_WIRE_FORMAT_POLICY},
    prelude::{
        Credential, CredentialType, CredentialWithKey, Extension, Extensions, LeafNode,
        UnknownExtension,
    },
};
use openmls_traits::OpenMlsProvider;
use tls_codec::Serialize;

use crate::{
    db::access::WriteConnection,
    groups::{
        GroupDataBytes, PartialCreateGroupParams, PartialPqCreateGroupParams,
        openmls_provider::AirOpenMlsProvider,
    },
};

use super::Group;

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

impl Group {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create_apq_group(
        mut connection: impl WriteConnection,
        signer: &ClientSigningKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        t_group_id: GroupId,
        pq_group_id: GroupId,
        group_data_bytes: GroupDataBytes,
        safe_aad_components: Option<Vec<ComponentId>>,
        air_component: AirComponent,
        leaf_node_extensions: Option<Extensions<LeafNode>>,
    ) -> anyhow::Result<(Self, PartialCreateGroupParams)> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());

        let group_state_ear_key = GroupStateEarKey::random()?;

        let required_capabilities =
            Extension::RequiredCapabilities(default_group_required_extensions());

        let group_data_extension = Extension::Unknown(
            GROUP_DATA_EXTENSION_TYPE,
            UnknownExtension(group_data_bytes.bytes),
        );
        let gc_extensions = Extensions::from_vec(vec![
            group_data_extension,
            required_capabilities,
            // APQ groups automatically add an app data dictionary extension (to required
            // capabilities), so we can safely add it here for all APQ groups.
            default_group_context_app_data_dictionary_extension(air_component, safe_aad_components),
        ])?;

        // The leaf signature key must be the signer's *own* verifying key, not
        // the credential's. They coincide for regular groups, but for the
        // self-group the signer is a freshly minted key paired with a foreign
        // credential.
        let t_credential = CredentialWithKey {
            credential: signer.credential().try_into()?,
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

        let mut group_builder = ApqMlsGroup::builder()
            .with_group_ids(t_group_id, pq_group_id)
            .with_ciphersuite(APQ_CIPHERSUITE)
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(gc_extensions.clone(), gc_extensions)?
            .sender_ratchet_configuration(default_sender_ratchet_configuration())
            .max_past_epochs(MAX_PAST_EPOCHS)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY);

        if let Some(leaf_node_extensions) = leaf_node_extensions {
            group_builder = group_builder
                .with_leaf_node_extensions(leaf_node_extensions.clone(), leaf_node_extensions)?;
        }

        let (t_group, pq_group) = group_builder
            .build(&provider, signer, apq_credential_with_key)?
            .into_groups();

        let user_id = signer.credential().user_id();
        let room_state = VerifiedRoomState::new(
            user_id.tls_serialize_detached()?,
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

        let now = TimeStamp::now();
        let group = Self {
            identity_link_wrapper_key,
            mls_group: t_group,
            room_state,
            group_state_ear_key,
            pending_diff: None,
            self_updated_at: Some(now),
            pq: Some(PqGroup {
                mls_group: pq_group,
                self_updated_at: Some(now),
            }),
            pending_commit_failed: false,
            send_message_collision_key: None,
        };

        Ok((group, params))
    }

    /// Register a virtual-clients emulation epoch on both the classical and
    /// post-quantum groups.
    ///
    /// TODO(gabriel): since this method can only be called on the self-group
    /// we should most likely introduce a new type for it.
    pub(crate) fn register_vc_emulation_epoch(
        &mut self,
        mut connection: impl WriteConnection,
    ) -> anyhow::Result<EpochId> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let (t_group, _) = self.apq_mls_groups_mut()?;
        let t_epoch_id = t_group
            .register_vc_emulation_epoch(provider.crypto(), provider.storage())
            .context("register VC emulation epoch (t)")?;
        Ok(t_epoch_id)
    }
}
