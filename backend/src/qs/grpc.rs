// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use airprotos::{
    common::v1::ClientMetadata,
    queue_service::v1::{queue_service_server::QueueService, *},
    signed::SignedRequest,
    validation::{InvalidTlsExt, MissingFieldExt},
};

use aircommon::{
    identifiers,
    messages::client_qs::{
        CreateClientRecordParams, CreateUserRecordParams, DeleteClientRecordParams,
        DeleteUserRecordParams, KeyPackageParams, PublishKeyPackagesParams,
        UpdateClientRecordParams, UpdateUserRecordParams,
    },
    time::TimeStamp,
    virtual_client::KeyPackageBatchId,
};
use chrono::Utc;
use displaydoc::Display;
use mls_assist::openmls::{components::vc_derivation_info::EpochId, prelude::LeafNodeIndex};
use prost::Message;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming, async_trait};
use tracing::{error, info_span, warn};

use crate::{
    errors::QueueError,
    listen_session::{ListenRequestHandler, spawn_listen_session},
    qs::{
        client_record::QsClientRecord,
        queue::{Queues, user_clients::ClientSession},
        user_record::UserRecord,
    },
    version::VerifiedClientVersion,
};

/// Maximum number of key packages per batch to upload in one request.
const MAX_KEY_PACKAGES_PER_BATCH: usize = 512;

/// Maximum size in bytes of the encrypted client state payload.
const MAX_ENCRYPTED_CLIENT_STATE_SIZE_BYTES: usize = 256;

use super::Qs;

pub struct GrpcQs {
    pub(super) qs: Qs,
}

impl GrpcQs {
    pub fn new(qs: Qs) -> Self {
        Self { qs }
    }

    async fn update_client_activity_and_report_metrics(
        &self,
        client_id: aircommon::identifiers::QsClientId,
    ) -> sqlx::Result<()> {
        let mut connection = self.qs.db_pool.acquire().await?;
        QsClientRecord::update_activity_time(&mut *connection, client_id, TimeStamp::now()).await?;
        UserRecord::metrics(&mut *connection).await?.report();
        Ok(())
    }

    fn verify_client_version(
        &self,
        client_metadata: Option<&ClientMetadata>,
    ) -> Result<VerifiedClientVersion, Status> {
        self.qs
            .version_policy
            .verify_client_version(client_metadata, Utc::now())
    }
}

#[derive(Debug, thiserror::Error, Display)]
enum ProcessListenQueueRequestError {
    /// Unexpected init request
    UnexpectedInitRequest,
}

impl From<ProcessListenQueueRequestError> for Status {
    fn from(error: ProcessListenQueueRequestError) -> Self {
        match error {
            ProcessListenQueueRequestError::UnexpectedInitRequest => {
                Status::invalid_argument(error.to_string())
            }
        }
    }
}

// Note: currently, *no* authentication is done
#[async_trait]
impl QueueService for GrpcQs {
    async fn create_user(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<CreateUserResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;

        let params = CreateUserRecordParams {
            user_record_auth_key: request
                .user_record_auth_key
                .ok_or_missing_field("user_record_auth_key")?
                .try_into()?,
            friendship_token: request
                .friendship_token
                .ok_or_missing_field("friendship_token")?
                .try_into()?,
            client_record_auth_key: request
                .client_record_auth_key
                .ok_or_missing_field("client_record_auth_key")?
                .try_into()?,
            queue_encryption_key: request
                .queue_encryption_key
                .ok_or_missing_field("queue_encryption_key")?
                .try_into()?,
            encrypted_push_token: request
                .encrypted_push_token
                .map(|token| token.try_into())
                .transpose()?,
            initial_ratchet_secret: request
                .initial_ratched_secret
                .ok_or_missing_field("initial_ratched_secret")?
                .try_into()?,
        };
        let response = self
            .qs
            .qs_create_user_record(params)
            .await
            .map_err(|error| {
                error!(%error, "failed to create user record");
                Status::internal("failed to create user record")
            })?;
        let response = CreateUserResponse {
            user_id: Some(response.user_id.into()),
            client_id: Some(response.qs_client_id.into()),
        };
        Ok(Response::new(response))
    }

