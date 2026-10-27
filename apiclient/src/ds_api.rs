// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Client API for the delivery service (DS)

use std::{collections::HashMap, time::Duration};

use aircommon::{
    LibraryError,
    credentials::keys::{ClientKeyType, UserSigningKey},
    crypto::{
        aead::keys::{EncryptedUserProfileKey, GroupStateEarKey},
        signatures::{private_keys::SigningKey, signable::Signable},
    },
    identifiers::{QsReference, QualifiedGroupId, RemoteAttachmentId, UserId},
    messages::{
        client_ds::UserProfileKeyUpdateParams,
        client_ds_out::{
            ApqGroupOperationParamsOut, CreateGroupParamsOut, DeleteGroupParamsOut,
            EpochSnapshotIn, ExternalCommitInfoIn, GroupOperationParamsOut, PqEpochSnapshotIn,
            PqExternalCommitInfoIn, SelfRemoveParamsOut, SendMessageCollisionTag,
            SendMessageParamsOut, TargetedMessageParamsOut, WelcomeInfoIn,
        },
        client_references::SealedClientReferencesSignature,
    },
    time::TimeStamp,
};
pub use airprotos::delivery_service::v1::ProvisionAttachmentResponse;
use airprotos::{
    common::v1::{
        AttachmentTooLargeDetail, DeviceLimitReachedDetail,
        QualifiedGroupId as QualifiedGroupIdProto, StatusDetails, StatusDetailsCode,
        status_details::{self, Detail},
    },
    convert::{RefInto, TryRefInto},
    delivery_service::v1::{
        AddUsersInfo, ApqAddUsersInfo, ApqAssistedMlsMessage, ApqDeleteGroupPayload,
        ApqGroupOperationPayload, ApqJoinConnectionGroupRequest, ApqResyncPayload,
        ApqSelfRemovePayload, ConnectionGroupInfoRequest, CreateApqGroupPayload,
        CreateGroupPayload, CreateGroupStreamBeginPayload, CreateGroupStreamRequest,
        CreateGroupStreamResponse, DeleteGroupPayload, EpochSnapshotRequest,
        ExternalCommitInfoRequest, GetAttachmentUrlPayload, GroupInfo, GroupOperationPayload,
        GroupSessionData, IndexedEncryptedUserProfileKey, JoinConnectionGroupRequest, MlsMessage,
        ProvisionAttachmentPayload, RatchetTree, RequestGroupIdRequest, ResyncPayload,
        SelfRemovePayload, SendMessageCollisionTags, SendMessagePayload, StorageObjectType,
        TargetedMessagePayload, UpdateProfileKeyPayload, WelcomeInfoPayload,
        create_group_stream_request, create_group_stream_response,
    },
    validation::MissingFieldExt,
};
use apqmls::commit_builder::ApqCommitMessageBundle;
use mimi_room_policy::VerifiedRoomState;
use mls_assist::{
    messages::AssistedMessageOut,
    openmls::prelude::{
        GroupEpoch, GroupId, LeafNodeIndex, MlsMessageIn, MlsMessageOut,
        tls_codec::DeserializeBytes as _,
    },
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Code, Streaming};
use tracing::error;

use crate::{ApiClient, ClassifyRequestError, RequestErrorKind, classify_status};

/// How long we wait for the DS to answer a send request.
///
/// The deadline is per request, because the channel is shared with the
/// long-lived streaming calls. We enforce it ourselves rather than through
/// `Request::set_timeout`, whose expiry surfaces as an ambiguous
/// `Code::Cancelled`, and treat it as a network error: the DS may have fanned
/// the message out before we stopped waiting, so a retry can duplicate a
/// delivery, which the recipient deduplicates.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

/// Errors that can occur when sending requests to the DS.
#[derive(Debug, thiserror::Error)]
pub enum DsRequestError {
    #[error("Library Error")]
    LibraryError,
    #[error(transparent)]
    Tonic(#[from] tonic::Status),
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error("We received an unexpected response type.")]
    UnexpectedResponse,
    #[error("The DS did not answer within {0:?}")]
    Timeout(Duration),
}

impl From<LibraryError> for DsRequestError {
    fn from(_: LibraryError) -> Self {
        Self::LibraryError
    }
}

impl ClassifyRequestError for DsRequestError {
    fn kind(&self) -> RequestErrorKind {
        match self {
            Self::Tonic(status) => classify_status(status),
            // We stopped waiting, the DS may still have received the request
            Self::Timeout(_) => RequestErrorKind::Network,
            Self::LibraryError | Self::Tls(_) | Self::UnexpectedResponse => {
                RequestErrorKind::Rejected
            }
        }
    }
}

