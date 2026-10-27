// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{LeafCredential, UserCredential, keys::ClientVerifyingKey},
    crypto::aead::keys::{EncryptedUserProfileKey, GroupStateEarKey},
    identifiers::{QsReference, QualifiedGroupId, UserId},
};
use airprotos::{
    client::app_data::GroupAppData,
    common::v1::ClientMetadata,
    convert::{TryFromRef, TryRefInto},
    delivery_service::v1::{
        CreateApqGroupPayload, CreateGroupPayload, GroupSessionData, MlsMessage as MlsMessageProto,
        ProvisionAttachmentResponse, RatchetTree as RatchetTreeProto, StorageObjectType,
        UserCredential as UserCredentialProto,
    },
    validation::{InvalidTlsExt, MissingFieldExt},
};
use apqmls::validation::validate_apq_group_contexts;
use chrono::{TimeDelta, Utc};
use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    group::Group,
    openmls::prelude::{GroupId, MlsMessageBodyIn, MlsMessageIn, RatchetTreeIn},
};
use tls_codec::DeserializeBytes;
use tonic::Status;
use tracing::{error, warn};
use uuid::Uuid;

use crate::{
    auth_service::AsConnector,
    ds::{
        GrpcDs, ReservedGroupId,
        attachments::ProvisionObjectError,
        create_group_stream::{CreateGroupStreamBackend, StreamCreator, StreamReservation},
        group_state::{
            DsGroupState, EncryptedDsGroupState, StorableDsGroupData, leaf_credential_matches_flag,
        },
        grpc::{GroupBootstrapMaterial, WithGroupStateEarKey, WithQualifiedGroupId},
        process::Provider,
    },
    qs::QsConnector,
    rate_limiter::{RateLimiter, RlConfig, RlKey, RlOperation, provider::RlPostgresStorage},
    settings::DsSettings,
};

/// Rejects a new group whose context already carries a tombstone. Only a
/// delete commit sets it.
pub(super) fn ensure_created_without_tombstone(group: &Group) -> Result<(), Status> {
    if GroupAppData::has_tombstone(group.group_info().group_context().extensions()) {
        error!("new group carries a tombstone");
        return Err(Status::invalid_argument("new group carries a tombstone"));
    }
    Ok(())
}

/// A classical group built from an unverified creation payload.
pub(super) struct NewGroup {
    pub(super) qgid: QualifiedGroupId,
    pub(super) ear_key: GroupStateEarKey,
    pub(super) provider: Provider,
    pub(super) group: Group,
}

/// The state of a new classical group, built from its verified creation
/// payload.
pub(super) struct NewGroupState {
    pub(super) group_state: DsGroupState,
    pub(super) creator_client_reference: QsReference,
    pub(super) bootstrap: Option<GroupBootstrapMaterial>,
}

/// The chat data an APQ creation payload carries for both legs.
pub(super) struct NewApqChatData {
    pub(super) encrypted_user_profile_key: EncryptedUserProfileKey,
    pub(super) creator_client_reference: QsReference,
    pub(super) room_state: VerifiedRoomState,
}

impl NewApqChatData {
    pub(super) fn from_payload(payload: &CreateApqGroupPayload) -> Result<Self, Status> {
        let encrypted_user_profile_key = payload
            .encrypted_user_profile_key
            .clone()
            .ok_or_missing_field("encrypted_user_profile_key")?
            .try_into()?;
        let creator_client_reference = payload
            .creator_client_reference
            .clone()
            .ok_or_missing_field("creator_client_reference")?
            .try_into()?;
        let room_state = mimi_room_policy::RoomState::try_from_ref(
            payload
                .room_state
                .as_ref()
                .ok_or_missing_field("room_state")?,
        )
        .map_err(|_| Status::invalid_argument("Invalid room_state message"))?;
        let room_state = VerifiedRoomState::verify(room_state).map_err(|error| {
            error!(%error, "proposed room policy failed verification");
            Status::invalid_argument("Room state verification failed")
        })?;
        Ok(Self {
            encrypted_user_profile_key,
            creator_client_reference,
            room_state,
        })
    }
}

/// One leg of a new group, ready to be stored.
pub(super) struct NewGroupLeg {
    pub(super) group_id: ReservedGroupId,
    pub(super) encrypted_group_state: EncryptedDsGroupState,
    /// Names the leg in errors.
    pub(super) label: &'static str,
}

/// What storing a new group does when its id is taken.
#[derive(Debug, Clone, Copy)]
pub(super) enum GroupInsert {
    /// Reports success without storing anything.
    IgnoreConflict,
    /// Fails with `ALREADY_EXISTS`.
    FailOnConflict,
}

