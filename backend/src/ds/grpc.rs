// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{ClientCredential, keys::ClientVerifyingKey},
    crypto::{
        aead::keys::GroupStateEarKey,
        signatures::{
            keys::LeafVerifyingKeyRef,
            private_keys::SignatureVerificationError,
            signable::{Verifiable, VerifiedStruct},
        },
    },
    identifiers::{self, Fqdn, QualifiedGroupId},
    messages::client_ds::{
        self, GroupOperationParams, JoinConnectionGroupParams, QsQueueMessagePayload,
        UserProfileKeyUpdateParams, WelcomeInfoParams,
    },
    mls_group_config::MAX_PAST_EPOCHS,
    time::TimeStamp,
};
use airprotos::{
    common::v1::ClientMetadata,
    convert::{RefInto, TryFromRef as _, TryRefInto},
    delivery_service::v1::{
        self, delivery_service_server::DeliveryService,
        targeted_message_payload::TargetedMessageType, *,
    },
    signed::{SignedRequest, VerifiableRequest},
    validation::{InvalidTlsExt, MissingFieldExt},
};
use chrono::TimeDelta;
use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    group::Group,
    messages::{AssistedMessageIn, SerializedMlsMessage},
    openmls::prelude::{LeafNodeIndex, MlsMessageBodyIn, MlsMessageIn, RatchetTreeIn, Sender},
};
use semver::Version;
use sqlx::{PgConnection, PgTransaction};
use thiserror::Error;
use tls_codec::DeserializeBytes;
use tokio::task::{JoinError, JoinSet};
use tonic::{Request, Response, Status, async_trait};
use tracing::{error, warn};

use crate::{
    auth_service::AsConnector,
    ds::{attachments::ProvisionObjectError, group_state::MemberProfile, process::Provider},
    messages::intra_backend::{DsFanOutMessage, DsFanOutPayload},
    qs::QsConnector,
    rate_limiter::{RateLimiter, RlConfig, RlKey, provider::RlPostgresStorage},
};

use super::{
    Ds,
    group_operation::AddUsersState,
    group_state::{DsGroupState, StorableDsGroupData},
};

pub struct GrpcDs<Qep: QsConnector, As: AsConnector> {
    pub(super) ds: Ds,
    qs_connector: Qep,
    as_connector: As,
}

#[derive(Debug, thiserror::Error)]
enum LoadGroupStateError {
    #[error(transparent)]
    Status(Status),
    #[allow(unused)]
    #[error("Group state expired")]
    Expired,
}

impl<E: Into<Status>> From<E> for LoadGroupStateError {
    fn from(error: E) -> Self {
        Self::Status(error.into())
    }
}

fn to_status(e: LoadGroupStateError) -> Status {
    match e {
        LoadGroupStateError::Status(status) => status,
        LoadGroupStateError::Expired => Status::not_found("Group state expired"),
    }
}

const MAX_CONCURRENT_FANOUTS: usize = 128;

impl<Qep: QsConnector, As: AsConnector> GrpcDs<Qep, As> {
    pub fn new(ds: Ds, qs_connector: Qep, as_connector: As) -> Self {
        Self {
            ds,
            qs_connector,
            as_connector,
        }
    }

    /// Loads encrypted group state from the database and decrypts it.
    ///
    /// If the group state has expired, the group is deleted and not found is returned.
    async fn load_group_state<const LOADED_FOR_UPDATE: bool>(
        &self,
        connection: &mut PgConnection,
        qgid: &QualifiedGroupId,
        ear_key: &GroupStateEarKey,
    ) -> Result<(StorableDsGroupData<LOADED_FOR_UPDATE>, DsGroupState), LoadGroupStateError> {
        let group_data = StorableDsGroupData::load(&mut *connection, qgid)
            .await?
            .ok_or(GroupNotFoundError)?;
        // Group state expiration is disabled for now
        //if group_data.has_expired() {
        //    warn!(%qgid, "Group state has expired, deleting group");
        //    StorableDsGroupData::<true>::delete(connection, qgid)
        //        .await
        //        .map_err(|error| {
        //            error!(%error, "Failed to delete expired group");
        //            Status::internal("Failed to delete expired group")
        //        })?;
        //    return Err(LoadGroupStateError::Expired);
        //}
        let group_state = DsGroupState::decrypt(&group_data.encrypted_group_state, ear_key)?;
        Ok((group_data, group_state))
    }

    async fn load_group_state_for_update(
        &self,
        connection: &mut PgTransaction<'_>,
        qgid: &QualifiedGroupId,
        ear_key: &GroupStateEarKey,
    ) -> Result<(StorableDsGroupData<true>, DsGroupState), LoadGroupStateError> {
        self.load_group_state::<true>(connection, qgid, ear_key)
            .await
    }