impl DsRequestError {
    pub fn get_attachment_too_large(&self) -> Option<AttachmentTooLargeDetail> {
        if let Self::Tonic(status) = self
            && status.code() == Code::InvalidArgument
        {
            let details = StatusDetails::from_status(status)?;
            if let Detail::AttachmentTooLarge(attachment_too_large_detail) = details.detail? {
                Some(attachment_too_large_detail)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn process_tag_collisions(
        &self,
        tags: &[SendMessageCollisionTag],
    ) -> Vec<SendMessageCollisionTag> {
        if let Self::Tonic(status) = self
            && status.code() == tonic::Code::AlreadyExists
            && let Some(details) = StatusDetails::from_status(status)
            && let StatusDetailsCode::GenerationCollision = details.code()
            && let Some(status_details::Detail::GenerationCollision(detail)) = details.detail
        {
            tags.iter()
                .cloned()
                .filter(|tag| detail.tags.contains(&tag.value()))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Whether the DS did not find the group or other resource. True for a
    /// closed group as well, see [`Self::is_group_closed`].
    pub fn is_not_found(&self) -> bool {
        if let Self::Tonic(status) = self {
            matches!(status.code(), Code::NotFound)
        } else {
            false
        }
    }

    /// Whether the DS rejected the request because the group's context carries
    /// a tombstone.
    pub fn is_group_closed(&self) -> bool {
        matches!(self.kind(), RequestErrorKind::GroupClosed)
    }

    pub fn device_limit_reached(&self) -> Option<DeviceLimitReachedDetail> {
        if let Self::Tonic(status) = self
            && status.code() == Code::ResourceExhausted
            && let details = StatusDetails::from_status(status)?
            && let Detail::DeviceLimitReached(detail) = details.detail?
        {
            Some(detail)
        } else {
            None
        }
    }
}

/// The group ids and group profile upload a group creation stream handed out.
#[derive(Debug)]
pub struct DsGroupReservation {
    pub group_id: GroupId,
    pub pq_group_id: GroupId,
    /// Set when a group profile upload was requested and the server has
    /// storage configured.
    pub group_profile_provisioning: Option<ProvisionAttachmentResponse>,
}

/// The open create step of a group creation stream.
///
/// Dropping it closes the stream without creating the group.
#[derive(Debug)]
pub struct DsGroupCreator {
    api_client: ApiClient,
    requests: mpsc::Sender<CreateGroupStreamRequest>,
    responses: Streaming<CreateGroupStreamResponse>,
}

impl DsGroupCreator {
    /// Creates the APQ group. `params` must use both ids of the reservation.
    pub async fn create(
        mut self,
        params: CreateGroupParamsOut,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        let payload = match self
            .api_client
            .create_group_payload(params, group_state_ear_key)?
        {
            CreateGroupPayloads::Apq(payload) => payload,
            CreateGroupPayloads::Classical(_) => {
                error!("group creation streams only create APQ groups");
                return Err(DsRequestError::LibraryError);
            }
        };
        let step = create_group_stream_request::Step::Create(payload);
        if self
            .requests
            .send(CreateGroupStreamRequest { step: Some(step) })
            .await
            .is_err()
        {
            // The server closed the stream, and its last response carries the
            // reason.
            next_create_group_stream_step(&mut self.responses).await?;
            error!("group creation stream closed before the create step");
            return Err(DsRequestError::UnexpectedResponse);
        }
        match next_create_group_stream_step(&mut self.responses).await? {
            create_group_stream_response::Step::Created(_) => Ok(()),
            create_group_stream_response::Step::Reserved(_) => {
                error!("protocol violation: expected created step");
                Err(DsRequestError::UnexpectedResponse)
            }
        }
    }
}

enum CreateGroupPayloads {
    Classical(CreateGroupPayload),
    Apq(CreateApqGroupPayload),
}

fn group_profile_size_to_i64(size: Option<usize>) -> Result<Option<i64>, DsRequestError> {
    size.map(i64::try_from)
        .transpose()
        .inspect_err(|_| {
            error!("Failed to convert group profile size to i64");
        })
        .map_err(|_| DsRequestError::LibraryError)
}

/// Parses a group id the server must have sent in field `name`.
fn response_qgid(
    qgid: Option<QualifiedGroupIdProto>,
    name: &'static str,
) -> Result<QualifiedGroupId, DsRequestError> {
    qgid.ok_or_missing_field(name)
        .map_err(|error| {
            error!(%error, "unexpected response");
            DsRequestError::UnexpectedResponse
        })?
        .try_ref_into()
        .map_err(|error| {
            error!(%error, "unexpected response");
            DsRequestError::UnexpectedResponse
        })
}

async fn next_create_group_stream_step(
    responses: &mut Streaming<CreateGroupStreamResponse>,
) -> Result<create_group_stream_response::Step, DsRequestError> {
    responses
        .message()
        .await?
        .and_then(|response| response.step)
        .ok_or_else(|| {
            error!("protocol violation: missing group creation stream step");
            DsRequestError::UnexpectedResponse
        })
}

pub enum DsAttachmentTarget<'a> {
    Group {
        group_state_ear_key: &'a GroupStateEarKey,
        group_id: &'a GroupId,
        sender_index: LeafNodeIndex,
    },
    User {
        user_id: &'a UserId,
    },
}

impl ApiClient {
    /// Creates a new group on the DS.
    pub async fn ds_create_group(
        &self,
        params: CreateGroupParamsOut,
        signing_key: &UserSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        match self.create_group_payload(params, group_state_ear_key)? {
            CreateGroupPayloads::Classical(payload) => {
                let request = payload.sign(signing_key)?;
                self.ds_grpc_client().create_group(request).await?;
            }
            CreateGroupPayloads::Apq(payload) => {
                let request = payload.sign(signing_key)?;
                self.ds_grpc_client().create_apq_group(request).await?;
            }
        }
        Ok(())
    }

    /// Opens a group creation stream for an APQ group: authenticates as the
    /// user of `signing_key`, and reserves fresh T and PQ group ids and
    /// optionally the upload of a group profile of the given size.
    ///
    /// Each call consumes one unit of the user's group creation allowance,
    /// whether or not the group is created later. The group must be created
    /// with [`DsGroupCreator::create`] before the server's deadline.
    pub async fn ds_begin_group_creation(
        &self,
        signing_key: &UserSigningKey,
        group_profile_size: Option<usize>,
    ) -> Result<(DsGroupReservation, DsGroupCreator), DsRequestError> {
        let group_profile_size = group_profile_size_to_i64(group_profile_size)?;
        let payload = CreateGroupStreamBeginPayload {
            client_metadata: Some(self.metadata().clone()),
            user_id: Some(signing_key.credential().user_id().clone().into()),
            group_profile_size,
        };
        let begin = payload.sign(signing_key)?;

        let (requests, requests_rx) = mpsc::channel(1);
        requests
            .send(CreateGroupStreamRequest {
                step: Some(create_group_stream_request::Step::Begin(begin)),
            })
            .await
            .map_err(|_| DsRequestError::LibraryError)?;
        let mut responses = self
            .ds_grpc_client()
            .create_group_stream(ReceiverStream::new(requests_rx))
            .await?
            .into_inner();

        let reserved = match next_create_group_stream_step(&mut responses).await? {
            create_group_stream_response::Step::Reserved(reserved) => reserved,
            create_group_stream_response::Step::Created(_) => {
                error!("protocol violation: expected reserved step");
                return Err(DsRequestError::UnexpectedResponse);
            }
        };
        let group_id = response_qgid(reserved.group_id, "group_id")?;
        let pq_group_id = response_qgid(reserved.pq_group_id, "pq_group_id")?;

        let reservation = DsGroupReservation {
            group_id: group_id.into(),
            pq_group_id: pq_group_id.into(),
            group_profile_provisioning: reserved.group_profile_provisioning,
        };
        let creator = DsGroupCreator {
            api_client: self.clone(),
            requests,
            responses,
        };
        Ok((reservation, creator))
    }

    fn create_group_payload(
        &self,
        params: CreateGroupParamsOut,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<CreateGroupPayloads, DsRequestError> {
        let CreateGroupParamsOut {
            group_id,
            group_info,
            ratchet_tree,
            encrypted_user_profile_key,
            creator_client_reference,
            room_state,
            pq,
            creator_user_credential,
            group_bootstrap,
        } = params;

        let qgid: QualifiedGroupId = group_id.try_into()?;
        let creator_user_credential = creator_user_credential
            .as_ref()
            .map(TryRefInto::try_ref_into)
            .transpose()?;

        if let Some(pq) = pq {
            let pq_qgid: QualifiedGroupId = pq.group_id.try_into()?;

            let t_group_data = GroupSessionData {
                qgid: Some(qgid.ref_into()),
                group_state_ear_key: Some(group_state_ear_key.ref_into()),
                ratchet_tree: Some(ratchet_tree.try_ref_into()?),
                group_info: Some(group_info.try_ref_into()?),
            };
            let pq_group_data = GroupSessionData {
                qgid: Some(pq_qgid.ref_into()),
                group_state_ear_key: Some(group_state_ear_key.ref_into()),
                ratchet_tree: Some(pq.ratchet_tree.try_ref_into()?),
                group_info: Some(pq.group_info.try_ref_into()?),
            };

            let payload = CreateApqGroupPayload {
                client_metadata: Some(self.metadata().clone()),
                encrypted_user_profile_key: Some(encrypted_user_profile_key.into()),
                creator_client_reference: Some(creator_client_reference.into()),
                room_state: Some(room_state.unverified().try_ref_into()?),
                t_group_data: Some(t_group_data),
                pq_group_data: Some(pq_group_data),
                creator_user_credential,
                group_bootstrap,
            };
            Ok(CreateGroupPayloads::Apq(payload))
        } else {
            let payload = CreateGroupPayload {
                client_metadata: Some(self.metadata().clone()),
                qgid: Some(qgid.ref_into()),
                group_state_ear_key: Some(group_state_ear_key.ref_into()),
                ratchet_tree: Some(ratchet_tree.try_ref_into()?),
                encrypted_user_profile_key: Some(encrypted_user_profile_key.into()),
                creator_client_reference: Some(creator_client_reference.into()),
                group_info: Some(group_info.try_ref_into()?),
                room_state: Some(room_state.unverified().try_ref_into()?),
                creator_user_credential,
                group_bootstrap,
            };
            Ok(CreateGroupPayloads::Classical(payload))
        }
    }

    /// Performs a group operation.
    pub async fn ds_group_operation(
        &self,
        payload: GroupOperationParamsOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
        qs_client_reference: QsReference,
        encrypted_user_profile_key: EncryptedUserProfileKey,
        client_references_signature: Option<SealedClientReferencesSignature>,
    ) -> Result<TimeStamp, DsRequestError> {
        let add_users_info = payload
            .add_users_info_option
            .map(|add_user_infos| {
                Ok::<_, DsRequestError>(AddUsersInfo {
                    welcome: Some(add_user_infos.welcome.try_ref_into()?),
                    encrypted_welcome_attribution_info: add_user_infos
                        .encrypted_welcome_attribution_infos
                        .into_iter()
                        .map(From::from)
                        .collect(),
                })
            })
            .transpose()?;
        let payload = GroupOperationPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(payload.commit.try_ref_into()?),
            add_users_info,
            qs_client_reference: Some(qs_client_reference.into()),
            encrypted_user_profile_key: Some(encrypted_user_profile_key.into()),
            client_references_signature: client_references_signature.map(From::from),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .group_operation(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Performs a APQ group operation.
    pub async fn ds_apq_group_operation(
        &self,
        params: ApqGroupOperationParamsOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
        qs_client_reference: QsReference,
        encrypted_user_profile_key: EncryptedUserProfileKey,
        client_references_signature: Option<SealedClientReferencesSignature>,
    ) -> Result<TimeStamp, DsRequestError> {
        let ApqGroupOperationParamsOut {
            bundle:
                ApqCommitMessageBundle {
                    commit,
                    welcome,
                    group_info,
                },
            encrypted_welcome_attribution_infos,
        } = params;
        let add_users_info = welcome
            .map(|welcome| -> Result<_, tls_codec::Error> {
                let welcome = apqmls::messages::ApqMlsMessageOut::from(welcome).try_ref_into()?;
                Ok(ApqAddUsersInfo {
                    welcome: Some(welcome),
                    encrypted_welcome_attribution_info: encrypted_welcome_attribution_infos
                        .into_iter()
                        .map(From::from)
                        .collect(),
                })
            })
            .transpose()?;
        let (t_commit, pq_commit) = commit.split();
        let (t_group_info, pq_group_info) = group_info
            .map(apqmls::messages::ApqMlsMessageOut::from)
            .map(|msg| msg.split())
            .unzip();
        let commit = ApqAssistedMlsMessage {
            t_message: Some(AssistedMessageOut::new(t_commit, t_group_info).try_ref_into()?),
            pq_message: Some(AssistedMessageOut::new(pq_commit, pq_group_info).try_ref_into()?),
        };
        let payload = ApqGroupOperationPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(commit),
            add_users_info,
            qs_client_reference: Some(qs_client_reference.into()),
            encrypted_user_profile_key: Some(encrypted_user_profile_key.into()),
            client_references_signature: client_references_signature.map(From::from),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .apq_group_operation(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Get welcome information for a group.
    pub async fn ds_welcome_info(
        &self,
        group_id: GroupId,
        epoch: GroupEpoch,
        group_state_ear_key: &GroupStateEarKey,
        signing_key: &SigningKey<ClientKeyType>,
    ) -> Result<WelcomeInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let payload = WelcomeInfoPayload {
            client_metadata: Some(self.metadata().clone()),
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            sender: Some(signing_key.verifying_key().clone().into()),
            epoch: Some(epoch.into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .welcome_info(request)
            .await?
            .into_inner();
        let (encrypted_user_profile_keys, indexed_encrypted_user_profile_keys) =
            extract_encrypted_user_profile_keys(
                response.encrypted_user_profile_keys,
                response.indexed_encrypted_user_profile_keys,
            )?;
        Ok(WelcomeInfoIn {
            ratchet_tree: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            encrypted_user_profile_keys,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
            indexed_encrypted_user_profile_keys,
        })
    }

    /// Get external commit information for a group.
    ///
    /// `pq_group_id` must be set iff `group_id` refers to an APQ group, in which case the PQ
    /// leg's group info, ratchet tree and parked proposals are fetched atomically alongside the
    /// T leg's.
    pub async fn ds_external_commit_info(
        &self,
        group_id: GroupId,
        pq_group_id: Option<GroupId>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let pq_qgid: Option<QualifiedGroupId> = pq_group_id.map(|id| id.try_into()).transpose()?;
        let request = ExternalCommitInfoRequest {
            client_metadata: Some(self.metadata().clone()),
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            pq_qgid: pq_qgid.as_ref().map(RefInto::ref_into),
        };
        let response = self
            .ds_grpc_client()
            .external_commit_info(request)
            .await?
            .into_inner();

        let (encrypted_user_profile_keys, indexed_encrypted_user_profile_keys) =
            extract_encrypted_user_profile_keys(
                response.encrypted_user_profile_keys,
                response.indexed_encrypted_user_profile_keys,
            )?;

        let pq = parse_pq_external_commit_info(
            response.pq_group_info,
            response.pq_ratchet_tree,
            response.pq_proposals,
        )?;

        Ok(ExternalCommitInfoIn {
            verifiable_group_info: response
                .group_info
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            ratchet_tree_in: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            encrypted_user_profile_keys,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
            proposals: response.proposals.into_iter().map(|m| m.tls).collect(),
            indexed_encrypted_user_profile_keys,
            pq,
        })
    }

    /// Get the group state the DS served at `epoch`.
    ///
    /// `group_id` is the T leg's group id, also for APQ groups: the snapshot of
    /// an APQ group covers both legs.
    pub async fn ds_epoch_snapshot(
        &self,
        group_id: GroupId,
        epoch: GroupEpoch,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<EpochSnapshotIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let request = EpochSnapshotRequest {
            client_metadata: Some(self.metadata().clone()),
            qgid: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            epoch: epoch.as_u64(),
        };
        let response = self
            .ds_grpc_client()
            .epoch_snapshot(request)
            .await?
            .into_inner();

        let pq = match (response.pq_group_info, response.pq_ratchet_tree) {
            (Some(pq_group_info), Some(pq_ratchet_tree)) => Some(PqEpochSnapshotIn {
                verifiable_group_info: pq_group_info.try_ref_into()?,
                ratchet_tree_in: pq_ratchet_tree.try_ref_into()?,
                join_commit: response
                    .pq_join_commit
                    .map(|bytes| MlsMessageIn::tls_deserialize_exact_bytes(&bytes))
                    .transpose()
                    .map_err(|_| DsRequestError::UnexpectedResponse)?,
            }),
            (None, None) if response.pq_join_commit.is_none() => None,
            _ => return Err(DsRequestError::UnexpectedResponse),
        };

        Ok(EpochSnapshotIn {
            verifiable_group_info: response
                .group_info
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            ratchet_tree_in: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
            pq,
            join_commit: response
                .join_commit
                .map(|bytes| MlsMessageIn::tls_deserialize_exact_bytes(&bytes))
                .transpose()
                .map_err(|_| DsRequestError::UnexpectedResponse)?,
        })
    }

    /// Get external commit information for a connection group.
    pub async fn ds_connection_group_info(
        &self,
        group_id: GroupId,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<ExternalCommitInfoIn, DsRequestError> {
        let qgid: QualifiedGroupId = group_id.try_into()?;
        let request = ConnectionGroupInfoRequest {
            client_metadata: Some(self.metadata().clone()),
            group_id: Some(qgid.ref_into()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
        };
        let response = self
            .ds_grpc_client()
            .connection_group_info(request)
            .await?
            .into_inner();
        let (encrypted_user_profile_keys, indexed_encrypted_user_profile_keys) =
            extract_encrypted_user_profile_keys(
                response.encrypted_user_profile_keys,
                response.indexed_encrypted_user_profile_keys,
            )?;
        let pq = parse_pq_external_commit_info(
            response.pq_group_info,
            response.pq_ratchet_tree,
            response.pq_proposals,
        )?;
        Ok(ExternalCommitInfoIn {
            verifiable_group_info: response
                .group_info
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            ratchet_tree_in: response
                .ratchet_tree
                .ok_or(DsRequestError::UnexpectedResponse)?
                .try_ref_into()?,
            encrypted_user_profile_keys,
            room_state: VerifiedRoomState::verify(
                response
                    .room_state
                    .ok_or(DsRequestError::UnexpectedResponse)?
                    .try_ref_into()?,
            )
            .map_err(|_| DsRequestError::UnexpectedResponse)?,
            proposals: response.proposals.into_iter().map(|m| m.tls).collect(),
            indexed_encrypted_user_profile_keys,
            pq,
        })
    }

    /// Join the connection group with a new client.
    pub async fn ds_join_connection_group(
        &self,
        commit: MlsMessageOut,
        group_info: MlsMessageOut,
        qs_client_reference: QsReference,
        group_state_ear_key: &GroupStateEarKey,
        // A `GroupBootstrapBlob` (CBOR, defined in `airprotos`) encrypted for
        // the joiner's sibling clients.
        group_bootstrap: Option<Vec<u8>>,
    ) -> Result<TimeStamp, DsRequestError> {
        let external_commit = AssistedMessageOut::new(commit, Some(group_info));
        let request = JoinConnectionGroupRequest {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit.try_ref_into()?),
            qs_client_reference: Some(qs_client_reference.into()),
            group_bootstrap,
        };
        let response = self
            .ds_grpc_client()
            .join_connection_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Join an APQ connection group with a new client.
    pub async fn ds_apq_join_connection_group(
        &self,
        external_commit_bundle: ApqCommitMessageBundle,
        qs_client_reference: QsReference,
        group_state_ear_key: &GroupStateEarKey,
        // A `GroupBootstrapBlob` (CBOR, defined in `airprotos`) encrypted for
        // the joiner's sibling clients.
        group_bootstrap: Option<Vec<u8>>,
    ) -> Result<TimeStamp, DsRequestError> {
        let external_commit = apq_external_commit_message(external_commit_bundle)?;
        let request = ApqJoinConnectionGroupRequest {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit),
            qs_client_reference: Some(qs_client_reference.into()),
            group_bootstrap,
        };
        let response = self
            .ds_grpc_client()
            .apq_join_connection_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Resync a client to rejoin a group.
    pub async fn ds_resync(
        &self,
        commit: MlsMessageOut,
        group_info: MlsMessageOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
        own_leaf_index: LeafNodeIndex,
    ) -> Result<TimeStamp, DsRequestError> {
        let external_commit = AssistedMessageOut::new(commit, Some(group_info));
        let payload = ResyncPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit.try_ref_into()?),
            sender: Some(own_leaf_index.into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self.ds_grpc_client().resync(request).await?.into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Resync a client to rejoin an APQ group.
    pub async fn ds_apq_resync(
        &self,
        external_commit_bundle: ApqCommitMessageBundle,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
        own_leaf_index: LeafNodeIndex,
    ) -> Result<TimeStamp, DsRequestError> {
        let external_commit = apq_external_commit_message(external_commit_bundle)?;
        let payload = ApqResyncPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            external_commit: Some(external_commit),
            sender: Some(own_leaf_index.into()),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .apq_resync(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Leave the given group with this client.
    ///
    /// Dispatches to the APQ or non-APQ self-remove RPC depending on whether a PQ remove proposal
    /// is present.
    pub async fn ds_self_remove(
        &self,
        params: SelfRemoveParamsOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        if let Some(pq_remove_proposal) = params.pq_remove_proposal {
            return self
                .ds_apq_self_remove(
                    params.t_remove_proposal,
                    pq_remove_proposal,
                    signing_key,
                    group_state_ear_key,
                )
                .await;
        }
        let payload = SelfRemovePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            remove_proposal: Some(params.t_remove_proposal.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .self_remove(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Same as [`Self::ds_self_remove`], but for APQ groups.
    async fn ds_apq_self_remove(
        &self,
        t_remove_proposal: AssistedMessageOut,
        pq_remove_proposal: AssistedMessageOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let remove_proposal = ApqAssistedMlsMessage {
            t_message: Some(t_remove_proposal.try_ref_into()?),
            pq_message: Some(pq_remove_proposal.try_ref_into()?),
        };
        let payload = ApqSelfRemovePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            remove_proposal: Some(remove_proposal),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .apq_self_remove(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Send a message to the given group.
    pub async fn ds_send_message(
        &self,
        params: SendMessageParamsOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = SendMessagePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            message: Some(params.message.try_ref_into()?),
            sender: Some(params.sender.into()),
            suppress_notifications: Some(params.suppress_notifications),
            collision_tags: if params.collision_tags.is_empty() {
                None
            } else {
                Some(SendMessageCollisionTags {
                    tags: params
                        .collision_tags
                        .into_iter()
                        .map(|t| t.value())
                        .collect(),
                })
            },
        };
        let request = payload.sign(signing_key)?;
        let mut ds_client = self.ds_grpc_client();
        let send = ds_client.send_message(request);
        let Ok(result) = tokio::time::timeout(SEND_TIMEOUT, send).await else {
            return Err(DsRequestError::Timeout(SEND_TIMEOUT));
        };
        let response = result?.into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Send a message to the recipient within the given group.
    pub async fn ds_targeted_message(
        &self,
        params: TargetedMessageParamsOut,
        signing_key: &UserSigningKey,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = TargetedMessagePayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            sender: Some(params.sender.into()),
            collision_tags: if params.collision_tags.is_empty() {
                None
            } else {
                Some(SendMessageCollisionTags {
                    tags: params
                        .collision_tags
                        .into_iter()
                        .map(|t| t.value())
                        .collect(),
                })
            },
            targeted_message_type: Some(params.message_type.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .targeted_message(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Delete the given group.
    pub async fn ds_delete_group(
        &self,
        params: DeleteGroupParamsOut,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let payload = DeleteGroupPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(params.commit.try_ref_into()?),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .delete_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Delete the given APQ group
    pub async fn ds_apq_delete_group(
        &self,
        delete_bundle: ApqCommitMessageBundle,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<TimeStamp, DsRequestError> {
        let ApqCommitMessageBundle {
            commit,
            welcome,
            group_info,
        } = delete_bundle;
        if welcome.is_some() {
            error!("Unexpected welcome message in APQ delete group request");
            return Err(DsRequestError::LibraryError);
        }
        let (t_commit, pq_commit) = commit.split();
        let (t_group_info, pq_group_info) = group_info
            .map(apqmls::messages::ApqMlsMessageOut::from)
            .map(|msg| msg.split())
            .unzip();
        let commit = ApqAssistedMlsMessage {
            t_message: Some(AssistedMessageOut::new(t_commit, t_group_info).try_ref_into()?),
            pq_message: Some(AssistedMessageOut::new(pq_commit, pq_group_info).try_ref_into()?),
        };
        let payload = ApqDeleteGroupPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            commit: Some(commit),
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .apq_delete_group(request)
            .await?
            .into_inner();
        Ok(response
            .fanout_timestamp
            .ok_or(DsRequestError::UnexpectedResponse)?
            .into())
    }

    /// Update the user's user profile key
    pub async fn ds_user_profile_key_update(
        &self,
        params: UserProfileKeyUpdateParams,
        signing_key: &SigningKey<ClientKeyType>,
        group_state_ear_key: &GroupStateEarKey,
    ) -> Result<(), DsRequestError> {
        let qgid: QualifiedGroupId = params.group_id.try_into()?;
        let payload = UpdateProfileKeyPayload {
            client_metadata: Some(self.metadata().clone()),
            group_state_ear_key: Some(group_state_ear_key.ref_into()),
            group_id: Some(qgid.ref_into()),
            sender: Some(params.sender_index.into()),
            encrypted_user_profile_key: Some(params.user_profile_key.into()),
        };
        let request = payload.sign(signing_key)?;
        self.ds_grpc_client().update_profile_key(request).await?;
        Ok(())
    }

    /// Request a group ID
    ///
    /// Returns a new group ID. A group profile provisioning response is returned if
    /// `provision_group_profile_size` is set.
    pub async fn ds_request_group_id(
        &self,
        provision_group_profile_size: Option<usize>,
        request_pq_group_id: bool,
    ) -> Result<
        (
            GroupId,
            Option<GroupId>,
            Option<ProvisionAttachmentResponse>,
        ),
        DsRequestError,
    > {
        let group_profile_size = group_profile_size_to_i64(provision_group_profile_size)?;
        let response = self
            .ds_grpc_client()
            .request_group_id(RequestGroupIdRequest {
                client_metadata: Some(self.metadata().clone()),
                group_profile_size,
                request_pq_group_id,
            })
            .await?
            .into_inner();
        let qgid = response_qgid(response.group_id, "group_id")?;
        let pq_qgid = if request_pq_group_id {
            Some(response_qgid(response.pq_group_id, "pq_group_id")?)
        } else {
            None
        };
        Ok((
            qgid.into(),
            pq_qgid.map(Into::into),
            response.group_profile_provisioning,
        ))
    }

    /// Provision an attachment for a group.
    ///
    /// The result is used to upload the attachment to the server.
    pub async fn ds_provision_attachment(
        &self,
        signing_key: &SigningKey<ClientKeyType>,
        target: DsAttachmentTarget<'_>,
        content_length: i64,
        object_type: StorageObjectType,
    ) -> Result<ProvisionAttachmentResponse, DsRequestError> {
        let payload = match target {
            DsAttachmentTarget::Group {
                group_state_ear_key,
                group_id,
                sender_index,
            } => {
                let qgid: QualifiedGroupId = group_id.try_into()?;
                ProvisionAttachmentPayload {
                    client_metadata: Some(self.metadata().clone()),
                    group_state_ear_key: Some(group_state_ear_key.ref_into()),
                    group_id: Some(qgid.ref_into()),
                    sender: Some(sender_index.into()),
                    user_id: None,
                    use_post_policy: true,
                    content_length,
                    object_type: object_type.into(),
                }
            }
            DsAttachmentTarget::User { user_id } => ProvisionAttachmentPayload {
                client_metadata: Some(self.metadata().clone()),
                group_state_ear_key: None,
                group_id: None,
                sender: None,
                user_id: Some(user_id.clone().into()),
                use_post_policy: true,
                content_length,
                object_type: object_type.into(),
            },
        };

        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .provision_attachment(request)
            .await?
            .into_inner();
        Ok(response)
    }

    /// Get the download URL for an attachment.
    pub async fn ds_get_attachment_url(
        &self,
        object_type: StorageObjectType,
        signing_key: &SigningKey<ClientKeyType>,
        target: DsAttachmentTarget<'_>,
        remote_attachment_id: RemoteAttachmentId,
    ) -> Result<String, DsRequestError> {
        let payload = match target {
            DsAttachmentTarget::Group {
                group_state_ear_key,
                group_id,
                sender_index,
            } => {
                let qgid: QualifiedGroupId = group_id.try_into()?;
                GetAttachmentUrlPayload {
                    client_metadata: Some(self.metadata().clone()),
                    group_state_ear_key: Some(group_state_ear_key.ref_into()),
                    group_id: Some(qgid.ref_into()),
                    sender: Some(sender_index.into()),
                    user_id: None,
                    object_id: Some(remote_attachment_id.uuid().into()),
                    object_type: object_type.into(),
                }
            }
            DsAttachmentTarget::User { user_id } => GetAttachmentUrlPayload {
                client_metadata: Some(self.metadata().clone()),
                group_state_ear_key: None,
                group_id: None,
                sender: None,
                user_id: Some(user_id.clone().into()),
                object_id: Some(remote_attachment_id.uuid().into()),
                object_type: object_type.into(),
            },
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .ds_grpc_client()
            .get_attachment_url(request)
            .await?
            .into_inner();
        Ok(response.download_url)
    }
}

fn apq_external_commit_message(
    external_commit_bundle: ApqCommitMessageBundle,
) -> Result<ApqAssistedMlsMessage, DsRequestError> {
    let ApqCommitMessageBundle {
        commit,
        welcome,
        group_info,
    } = external_commit_bundle;
    if welcome.is_some() {
        error!("Unexpected welcome message");
        return Err(DsRequestError::LibraryError);
    }
    let (t_commit, pq_commit) = commit.split();
    let (t_group_info, pq_group_info) = group_info
        .map(apqmls::messages::ApqMlsMessageOut::from)
        .map(|msg| msg.split())
        .unzip();
    let external_commit = ApqAssistedMlsMessage {
        t_message: Some(AssistedMessageOut::new(t_commit, t_group_info).try_ref_into()?),
        pq_message: Some(AssistedMessageOut::new(pq_commit, pq_group_info).try_ref_into()?),
    };
    Ok(external_commit)
}

fn parse_pq_external_commit_info(
    pq_group_info: Option<GroupInfo>,
    pq_ratchet_tree: Option<RatchetTree>,
    pq_proposals: Vec<MlsMessage>,
) -> Result<Option<PqExternalCommitInfoIn>, DsRequestError> {
    match (pq_group_info, pq_ratchet_tree) {
        (Some(pq_group_info), Some(pq_ratchet_tree)) => Ok(Some(PqExternalCommitInfoIn {
            group_info: pq_group_info.try_ref_into()?,
            ratchet_tree: pq_ratchet_tree.try_ref_into()?,
            proposals: pq_proposals.into_iter().map(|m| m.tls).collect(),
        })),
        (None, None) => Ok(None),
        _ => Err(DsRequestError::UnexpectedResponse),
    }
}

fn extract_encrypted_user_profile_keys(
    encrypted_user_profile_keys: Vec<airprotos::delivery_service::v1::EncryptedUserProfileKey>,
    indexed_encrypted_user_profile_keys: Vec<IndexedEncryptedUserProfileKey>,
) -> Result<
    (
        Vec<EncryptedUserProfileKey>,
        HashMap<LeafNodeIndex, EncryptedUserProfileKey>,
    ),
    DsRequestError,
> {
    let (encrypted_user_profile_keys, indexed_encrypted_user_profile_keys) =
        if !indexed_encrypted_user_profile_keys.is_empty() {
            let indexed_encrypted_user_profile_keys = indexed_encrypted_user_profile_keys
                .into_iter()
                .filter_map(
                    |IndexedEncryptedUserProfileKey {
                         leaf_index,
                         encrypted_user_profile_key,
                     }| {
                        Some((
                            LeafNodeIndex::new(leaf_index),
                            encrypted_user_profile_key?.try_into().ok()?,
                        ))
                    },
                )
                .collect();
            (Default::default(), indexed_encrypted_user_profile_keys)
        } else {
            let encrypted_user_profile_keys = encrypted_user_profile_keys
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| DsRequestError::UnexpectedResponse)?;
            (encrypted_user_profile_keys, Default::default())
        };
    Ok((
        encrypted_user_profile_keys,
        indexed_encrypted_user_profile_keys,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{assert_matches, io};

    use airprotos::common::v1::GroupClosedDetail;

    #[test]
    fn timeout_is_classified_as_a_network_error() {
        let error = DsRequestError::Timeout(SEND_TIMEOUT);
        assert_matches!(
            error.kind(),
            RequestErrorKind::Network,
            "a send we stopped waiting for may still have reached the DS"
        );
        assert!(!error.is_not_found());
    }

    #[test]
    fn group_closed_is_a_not_found_with_details() {
        let status = StatusDetails {
            code: StatusDetailsCode::GroupClosed.into(),
            detail: Some(Detail::GroupClosed(GroupClosedDetail {})),
        }
        .to_status(Code::NotFound, "Group is closed");
        let error = DsRequestError::Tonic(status);
        assert!(error.is_group_closed());
        assert!(error.is_not_found());

        let error = DsRequestError::Tonic(tonic::Status::not_found("Group not found"));
        assert!(!error.is_group_closed());
        assert_matches!(error.kind(), RequestErrorKind::NotFound);
    }

    #[test]
    fn unknown_status_with_nested_io_error_is_a_network_error() {
        #[derive(Debug, thiserror::Error)]
        #[error("transport error")]
        struct Transport(#[source] io::Error);

        let transport = Transport(io::Error::from(io::ErrorKind::ConnectionReset));
        let status = tonic::Status::from_error(Box::new(transport));
        assert_eq!(status.code(), Code::Unknown);
        assert_matches!(
            DsRequestError::Tonic(status).kind(),
            RequestErrorKind::Network
        );

        let status = tonic::Status::unknown("server error");
        assert_matches!(
            DsRequestError::Tonic(status).kind(),
            RequestErrorKind::ServerError
        );
    }
}
