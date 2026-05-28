// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{ClientCredential, keys::ClientVerifyingKey},
    crypto::aead::keys::{EncryptedUserProfileKey, GroupStateEarKey},
    identifiers::{QsReference, QualifiedGroupId},
};
use airprotos::{
    convert::TryRefInto,
    delivery_service::v1::GroupSessionData,
    validation::{InvalidTlsExt, MissingFieldExt},
};
use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    group::Group,
    openmls::prelude::{MlsMessageBodyIn, MlsMessageIn, RatchetTreeIn},
};
use tls_codec::DeserializeBytes;
use tonic::Status;
use tracing::error;

use crate::{
    ds::{
        GrpcDs,
        group_state::DsGroupState,
        grpc::{WithGroupStateEarKey, WithQualifiedGroupId},
        process::Provider,
    },
    qs::QsConnector,
};

impl<Qep: QsConnector> GrpcDs<Qep> {
    pub(super) fn extract_group_state(
        &self,
        data: GroupSessionData,
        encrypted_user_profile_key: &EncryptedUserProfileKey,
        creator_client_reference: &QsReference,
        room_state: &VerifiedRoomState,
    ) -> Result<(QualifiedGroupId, DsGroupState, GroupStateEarKey), Status> {
        let qgid = data.validated_qgid(self.ds.own_domain())?;
        let ear_key = data.ear_key()?;

        let GroupSessionData {
            qgid: _,
            group_state_ear_key: _,
            ratchet_tree,
            group_info,
        } = data;

        let group_info: MlsMessageIn = group_info
            .as_ref()
            .ok_or_missing_field("group_info")?
            .try_ref_into()
            .invalid_tls("group_info")?;
        let MlsMessageBodyIn::GroupInfo(group_info) = group_info.extract() else {
            return Err(Status::invalid_argument("invalid message"));
        };
        let ratchet_tree: RatchetTreeIn = ratchet_tree
            .as_ref()
            .ok_or_missing_field("ratchet_tree")?
            .try_ref_into()
            .invalid_tls("ratchet_tree")?;
        let provider = Provider::default();
        let group = Group::new(&provider, group_info.clone(), ratchet_tree).map_err(|error| {
            error!(%error, "failed to create t_group");
            Status::internal("failed to create t_group")
        })?;

        let state = DsGroupState::new(
            provider,
            group,
            encrypted_user_profile_key.clone(),
            creator_client_reference.clone(),
            room_state.clone(),
        );

        Ok((qgid, state, ear_key))
    }

    pub(super) fn extract_credential(group: &Group) -> Result<ClientCredential, Status> {
        let mut members = group.members().fuse();
        match (members.next(), members.next()) {
            (Some(member), None) => ClientCredential::tls_deserialize_exact_bytes(
                member.credential.serialized_content(),
            )
            .map_err(|_| Status::invalid_argument("invalid credential")),
            _ => {
                error!("group must have exactly one member");
                Err(Status::invalid_argument(
                    "group must have exactly one member",
                ))
            }
        }
    }

    pub(super) fn verify_signing_key(
        group: &Group,
        verifying_key: &ClientVerifyingKey,
    ) -> Result<(), Status> {
        let mut members = group.members().fuse();
        match (members.next(), members.next()) {
            (Some(member), None) => {
                if member.signature_key != verifying_key.as_slice() {
                    Err(Status::invalid_argument(
                        "t and pq client signature keys do not match",
                    ))
                } else {
                    Ok(())
                }
            }
            _ => {
                error!("group must have exactly one member");
                Err(Status::invalid_argument(
                    "group must have exactly one member",
                ))
            }
        }
    }
}