    /// Loads the group state for update inside `txn`, returning the (owned) transaction alongside
    /// the state on success.
    ///
    /// If the group state has expired, it has already been deleted; the transaction is committed to
    /// persist the deletion and [`Status::not_found()`] is returned.
    async fn load_for_update_or_not_found<'a>(
        &self,
        mut txn: PgTransaction<'a>,
        qgid: &QualifiedGroupId,
        ear_key: &GroupStateEarKey,
    ) -> Result<(PgTransaction<'a>, DsGroupState, StorableDsGroupData<true>), Status> {
        match self
            .load_group_state_for_update(&mut txn, qgid, ear_key)
            .await
        {
            Ok((group_data, group_state)) => Ok((txn, group_state, group_data)),
            Err(LoadGroupStateError::Expired) => {
                // The group state has expired and has already been deleted.
                // Commit the transaction and return not found.
                txn.commit().await.map_err(|error| {
                    error!(%error, "Failed to commit transaction");
                    Status::internal("Failed to commit transaction")
                })?;
                Err(Status::not_found("Group state expired"))
            }
            Err(LoadGroupStateError::Status(status)) => Err(status),
        }
    }

    async fn load_group_state_immutable(
        &self,
        qgid: &QualifiedGroupId,
        ear_key: &GroupStateEarKey,
    ) -> Result<(StorableDsGroupData<false>, DsGroupState), LoadGroupStateError> {
        let mut connection = self.ds.db_pool.acquire().await.map_err(|error| {
            error!(%error, "Failed to acquire DB connection");
            Status::internal("Failed to acquire DB connection")
        })?;
        self.load_group_state::<false>(&mut connection, qgid, ear_key)
            .await
    }

    /// Fans out a message to the given clients (concurrently).
    ///
    /// The parallelism is limited by a constant. Logs failures but does not
    /// fail the whole operation.
    async fn fan_out_message(
        &self,
        fan_out_payload: impl Into<DsFanOutPayload>,
        destination_clients: impl IntoIterator<Item = identifiers::QsReference>,
        suppress_notifications: bool,
    ) -> TimeStamp {
        let fan_out_payload = fan_out_payload.into();
        let timestamp = fan_out_payload.timestamp();

        let mut join_set: JoinSet<Result<(), <Qep as QsConnector>::EnqueueError>> = JoinSet::new();
        for client_reference in destination_clients {
            while MAX_CONCURRENT_FANOUTS <= join_set.len() {
                join_set
                    .join_next()
                    .await
                    .expect("logic error")
                    .map_err(DistributeMessageError::Join)
                    .and_then(|result| result.map_err(DistributeMessageError::Connector))
                    .inspect_err(|error| error!(%error, "Failed to dispatch message"))
                    .ok();
            }
            join_set.spawn(self.qs_connector.dispatch(DsFanOutMessage {
                payload: fan_out_payload.clone(),
                client_reference,
                suppress_notifications: suppress_notifications.into(),
            }));
        }

        while let Some(result) = join_set.join_next().await {
            result
                .map_err(DistributeMessageError::Join)
                .and_then(|result| result.map_err(DistributeMessageError::Connector))
                .inspect_err(|error| error!(%error, "Failed to dispatch message"))
                .ok();
        }

        timestamp
    }

    /// Fans out a message to the given clients (concurrently) without
    /// triggering notifications.
    ///
    /// The parallelism is limited by a constant. Logs failures but does not
    /// fail the whole operation.
    async fn fan_out_message_without_notifications(
        &self,
        fan_out_payload: impl Into<DsFanOutPayload>,
        destination_clients: impl IntoIterator<Item = identifiers::QsReference>,
    ) -> TimeStamp {
        self.fan_out_message(fan_out_payload, destination_clients, true)
            .await
    }

    async fn encrypt_and_persist(
        &self,
        txn: &mut PgTransaction<'_>,
        mut group_data: StorableDsGroupData<true>,
        group_state: DsGroupState,
        ear_key: &GroupStateEarKey,
    ) -> Result<(), Status> {
        let encrypted_group_state = group_state.encrypt(ear_key)?;
        group_data.encrypted_group_state = encrypted_group_state;
        group_data.update(txn).await.map_err(|error| {
            error!(%error, "Failed to update group state");
            Status::internal("Failed to update group state")
        })?;
        Ok(())
    }

    /// The same as `update_group_state`, but does not perform any verification
    /// of the request.
    async fn update_group_state_without_verification<T: Send>(
        &self,
        qgid: &QualifiedGroupId,
        ear_key: &GroupStateEarKey,
        f: impl AsyncFnOnce(&mut DsGroupState, &mut StorableDsGroupData<true>) -> Result<T, Status>,
    ) -> Result<T, Status> {
        let txn = self.ds.db_pool.begin().await.map_err(|error| {
            error!(%error, "Failed to start transaction");
            Status::internal("Failed to start transaction")
        })?;
        let (mut txn, mut group_state, mut group_data) = self
            .load_for_update_or_not_found(txn, qgid, ear_key)
            .await?;

        let value = f(&mut group_state, &mut group_data).await?;
        let new_epoch = group_state.group().epoch().as_u64();
        self.encrypt_and_persist(&mut txn, group_data, group_state, ear_key)
            .await?;

        txn.commit().await.map_err(|error| {
            error!(%error, "Failed to commit transaction");
            Status::internal("Failed to commit transaction")
        })?;

        // Best-effort cleanup: the transaction is already committed, so failures here are non-fatal.
        super::collision_tags::delete_old(
            &self.ds.db_pool,
            qgid.group_uuid(),
            new_epoch,
            MAX_PAST_EPOCHS as u64,
        )
        .await
        .inspect_err(|error| {
            error!(%error, "Failed to clean up old collision tags");
        })
        .ok();

        Ok(value)
    }

    /// Verifies the given request and applies the necessary changes to the
    /// group state.
    ///
    /// This function loads the group state for update, calls the provided async
    /// function with the group state and the storable group data, and then
    /// persists any changes to the database. The transaction is committed if
    /// the function returns `Ok`, and rolled back if the function returns
    /// `Err`.
    ///
    /// If the group state has expired, it is deleted and not found is returned.
    async fn update_group_state<R, P, T: Send, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
        sender_index: Option<LeafNodeIndex>,
        f: impl AsyncFnOnce(LeafVerificationData<P, true>) -> Result<T, Status>,
    ) -> Result<T, Status>
    where
        R: WithGroupStateEarKey + WithMessage + VerifiableRequest,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let ear_key = request.inner().ear_key()?;
        let message = request.inner().message()?;
        let qgid = message.validated_qgid(self.ds.own_domain())?;

        let txn = self.ds.db_pool.begin().await.map_err(|error| {
            error!(%error, "Failed to start transaction");
            Status::internal("Failed to start transaction")
        })?;
        let (mut txn, mut group_state, group_data) = self
            .load_for_update_or_not_found(txn, &qgid, &ear_key)
            .await?;

        let (payload, sender_index, message) = verify_message(request, &group_state, sender_index)?;

        let verification_data = LeafVerificationData {
            ear_key: &ear_key,
            group_state: &mut group_state,
            sender_index,
            payload,
            message,
        };

        let value = f(verification_data).await?;

        let new_epoch = group_state.group().epoch().as_u64();
        self.encrypt_and_persist(&mut txn, group_data, group_state, &ear_key)
            .await?;

        txn.commit().await.map_err(|error| {
            error!(%error, "Failed to commit transaction");
            Status::internal("Failed to commit transaction")
        })?;

        // Best-effort cleanup: the transaction is already committed, so failures here are non-fatal.
        super::collision_tags::delete_old(
            &self.ds.db_pool,
            qgid.group_uuid(),
            new_epoch,
            MAX_PAST_EPOCHS as u64,
        )
        .await
        .inspect_err(|error| error!(%error, "Failed to clean up old collision tags"))
        .ok();

        Ok(value)
    }

    /// Verifies the given request and applies the necessary changes to the group state.
    ///
    /// This function loads the group state of the T and PQ group for update, calls the provided
    /// async function with the group state and the storable group data, and then persists any
    /// changes to the database. The transaction is committed if the function returns `Ok`, and
    /// rolled back if the function returns `Err`.
    ///
    /// If the group state has expired, it is deleted and [`Status::not_found()`] is returned.
    async fn update_apq_group_state<R, P, T: Send, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
        f: impl AsyncFnOnce(ApqVerificationData<'_, P>) -> Result<ApqFanOut<T>, Status>,
    ) -> Result<T, Status>
    where
        R: WithGroupStateEarKey + WithApqMessage + VerifiableRequest,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let ear_key = request.inner().ear_key()?;
        let (t_message, pq_message) = request.inner().apq_message()?;

        let t_qgid = t_message.validated_qgid(self.ds.own_domain())?;
        let pq_qgid = pq_message.validated_qgid(self.ds.own_domain())?;

        let txn = self.ds.db_pool.begin().await.map_err(|error| {
            error!(%error, "Failed to start transaction");
            Status::internal("Failed to start transaction")
        })?;

        let (txn, mut t_group_state, t_group_data) = self
            .load_for_update_or_not_found(txn, &t_qgid, &ear_key)
            .await?;
        let (payload, t_sender_index) =
            resolve_and_verify(request, &t_message, &t_group_state, None)?;

        let (mut txn, mut pq_group_state, pq_group_data) = self
            .load_for_update_or_not_found(txn, &pq_qgid, &ear_key)
            .await?;

        // Check that the T/PQ indices and signature keys match
        let Sender::Member(pq_sender_index) = *pq_message.sender().ok_or_missing_field("sender")?
        else {
            return Err(Status::invalid_argument(
                "unexpected pq sender: expected member",
            ));
        };
        if t_sender_index != pq_sender_index {
            return Err(Status::invalid_argument(
                "t and pq sender indices do not match",
            ));
        }
        let t_sender = t_group_state
            .group()
            .leaf(t_sender_index)
            .ok_or(Status::invalid_argument("unknown sender"))?;
        let pq_sender = pq_group_state
            .group()
            .leaf(pq_sender_index)
            .ok_or(Status::invalid_argument("unknown PQ sender"))?;
        if t_sender.signature_key() != pq_sender.signature_key() {
            return Err(Status::invalid_argument(
                "t and pq credentials do not match",
            ));
        }

        // Process group operation
        let ApqFanOut {
            broadcast: (qs_payload, destination_clients),
            individual,
            value,
        } = f(ApqVerificationData {
            payload,
            t_group_state: &mut t_group_state,
            pq_group_state: &mut pq_group_state,
            t_message,
            pq_message,
            t_sender_index,
            ear_key: &ear_key,
        })
        .await?;

        // Persist and commit the DS state
        let t_new_epoch = t_group_state.group().epoch().as_u64();
        self.encrypt_and_persist(&mut txn, t_group_data, t_group_state, &ear_key)
            .await?;
        self.encrypt_and_persist(&mut txn, pq_group_data, pq_group_state, &ear_key)
            .await?;
        txn.commit().await.map_err(|error| {
            error!(%error, "Failed to commit transaction");
            Status::internal("Failed to commit transaction")
        })?;

        // Fan out
        self.fan_out_message_without_notifications(qs_payload, destination_clients)
            .await;
        for message in individual {
            if let Err(error) = self
                .qs_connector
                .dispatch(message)
                .await
                .map_err(DistributeMessageError::Connector)
            {
                error!(%error, "Failed to dispatch message");
            };
        }

        // Best-effort cleanup: the transaction is already committed, so failures here are non-fatal.
        super::collision_tags::delete_old(
            &self.ds.db_pool,
            t_qgid.group_uuid(),
            t_new_epoch,
            MAX_PAST_EPOCHS as u64,
        )
        .await
        .inspect_err(|error| {
            error!(%error, "Failed to clean up old collision tags");
        })
        .ok();

        Ok(value)
    }

    fn verify_client_version(
        &self,
        client_metadata: Option<&ClientMetadata>,
    ) -> Result<Option<Version>, Status> {
        let client_version_req = self.ds.client_version_req.as_ref();
        crate::version::verify_client_version(client_version_req, client_metadata)
    }
}