impl<Qep: QsConnector, As: AsConnector> GrpcDs<Qep, As> {
    /// Builds a classical group from its creation payload, before the payload is
    /// verified.
    pub(super) fn new_group(&self, payload: &CreateGroupPayload) -> Result<NewGroup, Status> {
        let qgid = payload.validated_qgid(&self.ds.own_domain)?;
        let ear_key = payload.ear_key()?;

        let (provider, group) = build_group(
            payload.group_info.as_ref(),
            payload.ratchet_tree.as_ref(),
            "group",
        )?;

        Ok(NewGroup {
            qgid,
            ear_key,
            provider,
            group,
        })
    }

    /// Builds the state of a classical group from its verified creation payload.
    pub(super) fn new_group_state(
        provider: Provider,
        group: Group,
        qgid: &QualifiedGroupId,
        payload: CreateGroupPayload,
    ) -> Result<NewGroupState, Status> {
        let encrypted_user_profile_key = payload
            .encrypted_user_profile_key
            .ok_or_missing_field("encrypted_user_profile_key")?
            .try_into()?;
        let creator_client_reference: QsReference = payload
            .creator_client_reference
            .ok_or_missing_field("creator_client_reference")?
            .try_into()?;
        let room_state = mimi_room_policy::RoomState::try_from_ref(
            &payload.room_state.ok_or_missing_field("room_state")?,
        )
        .map_err(|_| Status::invalid_argument("Invalid room_state message"))?;

        let room_state = VerifiedRoomState::verify(room_state).map_err(|e| {
            warn!(%e, "proposed room policy failed verification");
            Status::invalid_argument("Room state verification failed")
        })?;

        let group_state = DsGroupState::new(
            provider,
            group,
            encrypted_user_profile_key,
            creator_client_reference.clone(),
            room_state,
        );

        let bootstrap = payload
            .group_bootstrap
            .map(|group_bootstrap| {
                GroupBootstrapMaterial::for_creation(
                    &group_state,
                    qgid.clone().into(),
                    None,
                    group_bootstrap,
                )
            })
            .transpose()?;

        Ok(NewGroupState {
            group_state,
            creator_client_reference,
            bootstrap,
        })
    }

    /// Builds and checks the PQ leg of an APQ group from its verified creation
    /// payload, with the bootstrap of both legs.
    pub(super) fn new_pq_leg(
        &self,
        payload: CreateApqGroupPayload,
        chat: &NewApqChatData,
        t_qgid: &QualifiedGroupId,
        t_group_state: &DsGroupState,
    ) -> Result<
        (
            QualifiedGroupId,
            DsGroupState,
            Option<GroupBootstrapMaterial>,
        ),
        Status,
    > {
        // Extract pq group state (PQ group uses the same ear_key as the T group)
        let (pq_qgid, pq_group_state, _) = self.extract_group_state(
            payload
                .pq_group_data
                .as_ref()
                .ok_or_missing_field("pq_group_data")?,
            &chat.encrypted_user_profile_key,
            &chat.creator_client_reference,
            &chat.room_state,
        )?;
        // The T leg carries the meaningful creator credential, enforced via
        // `creator_credential`. The PQ leg's leaf holds only an empty placeholder credential, so we
        // cannot check its variant. Instead the two legs must agree on the self-group flag, which
        // transfers the T-leg consistency to the PQ leg.
        if t_group_state.is_self_group() != pq_group_state.is_self_group() {
            return Err(Status::invalid_argument(
                "self-group flag mismatch between t and pq groups",
            ));
        }

        // Check that the t and pq client signature keys match
        Self::verify_signing_key(&t_group_state.group, &pq_group_state.group)?;

        // Both legs must carry the same APQInfo, describing the two groups with a
        // mode that matches their ciphersuites. Commits cannot change it later.
        validate_apq_group_contexts(
            t_group_state.group().group_info().group_context(),
            pq_group_state.group().group_info().group_context(),
        )
        .map_err(|error| {
            warn!(%error, "Invalid APQInfo in new APQ group");
            Status::invalid_argument("Invalid APQInfo")
        })?;

        // Both legs live in the snapshot of the T leg's group id
        let bootstrap = payload
            .group_bootstrap
            .map(|group_bootstrap| {
                GroupBootstrapMaterial::for_creation(
                    t_group_state,
                    t_qgid.clone().into(),
                    Some((pq_qgid.clone().into(), &pq_group_state)),
                    group_bootstrap,
                )
            })
            .transpose()?;

        Ok((pq_qgid, pq_group_state, bootstrap))
    }

