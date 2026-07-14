// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::ClientCredential,
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
    auth_service::AsConnector,
    ds::{
        GrpcDs,
        group_state::DsGroupState,
        grpc::{WithGroupStateEarKey, WithQualifiedGroupId},
        process::Provider,
    },
    qs::QsConnector,
};

impl<Qep: QsConnector, As: AsConnector> GrpcDs<Qep, As> {
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

    /// Ensure the T and PQ groups' single leaves share the same signature key.
    ///
    /// We compare the two leaf keys directly rather than against the client
    /// credential's key: for the virtual-client self-group the leaves are signed
    /// with a freshly minted key that intentionally differs from the credential
    /// key.
    pub(super) fn verify_signing_key(t_group: &Group, pq_group: &Group) -> Result<(), Status> {
        for (t_member, p_member) in t_group.members().zip(pq_group.members()) {
            if t_member.signature_key != p_member.signature_key {
                return Err(Status::invalid_argument(
                    "t and pq client signature keys do not match",
                ));
            }
        }
        Ok(())
    }
}