fn verify_message<R, P, const TAG: u32>(
    request: SignedRequest<R, TAG>,
    group_state: &DsGroupState,
    sender_index: Option<LeafNodeIndex>,
) -> Result<(P, LeafNodeIndex, AssistedMessageIn), Status>
where
    R: WithMessage + VerifiableRequest,
    P: VerifiedStruct<SignedRequest<R, TAG>>,
{
    let message = request.inner().message()?;
    let (payload, sender_index) = resolve_and_verify(request, &message, group_state, sender_index)?;
    Ok((payload, sender_index, message))
}

/// Resolves the sender leaf or `message` and verifies request signature against the leaf signature
/// key.
fn resolve_and_verify<R, P>(
    request: R,
    message: &AssistedMessageIn,
    group_state: &DsGroupState,
    sender_index: Option<LeafNodeIndex>,
) -> Result<(P, LeafNodeIndex), Status>
where
    R: Verifiable,
    P: VerifiedStruct<R>,
{
    let sender_index = sender_index.map(Ok).unwrap_or_else(|| {
        match *message.sender().ok_or_missing_field("sender")? {
            Sender::Member(sender_index) => Ok(sender_index),
            _ => Err(Status::invalid_argument(
                "unexpected sender: expected member",
            )),
        }
    })?;
    let verifying_key: LeafVerifyingKeyRef = group_state
        .group()
        .leaf(sender_index)
        .ok_or(Status::invalid_argument("unknown sender"))?
        .signature_key()
        .into();
    let payload: P = request.verify(verifying_key).map_err(InvalidSignature)?;
    Ok((payload, sender_index))
}

/// Extracted data in leaf verification
struct LeafVerificationData<'a, P, const LOADED_FOR_UPDATE: bool> {
    ear_key: &'a GroupStateEarKey,
    group_state: &'a mut DsGroupState,
    sender_index: LeafNodeIndex,
    payload: P,
    message: AssistedMessageIn,
}

struct ApqVerificationData<'a, P> {
    payload: P,
    t_group_state: &'a mut DsGroupState,
    pq_group_state: &'a mut DsGroupState,
    t_message: AssistedMessageIn,
    pq_message: AssistedMessageIn,
    t_sender_index: LeafNodeIndex,
    ear_key: &'a GroupStateEarKey,
}

struct ApqFanOut<T> {
    broadcast: (QsQueueMessagePayload, Vec<identifiers::QsReference>),
    individual: Vec<DsFanOutMessage>,
    value: T,
}

#[async_trait]
impl<Qep: QsConnector, As: AsConnector> DeliveryService for GrpcDs<Qep, As> {
    async fn request_group_id(
        &self,
        request: Request<RequestGroupIdRequest>,
    ) -> Result<Response<RequestGroupIdResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;
        let qgid = self.ds.request_group_id().await;

        let pq_qgid = if request.request_pq_group_id {
            Some(self.ds.request_group_id().await)
        } else {
            None
        };

        let group_profile_provisioning =
            if let Some(group_profile_size) = request.group_profile_size {
                let content_length = group_profile_size
                    .try_into()
                    .map_err(|_| Status::invalid_argument("invalid group profile size"))?;
                match self
                    .ds
                    .provision_object(StorageObjectType::GroupProfile, Some(content_length), false)
                    .await
                {
                    Ok(response) => Some(response),
                    Err(ProvisionObjectError::NoStorageConfigured) => None,
                    Err(error) => {
                        error!(%error, "Failed to provision attachment");
                        return Err(Status::internal("Failed to provision attachment"));
                    }
                }
            } else {
                None
            };

