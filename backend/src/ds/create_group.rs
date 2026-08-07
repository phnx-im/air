// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{LeafCredential, UserCredential},
    crypto::aead::keys::{EncryptedUserProfileKey, GroupStateEarKey},
    identifiers::{QsReference, QualifiedGroupId},
};
use airprotos::{
    client::component::AirComponent,
    convert::TryRefInto,
    delivery_service::v1::{GroupSessionData, UserCredential as UserCredentialProto},
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
        group_state::{DsGroupState, leaf_credential_matches_flag},
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

    /// Return the credential that authenticates the creation of `group`.
    ///
    /// The creator's leaf credential must match the group kind: a self-group leaf carries a
    /// self-group credential, a regular-group leaf a user credential. A self-group's leaves carry
    /// no user credential, so the creator's user credential is provided out of band. A regular
    /// group takes it from the creator's leaf and must not provide one.
    pub(super) fn creator_credential(
        group: &Group,
        provided: Option<&UserCredentialProto>,
    ) -> Result<UserCredential, Status> {
        let mut members = group.members().fuse();
        let (Some(member), None) = (members.next(), members.next()) else {
            error!("group must have exactly one member");
            return Err(Status::invalid_argument(
                "group must have exactly one member",
            ));
        };

        let leaf_credential = LeafCredential::from_credential(&member.credential)
            .map_err(|_| Status::invalid_argument("invalid credential"))?;
        let is_self_group =
            AirComponent::is_self_group_context(group.group_info().group_context().extensions());
        if !leaf_credential_matches_flag(&leaf_credential, is_self_group) {
            return Err(Status::invalid_argument(
                "creator leaf credential does not match group kind",
            ));
        }

        match (provided, is_self_group) {
            (Some(credential), true) => {
                let credential = credential
                    .try_ref_into()
                    .invalid_tls("creator_user_credential")?;
                Ok(credential)
            }
            (None, false) => {
                UserCredential::tls_deserialize_exact_bytes(member.credential.serialized_content())
                    .map_err(|_| Status::invalid_argument("invalid credential"))
            }
            (Some(_), false) => Err(Status::invalid_argument(
                "creator user credential requires a self-group",
            )),
            (None, true) => Err(Status::invalid_argument(
                "self-group requires a creator user credential",
            )),
        }
    }

    /// Return the signature key of a group that is expected to have exactly one
    /// member (i.e. a freshly created group).
    fn sole_member_signature_key(group: &Group) -> Result<Vec<u8>, Status> {
        let mut members = group.members().fuse();
        match (members.next(), members.next()) {
            (Some(member), None) => Ok(member.signature_key),
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
        let t_member_signature_key = Self::sole_member_signature_key(t_group)?;
        let pq_member_signature_key = Self::sole_member_signature_key(pq_group)?;
        if t_member_signature_key != pq_member_signature_key {
            Err(Status::invalid_argument(
                "t and pq client signature keys do not match",
            ))
        } else {
            Ok(())
        }
    }
}