    /// Stores the legs of a new group and its bootstrap in one transaction,
    /// then dispatches the bootstrap echo.
    ///
    /// The bootstrap snapshot is stored under `bootstrap_qgid`, the id of the
    /// group's first leg.
    pub(super) async fn store_new_group(
        &self,
        legs: impl IntoIterator<Item = NewGroupLeg>,
        bootstrap: Option<GroupBootstrapMaterial>,
        bootstrap_qgid: &QualifiedGroupId,
        ear_key: &GroupStateEarKey,
        creator_client_reference: QsReference,
        insert: GroupInsert,
    ) -> Result<(), Status> {
        let mut txn = self.ds.db_pool.begin().await.map_err(|error| {
            error!(%error, "failed to start transaction");
            Status::internal("database error")
        })?;
        for NewGroupLeg {
            group_id,
            encrypted_group_state,
            label,
        } in legs
        {
            let stored =
                StorableDsGroupData::store_if_new(txn.as_mut(), group_id, encrypted_group_state)
                    .await
                    .map_err(|error| {
                        error!(%error, "failed to store {label}");
                        Status::internal(format!("failed to store {label}"))
                    })?;
            if !stored && matches!(insert, GroupInsert::FailOnConflict) {
                error!("new group id is already taken");
                return Err(Status::already_exists("group already exists"));
            }
        }
        if let Some(bootstrap) = bootstrap.as_ref() {
            bootstrap
                .store(&mut txn, bootstrap_qgid.group_uuid(), ear_key)
                .await?;
        }
        txn.commit().await.map_err(|error| {
            error!(%error, "failed to commit transaction");
            Status::internal("database error")
        })?;

        if let Some(bootstrap) = bootstrap {
            self.dispatch_group_bootstrap_echo(bootstrap.echo, creator_client_reference)
                .await;
        }
        Ok(())
    }

    /// Consumes one unit of the user's group creation allowance. Fails with
    /// `RESOURCE_EXHAUSTED` when none is left.
    pub(super) async fn consume_creation_allowance(&self, user_id: &UserId) -> Result<(), Status> {
        let rl_key = RlKey::new(
            RlOperation::ReserveGroupId,
            &[b"user_uuid", user_id.uuid().as_bytes()],
        );
        let config = RlConfig {
            max_requests: 100,
            time_window: TimeDelta::hours(1),
        };
        let rl_storage = RlPostgresStorage::new(self.ds.db_pool.clone());
        let rl = RateLimiter::new(config, rl_storage);
        if !rl.allowed(rl_key, Utc::now()).await {
            return Err(Status::resource_exhausted(
                "Too many requests, please try again later",
            ));
        }
        Ok(())
    }

    /// Provisions the upload of a group profile of `size` bytes. Returns `None`
    /// when no storage is configured.
    pub(super) async fn provision_group_profile(
        &self,
        size: i64,
    ) -> Result<Option<ProvisionAttachmentResponse>, Status> {
        let content_length = size
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid group profile size"))?;
        match self
            .ds
            .provision_object(StorageObjectType::GroupProfile, Some(content_length), false)
            .await
        {
            Ok(response) => Ok(Some(response)),
            Err(ProvisionObjectError::NoStorageConfigured) => Ok(None),
            Err(error) => {
                error!(%error, "Failed to provision attachment");
                Err(Status::internal("Failed to provision attachment"))
            }
        }
    }