        Ok(Response::new(RequestGroupIdResponse {
            group_id: Some(qgid.ref_into()),
            pq_group_id: pq_qgid.map(|id| id.ref_into()),
            group_profile_provisioning,
        }))
    }

    async fn create_group(
        &self,
        request: Request<SignedRequest<CreateGroupRequest>>,
    ) -> Result<Response<CreateGroupResponse>, Status> {
        let request = request.into_inner();

        // TODO: signature verification?
        let request = request.into_inner();
        let payload = request.payload.ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;
        let qgid = payload.validated_qgid(&self.ds.own_domain)?;
        let ear_key = payload.ear_key()?;

        let reserved_group_id = self
            .ds
            .claim_reserved_group_id(qgid.group_uuid())
            .await
            .ok_or_else(|| Status::invalid_argument("unreserved group id"))?;

        // create group
        let group_info: MlsMessageIn = payload
            .group_info
            .as_ref()
            .ok_or_missing_field("group_info")?
            .try_ref_into()
            .invalid_tls("group_info")?;
        let MlsMessageBodyIn::GroupInfo(group_info) = group_info.extract() else {
            return Err(Status::invalid_argument("invalid message"));
        };

        let ratchet_tree: RatchetTreeIn = payload
            .ratchet_tree
            .as_ref()
            .ok_or_missing_field("ratchet_tree")?
            .try_ref_into()
            .invalid_tls("ratchet_tree")?;

        let provider = Provider::default();
        let group = Group::new(&provider, group_info.clone(), ratchet_tree).map_err(|error| {
            error!(%error, "failed to create group");
            Status::internal("failed to create group")
        })?;

        // Extract user id
        let members = group.members().collect::<Vec<_>>();

        let &[own_leaf] = &members.as_slice() else {
            error!(members = %members.len(), "group must have exactly one member");
            return Err(Status::invalid_argument(
                "group must have exactly one member",
            ));
        };

        let credential =
            ClientCredential::tls_deserialize_exact_bytes(own_leaf.credential.serialized_content())
                .map_err(|_| Status::invalid_argument("invalid credential"))?;
        let user_id = credential.user_id().uuid();

        // Configure the rate-limiting
        let rl_key = RlKey::new(
            b"ds",
            b"reserve_group_id",
            &[b"user_uuid", user_id.as_bytes()],
        );
        let config = RlConfig {
            max_requests: 100,
            time_window: TimeDelta::hours(1),
        };
        let rl_storage = RlPostgresStorage::new(self.ds.db_pool.clone());
        let rl = RateLimiter::new(config, rl_storage);

        // Apply the rate-limiting
        if !rl.allowed(rl_key).await {
            return Err(Status::resource_exhausted(
                "Too many requests, please try again later",
            ));
        }

        // encrypt and store group state
        let encrypted_user_profile_key = payload
            .encrypted_user_profile_key
            .ok_or_missing_field("encrypted_user_profile_key")?
            .try_into()?;
        let creator_client_reference = payload
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
            creator_client_reference,
            room_state,
        );
        let encrypted_group_state = group_state.encrypt(&ear_key)?;

        StorableDsGroupData::new_and_store(
            &self.ds.db_pool,
            reserved_group_id,
            encrypted_group_state,
        )
        .await
        .map_err(|error| {
            error!(%error, "failed to store group state");
            Status::internal("failed to store group state")
        })?;

        Ok(Response::new(CreateGroupResponse {}))
    }

    async fn create_apq_group(
        &self,
        request: Request<SignedRequest<CreateApqGroupRequest>>,
    ) -> Result<Response<CreateApqGroupResponse>, Status> {
        let request = request.into_inner();

        // First use unverified payload; later we verify it using the client credential from the
        // leaf node.
        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        // Extract chat related data
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

        // Create t group state
        let (t_qgid, t_group_state, ear_key) = self.extract_group_state(
            payload
                .clone()
                .t_group_data
                .ok_or_missing_field("t_group_data")?,
            &encrypted_user_profile_key,
            &creator_client_reference,
            &room_state,
        )?;
        let t_client_credential = Self::extract_credential(&t_group_state.group)?;

        // Configure and apply rate-limiting
        let rl_key = RlKey::new(
            b"ds",
            b"reserve_group_id",
            &[
                b"user_uuid",
                t_client_credential.user_id().uuid().as_bytes(),
            ],
        );
        let config = RlConfig {
            max_requests: 100,
            time_window: TimeDelta::hours(1),
        };
        let rl_storage = RlPostgresStorage::new(self.ds.db_pool.clone());
        let rl = RateLimiter::new(config, rl_storage);
        if !rl.allowed(rl_key).await {
            return Err(Status::resource_exhausted(
                "Too many requests, please try again later",
            ));
        }

        // Now we can verify the payload
        let payload: CreateApqGroupPayload = request
            .verify(t_client_credential.verifying_key())
            .map_err(InvalidSignature)?;

        // Extract pq group state (PQ group uses the same ear_key as the T group)
        let (pq_qgid, pq_group_state, _) = Self::extract_group_state(
            &self,
            payload.pq_group_data.ok_or_missing_field("pq_group_data")?,
            &encrypted_user_profile_key,
            &creator_client_reference,
            &room_state,
        )?;

        // Check that the t and pq client signature keys match
        Self::verify_signing_key(&pq_group_state.group, t_client_credential.verifying_key())?;

        // Encrypt and store group state
        let t_reserved_group_id = self
            .ds
            .claim_reserved_group_id(t_qgid.group_uuid())
            .await
            .ok_or_else(|| Status::invalid_argument("unreserved group id"))?;
        let pq_reserved_group_id = self
            .ds
            .claim_reserved_group_id(pq_qgid.group_uuid())
            .await
            .ok_or_else(|| Status::invalid_argument("unreserved group id"))?;
        let encrypted_t_group_state = t_group_state.encrypt(&ear_key)?;
        let encrypted_pq_group_state = pq_group_state.encrypt(&ear_key)?;

        let mut txn = self.ds.db_pool.begin().await.map_err(|error| {
            error!(%error, "failed to start transaction");
            Status::internal("database error")
        })?;
        StorableDsGroupData::new_and_store(
            txn.as_mut(),
            t_reserved_group_id,
            encrypted_t_group_state,
        )
        .await
        .map_err(|error| {
            error!(%error, "failed to store t group state");
            Status::internal("failed to store t group state")
        })?;
        StorableDsGroupData::new_and_store(
            txn.as_mut(),
            pq_reserved_group_id,
            encrypted_pq_group_state,
        )
        .await
        .map_err(|error| {
            error!(%error, "failed to store pq group state");
            Status::internal("failed to store pq group state")
        })?;
        txn.commit().await.map_err(|error| {
            error!(%error, "failed to commit transaction");
            Status::internal("database error")
        })?;

        Ok(Response::new(CreateApqGroupResponse {}))
    }

    async fn welcome_info(
        &self,
        request: Request<SignedRequest<WelcomeInfoRequest, 2>>,
    ) -> Result<Response<WelcomeInfoResponse>, Status> {
        let request = request.into_inner();

        let sender: ClientVerifyingKey = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .sender
            .clone()
            .ok_or_missing_field("sender")?
            .into();
        let payload: WelcomeInfoPayload = request.verify(&sender).map_err(InvalidSignature)?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let qgid = payload.validated_qgid(&self.ds.own_domain)?;
        let ear_key = payload.ear_key()?;
        let (_, mut group_state) = self
            .load_group_state_immutable(&qgid, &ear_key)
            .await
            .map_err(to_status)?;

        let welcome_info_params = WelcomeInfoParams {
            sender: sender.clone(),
            epoch: payload.epoch.ok_or_missing_field("epoch")?.into(),
            group_id: qgid.into(),
        };
        let ratchet_tree = group_state
            .welcome_info(welcome_info_params)
            .ok_or(NoWelcomeInfoFound)?;
        Ok(Response::new(WelcomeInfoResponse {
            ratchet_tree: Some(ratchet_tree.try_ref_into().invalid_tls("ratchet_tree")?),
            encrypted_user_profile_keys: group_state
                .encrypted_user_profile_keys()
                .into_iter()
                .map(From::from)
                .collect(),
            room_state: Some(
                group_state
                    .room_state
                    .unverified()
                    .try_ref_into()
                    .invalid_tls("room_state")?,
            ),
            indexed_encrypted_user_profile_keys: group_state
                .member_profiles
                .into_iter()
                .map(|(index, profile)| IndexedEncryptedUserProfileKey {
                    leaf_index: index.u32(),
                    encrypted_user_profile_key: Some(profile.encrypted_user_profile_key.into()),
                })
                .collect(),
        }))
    }

    async fn external_commit_info(
        &self,
        request: Request<ExternalCommitInfoRequest>,
    ) -> Result<Response<ExternalCommitInfoResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;

        let qgid = request.qgid.ok_or_missing_field("qgid")?.try_ref_into()?;
        let ear_key = request
            .group_state_ear_key
            .ok_or_missing_field("group_state_ear_key")?
            .try_ref_into()?;

        let (_, group_state) = self
            .load_group_state_immutable(&qgid, &ear_key)
            .await
            .map_err(to_status)?;

        let commit_info = group_state.external_commit_info();

        Ok(Response::new(ExternalCommitInfoResponse {
            group_info: Some(
                commit_info
                    .group_info
                    .try_into()
                    .invalid_tls("group_info")?,
            ),
            ratchet_tree: Some(
                commit_info
                    .ratchet_tree
                    .try_ref_into()
                    .invalid_tls("ratchet_tree")?,
            ),
            encrypted_user_profile_keys: commit_info
                .encrypted_user_profile_keys
                .into_iter()
                .map(From::from)
                .collect(),
            room_state: Some(
                commit_info
                    .room_state
                    .unverified()
                    .try_ref_into()
                    .invalid_tls("room_state")?,
            ),
            proposals: commit_info.proposals.into_iter().map(From::from).collect(),
            indexed_encrypted_user_profile_keys: group_state
                .member_profiles
                .into_iter()
                .map(|(index, profile)| IndexedEncryptedUserProfileKey {
                    leaf_index: index.u32(),
                    encrypted_user_profile_key: Some(profile.encrypted_user_profile_key.into()),
                })
                .collect(),
        }))
    }

    async fn connection_group_info(
        &self,
        request: Request<ConnectionGroupInfoRequest>,
    ) -> Result<Response<ConnectionGroupInfoResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;

        let qgid: QualifiedGroupId = request
            .group_id
            .ok_or_missing_field("group_id")?
            .try_ref_into()?;
        let ear_key: GroupStateEarKey = request
            .group_state_ear_key
            .ok_or_missing_field("group_state_ear_key")?
            .try_ref_into()?;

        let (_, group_state) = self
            .load_group_state_immutable(&qgid, &ear_key)
            .await
            .map_err(to_status)?;
        let commit_info = group_state.external_commit_info();

        let group_info = commit_info
            .group_info
            .try_into()
            .invalid_tls("group_info")?;
        let ratchet_tree = commit_info
            .ratchet_tree
            .try_ref_into()
            .invalid_tls("ratchet_tree")?;
        Ok(Response::new(ConnectionGroupInfoResponse {
            group_info: Some(group_info),
            ratchet_tree: Some(ratchet_tree),
            encrypted_user_profile_keys: commit_info
                .encrypted_user_profile_keys
                .into_iter()
                .map(From::from)
                .collect(),
            room_state: Some(
                commit_info
                    .room_state
                    .unverified()
                    .try_ref_into()
                    .invalid_tls("room_state")?,
            ),
            proposals: commit_info.proposals.into_iter().map(From::from).collect(),
            indexed_encrypted_user_profile_keys: group_state
                .member_profiles
                .into_iter()
                .map(|(index, profile)| IndexedEncryptedUserProfileKey {
                    leaf_index: index.u32(),
                    encrypted_user_profile_key: Some(profile.encrypted_user_profile_key.into()),
                })
                .collect(),
        }))
    }

    async fn join_connection_group(
        &self,
        request: Request<JoinConnectionGroupRequest>,
    ) -> Result<Response<JoinConnectionGroupResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;

        let external_commit: AssistedMessageIn = request
            .external_commit
            .ok_or_missing_field("external_commit")?
            .try_ref_into()
            .invalid_tls("external_commit")?;
        let qgid = external_commit.validated_qgid(self.ds.own_domain())?;
        let ear_key = request
            .group_state_ear_key
            .ok_or_missing_field("group_state_ear_key")?
            .try_ref_into()?;

        let timestamp = self
            .update_group_state_without_verification(
                &qgid,
                &ear_key,
                async |group_state, _group_data| {
                    let params = JoinConnectionGroupParams {
                        external_commit,
                        qs_client_reference: request
                            .qs_client_reference
                            .ok_or_missing_field("qs_client_reference")?
                            .try_into()?,
                    };

                    // Destination clients do not contain self yet, TODO: will need to be adjusted with virtual clients
                    let destination_clients: Vec<_> = group_state.destination_clients().collect();

                    let group_message = group_state.join_connection_group(params)?;

                    group_state.proposals.clear();

                    let timestamp = self
                        .fan_out_message_without_notifications(group_message, destination_clients)
                        .await;
                    Ok(timestamp)
                },
            )
            .await?;

        Ok(Response::new(JoinConnectionGroupResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }

    async fn resync(
        &self,
        request: Request<SignedRequest<ResyncRequest>>,
    ) -> Result<Response<ResyncResponse>, Status> {
        let request = request.into_inner();

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let sender_index: LeafNodeIndex = payload.sender.ok_or_missing_field("sender")?.into();

        let timestamp = self
            .update_group_state(request, Some(sender_index), async |verified_data| {
                let LeafVerificationData::<'_, ResyncPayload, true> {
                    group_state,
                    sender_index,
                    message: external_commit,
                    ..
                } = verified_data;

                let destination_clients: Vec<_> = group_state.destination_clients().collect();

                let group_message = group_state.resync_client(external_commit, sender_index)?;

                group_state.proposals.clear();

                let timestamp = self
                    .fan_out_message_without_notifications(group_message, destination_clients)
                    .await;
                Ok(timestamp)
            })
            .await?;

        Ok(Response::new(ResyncResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }

    async fn self_remove(
        &self,
        request: Request<SignedRequest<SelfRemoveRequest, 2>>,
    ) -> Result<Response<SelfRemoveResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let timestamp = self
            .update_group_state(request, None, async |verification_data| {
                let LeafVerificationData::<'_, SelfRemovePayload, true> {
                    group_state,
                    message: remove_proposal,
                    ..
                } = verification_data;

                let destination_clients: Vec<_> = group_state.destination_clients().collect();

                let group_message = group_state.self_remove_client(remove_proposal)?;

                let timestamp = self
                    .fan_out_message_without_notifications(group_message, destination_clients)
                    .await;
                Ok(timestamp)
            })
            .await?;

        Ok(Response::new(SelfRemoveResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }

    async fn apq_self_remove(
        &self,
        request: Request<SignedRequest<ApqSelfRemoveRequest, 1>>,
    ) -> Result<Response<ApqSelfRemoveResponse>, Status> {
        let request = request.into_inner();

        // Short circuit requests without a signature
        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let fanout_timestamp = self
            .update_apq_group_state(
                request,
                async |ApqVerificationData {
                           payload: _,
                           t_group_state,
                           pq_group_state,
                           t_message,
                           pq_message,
                           t_sender_index: _,
                           ear_key: _,
                       }: ApqVerificationData<'_, ApqSelfRemovePayload>| {
                    let destination_clients: Vec<_> = t_group_state.destination_clients().collect();

                    let pq_serialized =
                        pq_group_state.self_remove_client_without_room_state(pq_message)?;
                    let t_serialized = t_group_state.self_remove_client(t_message)?;

                    let timestamp = TimeStamp::now();
                    let apq_payload = QsQueueMessagePayload::apq_mls_message(
                        timestamp,
                        SerializedMlsMessage::combine_apq(t_serialized, pq_serialized),
                    );

                    Ok(ApqFanOut {
                        broadcast: (apq_payload, destination_clients),
                        individual: Default::default(),
                        value: timestamp,
                    })
                },
            )
            .await?;

        Ok(Response::new(ApqSelfRemoveResponse {
            fanout_timestamp: Some(fanout_timestamp.into()),
        }))
    }

    async fn send_message(
        &self,
        request: Request<SignedRequest<SendMessageRequest>>,
    ) -> Result<Response<SendMessageResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let sender_index = payload.sender.ok_or_missing_field("sender")?.into();

        let ear_key = request.inner().ear_key()?;
        let message = request.inner().message()?;
        let qgid = message.validated_qgid(self.ds.own_domain())?;

        // No transaction needed as we do not update the group state and
        // application messages are out-of-order tolerant.
        let (_, group_state) = self
            .load_group_state_immutable(&qgid, &ear_key)
            .await
            .map_err(to_status)?;

        // verify signature
        let verifying_key: LeafVerifyingKeyRef = group_state
            .group()
            .leaf(sender_index)
            .ok_or_else(|| Status::invalid_argument("unknown sender"))?
            .signature_key()
            .into();
        let payload: SendMessagePayload =
            request.verify(verifying_key).map_err(InvalidSignature)?;

        if let Some(tags) = payload.collision_tags {
            let msg_epoch = message.epoch().as_u64();
            super::collision_tags::check_and_insert(
                &self.ds.db_pool,
                qgid.group_uuid(),
                msg_epoch as i64,
                tags,
            )
            .await?;
        }

        let destination_clients = group_state.destination_clients();

        // Messages from legacy clients won't have this field set. Default to false.
        let suppress_notifications = payload.suppress_notifications.unwrap_or(false);

        let timestamp = self
            .fan_out_message(
                message.into_serialized_mls_message(),
                destination_clients,
                suppress_notifications,
            )
            .await;

        Ok(Response::new(SendMessageResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }

    async fn delete_group(
        &self,
        request: Request<SignedRequest<DeleteGroupRequest>>,
    ) -> Result<Response<DeleteGroupResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let timestamp = self
            .update_group_state(request, None, async |verification_data| {
                let LeafVerificationData::<'_, DeleteGroupPayload, true> {
                    group_state,
                    message: commit,
                    ..
                } = verification_data;

                let destination_clients: Vec<_> = group_state.destination_clients().collect();

                let group_message = group_state.delete_group(commit)?;

                group_state.proposals.clear();

                let timestamp = self
                    .fan_out_message_without_notifications(group_message, destination_clients)
                    .await;
                Ok(timestamp)
            })
            .await?;

        Ok(Response::new(DeleteGroupResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }

    async fn group_operation(
        &self,
        request: Request<SignedRequest<GroupOperationRequest>>,
    ) -> Result<Response<GroupOperationResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let (destination_clients, fan_out_payload, individual_fan_out_messages) = self
            .update_group_state(request, None, async |verification_data| {
                let LeafVerificationData::<'_, GroupOperationPayload, true> {
                    ear_key,
                    group_state,
                    sender_index,
                    payload,
                    message: commit,
                    ..
                } = verification_data;

                // If specified by the client, we update our internal mapping of leaves to MemberProfile.
                // This is used to recover groups where the OpenMLS leaves and its representation on the DS drifted.
                if let Some((qs_client_reference, encrypted_user_profile_key)) = payload
                    .qs_client_reference
                    .zip(payload.encrypted_user_profile_key)
                {
                    group_state.member_profiles.insert(
                        sender_index,
                        MemberProfile {
                            leaf_index: sender_index,
                            client_queue_config: qs_client_reference.try_into()?,
                            activity_time: TimeStamp::now(),
                            activity_epoch: group_state.group().epoch(),
                            encrypted_user_profile_key: encrypted_user_profile_key.try_into()?,
                        },
                    );
                }

                let params = GroupOperationParams {
                    commit,
                    add_users_info_option: payload
                        .add_users_info
                        .map(|info| info.try_into())
                        .transpose()?,
                };

                let destination_clients: Vec<_> = group_state.destination_clients().collect();

                let (group_message, mut individual_fan_out_messages) =
                    group_state.group_operation(params, ear_key).await?;

                group_state.proposals.clear();

                let fan_out_payload: DsFanOutPayload = group_message.into();

                let commit_response = group_state
                    .create_commit_response(sender_index, fan_out_payload.timestamp())?;
                individual_fan_out_messages.push(commit_response);

                Ok((
                    destination_clients,
                    fan_out_payload,
                    individual_fan_out_messages,
                ))
            })
            .await?;

        // Fan out the commit message to existing members
        let timestamp = self
            .fan_out_message_without_notifications(fan_out_payload, destination_clients)
            .await;

        // Dispatch individual fan out messages to new members
        // TODO: Should we fan out the individual fan out messages concurrently?
        for message in individual_fan_out_messages {
            if let Err(e) = self
                .qs_connector
                .dispatch(message)
                .await
                .map_err(DistributeMessageError::Connector)
            {
                error!(%e, "Failed to dispatch message");
            };
        }

        Ok(Response::new(GroupOperationResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }

    async fn apq_group_operation(
        &self,
        request: Request<SignedRequest<ApqGroupOperationRequest>>,
    ) -> Result<Response<ApqGroupOperationResponse>, Status> {
        let request = request.into_inner();

        // Short circuit requests without a signature
        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let fanout_timestamp = self
            .update_apq_group_state(
                request,
                async |ApqVerificationData {
                           payload,
                           t_group_state,
                           pq_group_state,
                           t_message,
                           pq_message,
                           t_sender_index,
                           ear_key,
                       }: ApqVerificationData<ApqGroupOperationPayload>| {
                    // If specified by the client, we update our internal mapping of leaves to
                    // MemberProfile. This is used to recover groups where the OpenMLS leaves and
                    // its representation on the DS drifted.
                    if let Some((qs_client_reference, encrypted_user_profile_key)) = payload
                        .qs_client_reference
                        .zip(payload.encrypted_user_profile_key)
                    {
                        t_group_state.member_profiles.insert(
                            t_sender_index,
                            MemberProfile {
                                leaf_index: t_sender_index,
                                client_queue_config: qs_client_reference.try_into()?,
                                activity_time: TimeStamp::now(),
                                activity_epoch: t_group_state.group().epoch(),
                                encrypted_user_profile_key: encrypted_user_profile_key
                                    .try_into()?,
                            },
                        );
                    }

                    // Process group operation
                    let add_users_info: Option<client_ds::ApqAddUsersInfo> = payload
                        .add_users_info
                        .map(|info| info.try_into())
                        .transpose()?;
                    let (t_add_users_info, pq_add_users_info) =
                        add_users_info.map(|info| info.split()).unzip();

                    // Make sure you collect destination clients before processing the commit so that new invitees (added by
                    // this commit) don't receive the commit message before their welcome bundle.
                    let destination_clients: Vec<_> = t_group_state.destination_clients().collect();

                    let (serialized_apq_message, t_add_users_state, pq_welcome) =
                        DsGroupState::process_apq_group_operation(
                            t_group_state,
                            pq_group_state,
                            t_message,
                            pq_message,
                            t_add_users_info,
                            pq_add_users_info,
                        )?;

                    // Fan out the commit message to the destination clients
                    let timestamp = TimeStamp::now();

                    let apq_payload =
                        QsQueueMessagePayload::apq_mls_message(timestamp, serialized_apq_message);

                    // Generate welcome bundles for new members
                    let mut individual_fan_out_messages = match (t_add_users_state, pq_welcome) {
                        (
                            Some(AddUsersState {
                                added_users,
                                welcome: t_welcome,
                            }),
                            Some(pq_welcome),
                        ) => t_group_state.generate_apq_fan_out_messages(
                            added_users,
                            &t_welcome,
                            &pq_welcome,
                            ear_key,
                        )?,
                        (None, None) => Vec::new(),
                        _ => {
                            warn!("T and PQ group operations inconsistently add users");
                            return Err(Status::invalid_argument(
                                "Inconsistent APQ add users info",
                            ));
                        }
                    };

                    t_group_state.proposals.clear();
                    pq_group_state.proposals.clear();

                    let commit_response =
                        t_group_state.create_commit_response(t_sender_index, timestamp)?;
                    individual_fan_out_messages.push(commit_response);

                    Ok(ApqFanOut {
                        broadcast: (apq_payload, destination_clients),
                        individual: individual_fan_out_messages,
                        value: timestamp,
                    })
                },
            )
            .await?;

        Ok(Response::new(ApqGroupOperationResponse {
            fanout_timestamp: Some(fanout_timestamp.into()),
        }))
    }

    async fn update_profile_key(
        &self,
        request: Request<SignedRequest<UpdateProfileKeyRequest, 2>>,
    ) -> Result<Response<UpdateProfileKeyResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let ear_key = request.inner().ear_key()?;
        let qgid = payload.validated_qgid(self.ds.own_domain())?;
        let sender_index = payload.sender.ok_or_missing_field("sender")?.into();

        self.update_group_state_without_verification(
            &qgid,
            &ear_key,
            async |group_state, _group_data| {
                // verify signature
                let verifying_key: LeafVerifyingKeyRef = group_state
                    .group()
                    .leaf(sender_index)
                    .ok_or_else(|| Status::invalid_argument("unknown sender"))?
                    .signature_key()
                    .into();
                let payload: UpdateProfileKeyPayload =
                    request.verify(verifying_key).map_err(InvalidSignature)?;

                let user_profile_key = payload
                    .encrypted_user_profile_key
                    .ok_or_missing_field("user_profile_key")?
                    .try_into()?;
                let params = UserProfileKeyUpdateParams {
                    group_id: qgid.clone().into(),
                    sender_index,
                    user_profile_key,
                };

                let fan_out_payload =
                    QsQueueMessagePayload::try_from(&params).tls_failed("QsQueueMessagePayload")?;

                group_state.update_user_profile_key(sender_index, params.user_profile_key)?;

                let destination_clients: Vec<_> = group_state.destination_clients().collect();

                self.fan_out_message_without_notifications(fan_out_payload, destination_clients)
                    .await;
                Ok(())
            },
        )
        .await?;

        Ok(Response::new(UpdateProfileKeyResponse {}))
    }

    async fn provision_attachment(
        &self,
        request: Request<SignedRequest<ProvisionAttachmentRequest>>,
    ) -> Result<Response<ProvisionAttachmentResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        // the payload can be signed in different ways depending of the object type
        let payload: ProvisionAttachmentPayload = match payload.object_type() {
            StorageObjectType::Unspecified
            | StorageObjectType::Attachment
            | StorageObjectType::GroupProfile
            | StorageObjectType::UserProfile => {
                let ear_key = payload.ear_key()?;
                let qgid = payload.validated_qgid(self.ds.own_domain())?;
                let sender_index = payload.sender.ok_or_missing_field("sender")?.into();

                let (_group_data, group_state) = self
                    .load_group_state_immutable(&qgid, &ear_key)
                    .await
                    .map_err(to_status)?;

                let verifying_key: LeafVerifyingKeyRef = group_state
                    .group()
                    .leaf(sender_index)
                    .ok_or_else(|| Status::invalid_argument("unknown sender"))?
                    .signature_key()
                    .into();

                request.verify(verifying_key).map_err(InvalidSignature)?
            }
            StorageObjectType::DebugLogs => {
                let user_id = payload
                    .user_id
                    .clone()
                    .ok_or_missing_field("user_id")?
                    .try_into()?;
                let client_verifying_key = self
                    .as_connector
                    .client_verifying_key(&user_id)
                    .await
                    .map_err(|error| {
                        error!(%error, "failed to load client verifying key from AS");
                        Status::internal("failed to load client verifying key")
                    })?
                    .ok_or_else(|| Status::not_found("user not found"))?;

                request
                    .verify(&client_verifying_key)
                    .map_err(InvalidSignature)?
            }
        };

        let content_length = payload
            .content_length
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid content length"))?;

        let response = self
            .ds
            .provision_object(
                payload.object_type.try_into().unwrap_or_default(),
                Some(content_length),
                payload.use_post_policy,
            )
            .await?;

        Ok(Response::new(response))
    }

    async fn get_attachment_url(
        &self,
        request: Request<SignedRequest<GetAttachmentUrlRequest>>,
    ) -> Result<Response<GetAttachmentUrlResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        // the payload can be signed in different ways depending of the object type
        let payload: GetAttachmentUrlPayload = match payload.object_type() {
            StorageObjectType::Unspecified
            | StorageObjectType::Attachment
            | StorageObjectType::GroupProfile
            | StorageObjectType::UserProfile => {
                let ear_key = payload.ear_key()?;
                let qgid = payload.validated_qgid(self.ds.own_domain())?;
                let sender_index = payload.sender.ok_or_missing_field("sender")?.into();

                let (_group_data, group_state) = self
                    .load_group_state_immutable(&qgid, &ear_key)
                    .await
                    .map_err(to_status)?;

                let verifying_key: LeafVerifyingKeyRef = group_state
                    .group()
                    .leaf(sender_index)
                    .ok_or_else(|| Status::invalid_argument("unknown sender"))?
                    .signature_key()
                    .into();

                request.verify(verifying_key).map_err(InvalidSignature)?
            }
            StorageObjectType::DebugLogs => {
                let user_id = payload
                    .user_id
                    .clone()
                    .ok_or_missing_field("user_id")?
                    .try_into()?;
                let client_verifying_key = self
                    .as_connector
                    .client_verifying_key(&user_id)
                    .await
                    .map_err(|error| {
                        error!(%error, "failed to load client verifying key from AS");
                        Status::internal("failed to load client verifying key")
                    })?
                    .ok_or_else(|| Status::not_found("user not found"))?;

                request
                    .verify(&client_verifying_key)
                    .map_err(InvalidSignature)?
            }
        };

        let object_id = payload.object_id.ok_or_missing_field("object_id")?.into();
        let object_type = StorageObjectType::try_from(payload.object_type).unwrap_or_default();

        Ok(self.ds.get_object_url(object_id, object_type).await?)
    }

    async fn targeted_message(
        &self,
        request: Request<SignedRequest<TargetedMessageRequest>>,
    ) -> Result<Response<TargetedMessageResponse>, Status> {
        let request = request.into_inner();

        request
            .inner()
            .signature
            .as_ref()
            .ok_or_missing_field("signature")?;

        let payload = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;

        self.verify_client_version(payload.client_metadata.as_ref())?;

        let sender_index: LeafNodeIndex = payload.sender.ok_or_missing_field("sender")?.into();

        let ear_key = request.inner().ear_key()?;
        let message = request.inner().message()?;
        let TargetedMessageType::ApplicationMessage(req) = payload
            .targeted_message_type
            .as_ref()
            .ok_or_missing_field("message type")?;
        let recipient_index = req.recipient.ok_or_missing_field("recipient")?.into();
        let qgid = message.validated_qgid(self.ds.own_domain())?;

        // No transaction needed as we do not update the group state and
        // application messages are out-of-order tolerant.
        let (_, group_state) = self
            .load_group_state_immutable(&qgid, &ear_key)
            .await
            .map_err(to_status)?;

        // verify signature
        let verifying_key: LeafVerifyingKeyRef = group_state
            .group()
            .leaf(sender_index)
            .ok_or_else(|| Status::invalid_argument("unknown sender"))?
            .signature_key()
            .into();
        let payload: TargetedMessagePayload =
            request.verify(verifying_key).map_err(InvalidSignature)?;

        if let Some(tags) = payload.collision_tags {
            let msg_epoch = message.epoch().as_u64();
            super::collision_tags::check_and_insert(
                &self.ds.db_pool,
                qgid.group_uuid(),
                msg_epoch as i64,
                tags,
            )
            .await?;
        }

        let destination_client = group_state
            .qs_client_ref_by_index(recipient_index)
            .ok_or_else(|| Status::invalid_argument("unknown recipient"))?;

        // Messages from legacy clients won't have this field set. Default to false.
        let suppress_notifications = false;

        let fan_out_message = DsFanOutMessage {
            payload: QsQueueMessagePayload::targeted_message(message.into_serialized_mls_message())
                .map_err(|_| Status::internal("couldn't serialize targeted message"))?
                .into(),
            client_reference: destination_client,
            suppress_notifications: suppress_notifications.into(),
        };

        let timestamp = fan_out_message.payload.timestamp();

        self.qs_connector
            .dispatch(fan_out_message)
            .await
            .map_err(DistributeMessageError::Connector)?;

        Ok(Response::new(TargetedMessageResponse {
            fanout_timestamp: Some(timestamp.into()),
        }))
    }
}

