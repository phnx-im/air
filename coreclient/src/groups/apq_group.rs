// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::aead::keys::{GroupStateEarKey, IdentityLinkWrapperKey},
    mls_group_config::{
        APQ_CIPHERSUITE, GROUP_DATA_EXTENSION_TYPE, MAX_PAST_EPOCHS,
        default_group_required_extensions, default_leaf_node_capabilities,
        default_sender_ratchet_configuration,
    },
    time::TimeStamp,
};
use apqmls::{ApqMlsGroup, authentication::ApqCredentialWithKey};
use mimi_room_policy::{RoomPolicy, VerifiedRoomState};
use openmls::{
    group::{GroupId, MlsGroup, PURE_PLAINTEXT_WIRE_FORMAT_POLICY},
    prelude::{CredentialWithKey, Extension, Extensions, UnknownExtension},
};
use openmls_traits::OpenMlsProvider;
use sqlx::SqliteConnection;
use tls_codec::Serialize;

use crate::groups::{
    GroupDataBytes, PartialCreateGroupParams, PartialPqCreateGroupParams,
    openmls_provider::AirOpenMlsProvider,
};

use super::Group;

/// Part of the [`Group`] that is only used for the post-quantum group.
#[derive(Debug)]
pub(crate) struct PqGroup {
    pub(crate) mls_group: MlsGroup,
    pub(crate) group_state_ear_key: GroupStateEarKey,
    pub(crate) self_updated_at: Option<TimeStamp>,
}

impl PqGroup {
    pub(crate) fn group_id(&self) -> &GroupId {
        self.mls_group.group_id()
    }
}

impl Group {
    pub(crate) fn create_apq_group(
        connection: &mut SqliteConnection,
        signer: &ClientSigningKey,
        identity_link_wrapper_key: IdentityLinkWrapperKey,
        t_group_id: GroupId,
        pq_group_id: GroupId,
        group_data_bytes: GroupDataBytes,
    ) -> anyhow::Result<(Self, PartialCreateGroupParams)> {
        let provider = AirOpenMlsProvider::new(connection);

        let t_group_state_ear_key = GroupStateEarKey::random()?;
        let pq_group_state_ear_key = GroupStateEarKey::random()?;

        let required_capabilities =
            Extension::RequiredCapabilities(default_group_required_extensions());

        let group_data_extension = Extension::Unknown(
            GROUP_DATA_EXTENSION_TYPE,
            UnknownExtension(group_data_bytes.bytes),
        );
        let gc_extensions =
            Extensions::from_vec(vec![group_data_extension, required_capabilities])?;

        let credential_with_key = CredentialWithKey {
            credential: signer.credential().try_into()?,
            signature_key: signer.credential().verifying_key().clone().into(),
        };
        let apq_credential_with_key = ApqCredentialWithKey {
            t_credential: credential_with_key.clone(),
            pq_credential: credential_with_key,
        };

        let (t_group, pq_group) = ApqMlsGroup::builder()
            .with_group_ids(t_group_id, pq_group_id)
            .with_ciphersuite(APQ_CIPHERSUITE)
            .with_capabilities(default_leaf_node_capabilities())
            .with_group_context_extensions(gc_extensions.clone(), gc_extensions)?
            .sender_ratchet_configuration(default_sender_ratchet_configuration())
            .max_past_epochs(MAX_PAST_EPOCHS)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
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
            group_state_ear_key: t_group_state_ear_key,
            pending_diff: None,
            self_updated_at: Some(now),
            pq: Some(PqGroup {
                mls_group: pq_group,
                group_state_ear_key: pq_group_state_ear_key,
                self_updated_at: Some(now),
            }),
        };

        Ok((group, params))
    }
}