    pub(super) fn extract_group_state(
        &self,
        data: &GroupSessionData,
        encrypted_user_profile_key: &EncryptedUserProfileKey,
        creator_client_reference: &QsReference,
        room_state: &VerifiedRoomState,
    ) -> Result<(QualifiedGroupId, DsGroupState, GroupStateEarKey), Status> {
        let qgid = data.validated_qgid(self.ds.own_domain())?;
        let ear_key = data.ear_key()?;

        let (provider, group) = build_group(
            data.group_info.as_ref(),
            data.ratchet_tree.as_ref(),
            "t_group",
        )?;

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
            GroupAppData::is_self_group_context(group.group_info().group_context().extensions());
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

/// Builds a group from its GroupInfo and ratchet tree. `what` names the group
/// in errors.
fn build_group(
    group_info: Option<&MlsMessageProto>,
    ratchet_tree: Option<&RatchetTreeProto>,
    what: &str,
) -> Result<(Provider, Group), Status> {
    let group_info: MlsMessageIn = group_info
        .ok_or_missing_field("group_info")?
        .try_ref_into()
        .invalid_tls("group_info")?;
    let MlsMessageBodyIn::GroupInfo(group_info) = group_info.extract() else {
        return Err(Status::invalid_argument("invalid message"));
    };
    let ratchet_tree: RatchetTreeIn = ratchet_tree
        .ok_or_missing_field("ratchet_tree")?
        .try_ref_into()
        .invalid_tls("ratchet_tree")?;
    let provider = Provider::default();
    let group = Group::new(&provider, group_info, ratchet_tree).map_err(|error| {
        error!(%error, "failed to create {what}");
        Status::internal(format!("failed to create {what}"))
    })?;
    ensure_created_without_tombstone(&group)?;
    Ok((provider, group))
}

fn ensure_stream_group_id(
    group_id: &QualifiedGroupId,
    reserved: &QualifiedGroupId,
) -> Result<(), Status> {
    if group_id != reserved {
        return Err(Status::invalid_argument(
            "group id was not reserved by this stream",
        ));
    }
    Ok(())
}

/// Rejects a group whose MLS group id is not the one the stream reserved. The
/// outer group id of the request does not tell, since the group is built from
/// its GroupInfo.
fn ensure_mls_group_id(
    group_state: &DsGroupState,
    reserved: &QualifiedGroupId,
) -> Result<(), Status> {
    let reserved: GroupId = reserved.clone().into();
    if group_state.group().group_info().group_context().group_id() != &reserved {
        return Err(Status::invalid_argument(
            "MLS group id does not match the group id reserved by this stream",
        ));
    }
    Ok(())
}

/// Rejects a creator credential that is not the one the stream authenticated.
fn ensure_stream_creator(
    creator: &StreamCreator,
    credential: &UserCredential,
) -> Result<(), Status> {
    if credential.user_id() != &creator.user_id
        || credential.verifying_key() != &creator.verifying_key
    {
        return Err(Status::permission_denied(
            "creator credential does not belong to the authenticated user",
        ));
    }
    Ok(())
}

impl<Qep: QsConnector, As: AsConnector> CreateGroupStreamBackend for GrpcDs<Qep, As> {
    fn settings(&self) -> &DsSettings {
        &self.ds.settings
    }

    fn verify_client_version(
        &self,
        client_metadata: Option<&ClientMetadata>,
    ) -> Result<(), Status> {
        GrpcDs::verify_client_version(self, client_metadata)?;
        Ok(())
    }

    async fn user_verifying_key(
        &self,
        user_id: &UserId,
    ) -> Result<Option<ClientVerifyingKey>, Status> {
        self.load_client_verifying_key(user_id).await
    }

    async fn consume_creation_allowance(&self, user_id: &UserId) -> Result<(), Status> {
        GrpcDs::consume_creation_allowance(self, user_id).await
    }

    fn fresh_group_id(&self) -> QualifiedGroupId {
        QualifiedGroupId::new(Uuid::new_v4(), self.ds.own_domain.clone())
    }

    async fn provision_group_profile(
        &self,
        size: i64,
    ) -> Result<Option<ProvisionAttachmentResponse>, Status> {
        GrpcDs::provision_group_profile(self, size).await
    }

    /// Creates an APQ group, a self-group included, from the create step of a
    /// stream.
    async fn create_apq_group(
        &self,
        creator: &StreamCreator,
        reservation: &StreamReservation,
        payload: CreateApqGroupPayload,
    ) -> Result<(), Status> {
        let t_group_data = payload
            .t_group_data
            .as_ref()
            .ok_or_missing_field("t_group_data")?;
        let pq_group_data = payload
            .pq_group_data
            .as_ref()
            .ok_or_missing_field("pq_group_data")?;
        ensure_stream_group_id(
            &t_group_data.validated_qgid(&self.ds.own_domain)?,
            &reservation.t_group_id,
        )?;
        ensure_stream_group_id(
            &pq_group_data.validated_qgid(&self.ds.own_domain)?,
            &reservation.pq_group_id,
        )?;

        let chat = NewApqChatData::from_payload(&payload)?;
        let (t_qgid, t_group_state, ear_key) = self.extract_group_state(
            t_group_data,
            &chat.encrypted_user_profile_key,
            &chat.creator_client_reference,
            &chat.room_state,
        )?;
        ensure_mls_group_id(&t_group_state, &reservation.t_group_id)?;
        // For a self-group, this is the credential sent alongside the leaves
        let t_user_credential = Self::creator_credential(
            &t_group_state.group,
            payload.creator_user_credential.as_ref(),
        )?;
        ensure_stream_creator(creator, &t_user_credential)?;

        let (pq_qgid, pq_group_state, bootstrap) =
            self.new_pq_leg(payload, &chat, &t_qgid, &t_group_state)?;
        ensure_mls_group_id(&pq_group_state, &reservation.pq_group_id)?;
        let encrypted_t_group_state = t_group_state.encrypt(&ear_key)?;
        let encrypted_pq_group_state = pq_group_state.encrypt(&ear_key)?;

        self.store_new_group(
            [
                NewGroupLeg {
                    group_id: ReservedGroupId(t_qgid.group_uuid()),
                    encrypted_group_state: encrypted_t_group_state,
                    label: "t group state",
                },
                NewGroupLeg {
                    group_id: ReservedGroupId(pq_qgid.group_uuid()),
                    encrypted_group_state: encrypted_pq_group_state,
                    label: "pq group state",
                },
            ],
            bootstrap,
            &t_qgid,
            &ear_key,
            chat.creator_client_reference,
            GroupInsert::FailOnConflict,
        )
        .await
    }
}