#[derive(Debug, Error)]
enum DistributeMessageError<E> {
    #[error(transparent)]
    Join(JoinError),
    #[error(transparent)]
    Connector(E),
}

impl<E: std::error::Error> From<DistributeMessageError<E>> for Status {
    fn from(error: DistributeMessageError<E>) -> Self {
        error!(%error, "Failed to distribute message");
        Status::internal("failed to distribute message")
    }
}

struct GroupNotFoundError;

impl From<GroupNotFoundError> for Status {
    fn from(_: GroupNotFoundError) -> Self {
        Status::not_found("group not found")
    }
}

struct InvalidSignature(SignatureVerificationError);

impl From<InvalidSignature> for Status {
    fn from(e: InvalidSignature) -> Self {
        error!(error =% e.0, "invalid signature");
        Status::unauthenticated("invalid signature")
    }
}

/// Protobuf containing a qualified group id
pub(super) trait WithQualifiedGroupId {
    fn qgid(&self) -> Result<QualifiedGroupId, Status>;

    fn validated_qgid(&self, own_domain: &Fqdn) -> Result<QualifiedGroupId, Status> {
        let qgid = self.qgid()?;
        if qgid.owning_domain() == own_domain {
            Ok(qgid)
        } else {
            Err(NonMatchingOwnDomain(qgid).into())
        }
    }
}