    async fn update_user(
        &self,
        request: Request<SignedRequest<UpdateUserRequest, 5>>,
    ) -> Result<Response<UpdateUserResponse>, Status> {
        let request = request.into_inner();

        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref())
                .or(request.inner().client_metadata.as_ref()),
        )?;

        let UpdateUserPayload {
            client_metadata: _,
            sender,
            user_record_auth_key,
            friendship_token,
        } = self.verify_user_auth(request).await?;

        let params = UpdateUserRecordParams {
            sender: sender.ok_or_missing_field("sender")?.try_into()?,
            user_record_auth_key: user_record_auth_key
                .ok_or_missing_field("user_record_auth_key")?
                .try_into()?,
            friendship_token: friendship_token
                .ok_or_missing_field("friendship_token")?
                .try_into()?,
        };
        self.qs
            .qs_update_user_record(params)
            .await
            .map_err(|error| {
                error!(%error, "failed to update user record");
                Status::internal("failed to update user record")
            })?;
        Ok(Response::new(UpdateUserResponse {}))
    }

    async fn delete_user(
        &self,
        request: Request<SignedRequest<DeleteUserRequest, 3>>,
    ) -> Result<Response<DeleteUserResponse>, Status> {
        let request = request.into_inner();

        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref())
                .or(request.inner().client_metadata.as_ref()),
        )?;

        let DeleteUserPayload {
            client_metadata: _,
            sender,
        } = self.verify_user_auth(request).await?;

        let params = DeleteUserRecordParams {
            sender: sender.ok_or_missing_field("sender")?.try_into()?,
        };
        self.qs
            .qs_delete_user_record(params)
            .await
            .map_err(|error| {
                error!(%error, "failed to delete user record");
                Status::internal("failed to delete user record")
            })?;
        Ok(Response::new(DeleteUserResponse {}))
    }

    async fn create_client(
        &self,
        request: Request<SignedRequest<CreateClientRequest, 7>>,
    ) -> Result<Response<CreateClientResponse>, Status> {
        let request = request.into_inner();
        let CreateClientPayload {
            client_metadata,
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            initial_ratched_secret,
        } = self.verify_user_auth(request).await?;
        self.verify_client_version(client_metadata.as_ref())?;
        let params = CreateClientRecordParams {
            sender: sender.ok_or_missing_field("sender")?.try_into()?,
            client_record_auth_key: client_record_auth_key
                .ok_or_missing_field("client_record_auth_key")?
                .try_into()?,
            queue_encryption_key: queue_encryption_key
                .ok_or_missing_field("queue_encryption_key")?
                .try_into()?,
            encrypted_push_token: encrypted_push_token
                .map(|token| token.try_into())
                .transpose()?,
            initial_ratchet_secret: initial_ratched_secret
                .ok_or_missing_field("initial_ratched_secret")?
                .try_into()?,
        };
        let response = self.qs.qs_create_client_record(params).await?;
        Ok(Response::new(CreateClientResponse {
            client_id: Some(response.qs_client_id.into()),
        }))
    }

    async fn update_client(
        &self,
        request: Request<SignedRequest<UpdateClientRequest, 6>>,
    ) -> Result<Response<UpdateClientResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref())
                .or(request.inner().client_metadata.as_ref()),
        )?;
        let UpdateClientPayload {
            client_metadata: _,
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
        } = self.verify_client_auth(request).await?;
        let params = UpdateClientRecordParams {
            sender: sender.ok_or_missing_field("sender")?.try_into()?,
            client_record_auth_key: client_record_auth_key
                .ok_or_missing_field("client_record_auth_key")?
                .try_into()?,
            queue_encryption_key: queue_encryption_key
                .ok_or_missing_field("queue_encryption_key")?
                .try_into()?,
            encrypted_push_token: encrypted_push_token
                .map(|token| token.try_into())
                .transpose()?,
        };
        self.qs.qs_update_client_record(params).await?;
        Ok(Response::new(UpdateClientResponse {}))
    }

    async fn delete_client(
        &self,
        request: Request<SignedRequest<DeleteClientRequest, 3>>,
    ) -> Result<Response<DeleteClientResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref())
                .or(request.inner().client_metadata.as_ref()),
        )?;
        let DeleteClientPayload {
            client_metadata: _,
            sender,
        } = self.verify_client_auth(request).await?;
        let params = DeleteClientRecordParams {
            sender: sender.ok_or_missing_field("sender")?.try_into()?,
        };
        self.qs.qs_delete_client_record(&params).await?;
        self.qs.queues.disconnect(params.sender);
        Ok(Response::new(DeleteClientResponse {}))
    }

    async fn publish_key_packages(
        &self,
        request: Request<SignedRequest<PublishKeyPackagesRequest, 4>>,
    ) -> Result<Response<PublishKeyPackagesResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref())
                .or(request.inner().client_metadata.as_ref()),
        )?;
        let PublishKeyPackagesPayload {
            client_metadata: _,
            client_id,
            key_packages,
        } = self.verify_client_auth(request).await?;
        let params = PublishKeyPackagesParams {
            sender: client_id.ok_or_missing_field("client_id")?.try_into()?,
            key_packages: key_packages
                .into_iter()
                .map(|key_package| key_package.try_into())
                .collect::<Result<Vec<_>, _>>()
                .invalid_tls("key_packages")?,
        };
        self.qs.qs_publish_key_packages(params).await?;
        Ok(Response::new(PublishKeyPackagesResponse {}))
    }

    async fn stage_key_packages(
        &self,
        request: Request<SignedRequest<StageKeyPackagesRequest, 1>>,
    ) -> Result<Response<StageKeyPackagesResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref()),
        )?;
        let StageKeyPackagesPayload {
            client_metadata: _,
            client_id,
            epoch_id,
            leaf_index,
            generation,
            key_packages,
            apq_key_packages,
        } = self.verify_client_auth(request).await?;

        if key_packages.len() + apq_key_packages.len() > MAX_KEY_PACKAGES_PER_BATCH {
            return Err(Status::invalid_argument("Too many key packages"));
        }

        let client_id = client_id.ok_or_missing_field("client_id")?.try_into()?;
        self.qs
            .qs_stage_key_packages(
                client_id,
                KeyPackageBatchId {
                    epoch_id: EpochId::new(epoch_id),
                    leaf_index: LeafNodeIndex::new(leaf_index),
                    generation,
                },
                key_packages,
                apq_key_packages,
            )
            .await?;

        Ok(Response::new(StageKeyPackagesResponse {}))
    }

    async fn key_package(
        &self,
        request: Request<KeyPackageRequest>,
    ) -> Result<Response<KeyPackageResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;
        let params = KeyPackageParams {
            sender: request.sender.ok_or_missing_field("sender")?.try_into()?,
        };
        let response = self.qs.qs_key_package(params).await?;
        Ok(Response::new(KeyPackageResponse {
            key_package: Some(response.key_package.try_into().tls_failed("key_package")?),
        }))
    }

    async fn publish_apq_key_packages(
        &self,
        request: Request<SignedRequest<PublishApqKeyPackagesRequest>>,
    ) -> Result<Response<PublishApqKeyPackagesResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(
            request
                .inner()
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref()),
        )?;
        let PublishApqKeyPackagesPayload {
            client_metadata: _,
            client_id,
            apq_key_packages,
        } = self.verify_client_auth(request).await?;
        let sender = client_id.ok_or_missing_field("client_id")?.try_into()?;
        let apq_key_packages = apq_key_packages
            .into_iter()
            .map(|key_package| key_package.try_into())
            .collect::<Result<Vec<_>, _>>()
            .invalid_tls("key_packages")?;
        self.qs
            .qs_publish_apq_key_packages(sender, apq_key_packages)
            .await?;
        Ok(Response::new(PublishApqKeyPackagesResponse {}))
    }

    async fn apq_key_package(
        &self,
        request: Request<ApqKeyPackageRequest>,
    ) -> Result<Response<ApqKeyPackageResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;
        let friendship_token = request.sender.ok_or_missing_field("sender")?.try_into()?;
        let key_package = self.qs.qs_apq_key_package(friendship_token).await?;
        Ok(Response::new(ApqKeyPackageResponse {
            key_package: Some(key_package.try_into().tls_failed("key_package")?),
        }))
    }

    async fn qs_encryption_key(
        &self,
        request: Request<QsEncryptionKeyRequest>,
    ) -> Result<Response<QsEncryptionKeyResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;
        let response = self.qs.qs_encryption_key().await?;
        Ok(Response::new(QsEncryptionKeyResponse {
            encryption_key: Some(response.encryption_key.into()),
        }))
    }

    type ListenStream =
        Pin<Box<dyn Stream<Item = Result<ListenResponse, Status>> + Send + 'static>>;

    async fn listen(
        &self,
        request: Request<Streaming<ListenRequest>>,
    ) -> Result<Response<Self::ListenStream>, Status> {
        let mut requests = request.into_inner();

        let request = requests
            .next()
            .await
            .ok_or(ListenQueueProtocolViolation::MissingInitRequest)??;
        let Some(listen_request::Request::Init(init_request)) = request.request else {
            return Err(ListenQueueProtocolViolation::MissingInitRequest.into());
        };

        let verified_client_version = self.verify_client_version(
            init_request
                .payload
                .as_ref()
                .and_then(|p| p.client_metadata.as_ref())
                .or(init_request.client_metadata.as_ref()),
        )?;
        let version_status = ListenResponse {
            event: Some(listen_response::Event::VersionStatus(VersionStatus {
                expires_at: verified_client_version
                    .expires_at
                    .map(|ts| TimeStamp::from(ts).into()),
                max_devices: self.qs.max_devices.get(),
            })),
        };

        let payload_bytes = init_request
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .encode_to_vec();
        let (
            InitListenPayload {
                client_metadata: _,
                client_id,
                sequence_number_start,
            },
            user_id,
        ) = self
            .verify_queue_client_auth_with_user_id(SignedRequest::<_, 1>::new(
                init_request,
                payload_bytes.into(),
            ))
            .await?;

        let client_id = client_id.ok_or_missing_field("client_id")?.try_into()?;
        let user_id = match user_id {
            Some(user_id) => user_id,
            // Legacy clients are not authenticated. Like for authenticated
            // clients, the user may be deleted.
            None => {
                QsClientRecord::load_auth_record(&self.qs.db_pool, &client_id)
                    .await
                    .map_err(QueueError::from)?
                    .ok_or(QueueError::ClientNotFound)?
                    .user_id
            }
        };
        let (session, queue_messages) = self
            .qs
            .queues
            .listen(
                user_id,
                client_id,
                verified_client_version.version,
                sequence_number_start,
            )
            .await?;
        let events = queue_messages.map(|message| match message {
            Some(event) => event,
            None => ListenResponse {
                event: Some(listen_response::Event::Empty(QueueEmpty {})),
            },
        });
        // Always sent first, so that the client knows its siblings' states.
        let sibling_client_states = self.qs.queues.user_states(user_id, client_id);
        let events = tokio_stream::once(version_status)
            .chain(tokio_stream::iter(sibling_client_states))
            .chain(events);

        self.update_client_activity_and_report_metrics(client_id)
            .await
            .inspect_err(|error| {
                error!(%error, "Error updating client activity and reporting metrics");
            })
            .ok();

        let handler = QueueSessionHandler {
            queues: self.qs.queues.clone(),
            client_id,
            session,
        };
        let qs_client = self.qs.pseudonymizer.client(&client_id);
        let responses = info_span!("qs_listen", %qs_client).in_scope(|| {
            spawn_listen_session(requests, events, self.qs.stop.clone(), handler, "qs")
        });
        Ok(Response::new(responses))
    }
}