struct NonMatchingOwnDomain(QualifiedGroupId);

impl From<NonMatchingOwnDomain> for Status {
    fn from(e: NonMatchingOwnDomain) -> Self {
        error!(qgid =% e.0, "group id domain does not match own domain");
        Status::invalid_argument("group id domain does not match own domain")
    }
}

impl WithQualifiedGroupId for AssistedMessageIn {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.group_id()
            .try_into()
            .invalid_tls("group_id")
            .map_err(From::from)
    }
}

impl WithQualifiedGroupId for CreateGroupPayload {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.qgid
            .as_ref()
            .ok_or_missing_field("qgid")?
            .try_ref_into()
            .map_err(From::from)
    }
}

impl WithQualifiedGroupId for WelcomeInfoPayload {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.qgid
            .as_ref()
            .ok_or_missing_field("qgid")?
            .try_ref_into()
            .map_err(From::from)
    }
}

impl WithQualifiedGroupId for UpdateProfileKeyPayload {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.group_id
            .as_ref()
            .ok_or_missing_field("group_id")?
            .try_ref_into()
            .map_err(From::from)
    }
}

impl WithQualifiedGroupId for ProvisionAttachmentPayload {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.group_id
            .as_ref()
            .ok_or_missing_field("group_id")?
            .try_ref_into()
            .map_err(From::from)
    }
}

impl WithQualifiedGroupId for GetAttachmentUrlPayload {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.group_id
            .as_ref()
            .ok_or_missing_field("group_id")?
            .try_ref_into()
            .map_err(From::from)
    }
}

impl WithQualifiedGroupId for GroupSessionData {
    fn qgid(&self) -> Result<QualifiedGroupId, Status> {
        self.qgid
            .as_ref()
            .ok_or_missing_field("qgid")?
            .try_ref_into()
            .map_err(From::from)
    }
}

/// Protobuf containing a group state ear key
pub(super) trait WithGroupStateEarKey {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey>;

    fn ear_key(&self) -> Result<GroupStateEarKey, Status> {
        self.ear_key_proto()
            .ok_or_missing_field("group_state_ear_key")?
            .try_ref_into()
            .map_err(From::from)
    }
}

impl WithGroupStateEarKey for SendMessageRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for CreateGroupPayload {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for DeleteGroupRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for GroupOperationRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for TargetedMessageRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for SelfRemoveRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for ApqSelfRemoveRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for WelcomeInfoPayload {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for ResyncRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for UpdateProfileKeyRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for ProvisionAttachmentPayload {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for GetAttachmentUrlPayload {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for GroupSessionData {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.group_state_ear_key.as_ref()
    }
}

impl WithGroupStateEarKey for ApqGroupOperationRequest {
    fn ear_key_proto(&self) -> Option<&v1::GroupStateEarKey> {
        self.payload.as_ref()?.group_state_ear_key.as_ref()
    }
}

/// Request containing an MLS message
trait WithMessage {
    fn message(&self) -> Result<AssistedMessageIn, Status>;
}

impl WithMessage for SendMessageRequest {
    fn message(&self) -> Result<AssistedMessageIn, Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let message = payload.message.as_ref().ok_or_missing_field("message")?;
        let message = message.try_ref_into().invalid_tls("message")?;
        Ok(message)
    }
}

impl WithMessage for GroupOperationRequest {
    fn message(&self) -> Result<AssistedMessageIn, Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let commit = payload.commit.as_ref().ok_or_missing_field("commit")?;
        let commit = commit.try_ref_into().invalid_tls("commit")?;
        Ok(commit)
    }
}

impl WithMessage for TargetedMessageRequest {
    fn message(&self) -> Result<AssistedMessageIn, Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let TargetedMessageType::ApplicationMessage(req) = payload
            .targeted_message_type
            .as_ref()
            .ok_or_missing_field("message_type")?;
        let message = req.message.as_ref().ok_or_missing_field("request")?;
        let message = message.try_ref_into().invalid_tls("message")?;
        Ok(message)
    }
}