struct QueueSessionHandler {
    queues: Queues,
    client_id: identifiers::QsClientId,
    session: ClientSession,
}

impl ListenRequestHandler<ListenRequest> for QueueSessionHandler {
    async fn handle(&mut self, request: ListenRequest) -> Result<(), Status> {
        match request.request {
            Some(listen_request::Request::Ack(AckListenRequest {
                up_to_sequence_number,
            })) => {
                self.queues
                    .ack(self.client_id, up_to_sequence_number)
                    .await?;
            }
            Some(listen_request::Request::Fetch(FetchListenRequest {})) => {
                self.queues.trigger_fetch(self.client_id).await?;
            }
            Some(listen_request::Request::ClientState(ReportClientStateListenRequest {
                encrypted_blob,
            })) => {
                if encrypted_blob.len() > MAX_ENCRYPTED_CLIENT_STATE_SIZE_BYTES {
                    warn!(
                        client_id =? self.client_id,
                        size = encrypted_blob.len(),
                        "ignoring too large client state"
                    );
                } else {
                    self.session.update_state(encrypted_blob);
                }
            }
            Some(listen_request::Request::Init(_)) => {
                return Err(ProcessListenQueueRequestError::UnexpectedInitRequest.into());
            }
            Some(listen_request::Request::KeepAlive(_)) => {}
            None => {}
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Display)]
enum ListenQueueProtocolViolation {
    /// Missing initial request
    MissingInitRequest,
}

impl From<ListenQueueProtocolViolation> for Status {
    fn from(error: ListenQueueProtocolViolation) -> Self {
        Status::failed_precondition(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::{
        crypto::aead::AEAD_NONCE_SIZE, messages::push_token::MAX_ENCRYPTED_PUSH_TOKEN_SIZE,
    };
    use airprotos::common::v1::{Ciphertext, RatchetEncryptionKey, RatchetSecret};
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::air_service::BackendService;

    use super::*;

    fn valid_create_user_request() -> CreateUserRequest {
        CreateUserRequest {
            client_metadata: None,
            user_record_auth_key: Some(QsUserVerifyingKey { bytes: vec![1; 32] }),
            friendship_token: Some(FriendshipToken { bytes: vec![2; 32] }),
            client_record_auth_key: Some(QsClientVerifyingKey { bytes: vec![3; 32] }),
            queue_encryption_key: Some(RatchetEncryptionKey { bytes: vec![4; 32] }),
            encrypted_push_token: Some(EncryptedPushToken {
                ciphertext: Some(Ciphertext {
                    ciphertext: vec![5; 64],
                    nonce: vec![6; AEAD_NONCE_SIZE],
                }),
            }),
            initial_ratched_secret: Some(RatchetSecret { bytes: vec![7; 32] }),
        }
    }

    async fn record_counts(pool: &PgPool) -> sqlx::Result<(i64, i64)> {
        let users = sqlx::query_scalar("SELECT COUNT(*) FROM qs_user_record")
            .fetch_one(pool)
            .await?;
        let clients = sqlx::query_scalar("SELECT COUNT(*) FROM qs_client_record")
            .fetch_one(pool)
            .await?;
        Ok((users, clients))
    }

    #[sqlx::test]
    async fn create_user_rejects_oversized_fields(pool: PgPool) -> anyhow::Result<()> {
        let qs = Qs::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;
        let grpc_qs = GrpcQs::new(qs);

        let oversize: [fn(&mut CreateUserRequest); 5] = [
            |request| {
                request.user_record_auth_key = Some(QsUserVerifyingKey { bytes: vec![1; 33] })
            },
            |request| request.friendship_token = Some(FriendshipToken { bytes: vec![2; 33] }),
            |request| {
                request.client_record_auth_key = Some(QsClientVerifyingKey { bytes: vec![3; 33] })
            },
            |request| {
                request.queue_encryption_key = Some(RatchetEncryptionKey { bytes: vec![4; 33] })
            },
            |request| {
                request.encrypted_push_token = Some(EncryptedPushToken {
                    ciphertext: Some(Ciphertext {
                        ciphertext: vec![5; MAX_ENCRYPTED_PUSH_TOKEN_SIZE + 1],
                        nonce: vec![6; AEAD_NONCE_SIZE],
                    }),
                })
            },
        ];
        for apply in oversize {
            let mut request = valid_create_user_request();
            apply(&mut request);
            let status = grpc_qs
                .create_user(Request::new(request))
                .await
                .unwrap_err();
            assert_eq!(status.code(), tonic::Code::InvalidArgument);
        }
        assert_eq!(record_counts(&pool).await?, (0, 0));

        grpc_qs
            .create_user(Request::new(valid_create_user_request()))
            .await?;
        assert_eq!(record_counts(&pool).await?, (1, 1));

        Ok(())
    }

    #[sqlx::test]
    async fn key_package_lookups_reject_wrong_length_tokens(pool: PgPool) -> anyhow::Result<()> {
        let qs = Qs::initialize(
            pool,
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;
        let grpc_qs = GrpcQs::new(qs);
        let sender = Some(FriendshipToken { bytes: vec![2; 33] });

        let status = grpc_qs
            .key_package(Request::new(KeyPackageRequest {
                client_metadata: None,
                sender: sender.clone(),
            }))
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        let status = grpc_qs
            .apq_key_package(Request::new(ApqKeyPackageRequest {
                client_metadata: None,
                sender,
            }))
            .await
            .unwrap_err();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);

        Ok(())
    }
}