impl WithMessage for DeleteGroupRequest {
    fn message(&self) -> Result<AssistedMessageIn, Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let commit = payload.commit.as_ref().ok_or_missing_field("commit")?;
        let commit = commit.try_ref_into().invalid_tls("commit")?;
        Ok(commit)
    }
}

impl WithMessage for SelfRemoveRequest {
    fn message(&self) -> Result<AssistedMessageIn, Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let remove_proposal = payload
            .remove_proposal
            .as_ref()
            .ok_or_missing_field("remove_proposal")?;
        let remove_proposal = remove_proposal
            .try_ref_into()
            .invalid_tls("remove_proposal")?;
        Ok(remove_proposal)
    }
}

impl WithMessage for ResyncRequest {
    fn message(&self) -> Result<AssistedMessageIn, Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let external_commit = payload
            .external_commit
            .as_ref()
            .ok_or_missing_field("external_commit")?;
        let message = external_commit
            .try_ref_into()
            .invalid_tls("external_commit")?;
        Ok(message)
    }
}

/// Request containing an APQ MLS message
trait WithApqMessage {
    /// Returns the T and PQ message pair for the request.
    fn apq_message(&self) -> Result<(AssistedMessageIn, AssistedMessageIn), Status>;
}

impl WithApqMessage for ApqSelfRemoveRequest {
    fn apq_message(&self) -> Result<(AssistedMessageIn, AssistedMessageIn), Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let proposal = payload
            .remove_proposal
            .as_ref()
            .ok_or_missing_field("remove_proposal")?;
        let t_message = proposal
            .t_message
            .as_ref()
            .ok_or_missing_field("t_message")?;
        let pq_message = proposal
            .pq_message
            .as_ref()
            .ok_or_missing_field("pq_message")?;
        Ok((
            t_message.try_ref_into().invalid_tls("t_message")?,
            pq_message.try_ref_into().invalid_tls("pq_message")?,
        ))
    }
}

impl WithApqMessage for ApqGroupOperationRequest {
    fn apq_message(&self) -> Result<(AssistedMessageIn, AssistedMessageIn), Status> {
        let payload = self.payload.as_ref().ok_or_missing_field("payload")?;
        let commit = payload.commit.as_ref().ok_or_missing_field("commit")?;
        let t_message = commit.t_message.as_ref().ok_or_missing_field("t_message")?;
        let pq_message = commit
            .pq_message
            .as_ref()
            .ok_or_missing_field("pq_message")?;
        Ok((
            t_message.try_ref_into().invalid_tls("t_message")?,
            pq_message.try_ref_into().invalid_tls("pq_message")?,
        ))
    }
}

struct NoWelcomeInfoFound;

impl From<NoWelcomeInfoFound> for Status {
    fn from(_: NoWelcomeInfoFound) -> Self {
        Status::not_found("no welcome info found")
    }
}
