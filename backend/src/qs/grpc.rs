// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use airprotos::{
    queue_service::v1::{queue_service_server::QueueService, *},
    validation::{InvalidTlsExt, MissingFieldExt},
};

use aircommon::{
    identifiers,
    messages::client_qs::{
        CreateClientRecordParams, CreateUserRecordParams, DeleteClientRecordParams,
        DeleteUserRecordParams, KeyPackageParams, PublishKeyPackagesParams,
        UpdateClientRecordParams, UpdateUserRecordParams,
    },
};
use displaydoc::Display;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming, async_trait};
use tracing::error;

use crate::{errors::QueueError, qs::queue::Queues};

use super::Qs;

pub struct GrpcQs {
    qs: Qs,
}

impl GrpcQs {
    pub fn new(qs: Qs) -> Self {
        Self { qs }
    }

    async fn process_listen_queue_requests_task(
        queues: Queues,
        queue_id: identifiers::QsClientId,
        mut requests: Streaming<ListenRequest>,
    ) {
        while let Some(request) = requests.next().await {
            if let Err(error) = Self::process_listen_queue_request(&queues, queue_id, request).await
            {
                // We report the error, but don't stop processing requests.
                // TODO(#466): Send this to the client.
                error!(%error, "error processing listen queue request");
            }
        }
        // Listening stream was closed
        queues.stop_listening(queue_id).await;
    }

    async fn process_listen_queue_request(
        queues: &Queues,
        queue_id: identifiers::QsClientId,
        request: Result<ListenRequest, Status>,
    ) -> Result<(), ProcessListenQueueRequestError> {
        match request?.request {
            Some(listen_request::Request::Ack(AckListenRequest {
                up_to_sequence_number,
            })) => {
                queues.ack(queue_id, up_to_sequence_number).await?;
            }
            Some(listen_request::Request::Fetch(FetchListenRequest {})) => {
                queues.trigger_fetch(queue_id).await?;
            }
            Some(listen_request::Request::Init(_)) => {
                return Err(ProcessListenQueueRequestError::UnexpectedInitRequest);
            }
            None => {
                return Err(ProcessListenQueueRequestError::EmptyRequest);
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Display)]
enum ProcessListenQueueRequestError {
    /// {0}
    Queue(#[from] QueueError),
    /// Unexpected init request
    UnexpectedInitRequest,
    /// {0}
    Status(#[from] Status),
    /// Received empty request
    EmptyRequest,
}

// Note: currently, *no* authentication is done
#[async_trait]
impl QueueService for GrpcQs {
    async fn create_user(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<CreateUserResponse>, Status> {
        let request = request.into_inner();
        let params = CreateUserRecordParams {
            user_record_auth_key: request
                .user_record_auth_key
                .ok_or_missing_field("user_record_auth_key")?
                .into(),
            friendship_token: request
                .friendship_token
                .ok_or_missing_field("friendship_token")?
                .into(),
            client_record_auth_key: request
                .client_record_auth_key
                .ok_or_missing_field("client_record_auth_key")?
                .into(),
            queue_encryption_key: request
                .queue_encryption_key
                .ok_or_missing_field("queue_encryption_key")?
                .into(),
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
        request: Request<UpdateUserRequest>,
    ) -> Result<Response<UpdateUserResponse>, Status> {
        let request = request.into_inner();
        let params = UpdateUserRecordParams {
            sender: request.sender.ok_or_missing_field("sender")?.try_into()?,
            user_record_auth_key: request
                .user_record_auth_key
                .ok_or_missing_field("user_record_auth_key")?
                .into(),
            friendship_token: request
                .friendship_token
                .ok_or_missing_field("friendship_token")?
                .into(),
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
        request: Request<DeleteUserRequest>,
    ) -> Result<Response<DeleteUserResponse>, Status> {
        let request = request.into_inner();
        let params = DeleteUserRecordParams {
            sender: request.sender.ok_or_missing_field("sender")?.try_into()?,
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
        request: Request<CreateClientRequest>,
    ) -> Result<Response<CreateClientResponse>, Status> {
        let request = request.into_inner();
        let params = CreateClientRecordParams {
            sender: request.sender.ok_or_missing_field("sender")?.try_into()?,
            client_record_auth_key: request
                .client_record_auth_key
                .ok_or_missing_field("client_record_auth_key")?
                .into(),
            queue_encryption_key: request
                .queue_encryption_key
                .ok_or_missing_field("queue_encryption_key")?
                .into(),
            encrypted_push_token: request
                .encrypted_push_token
                .map(|token| token.try_into())
                .transpose()?,
            initial_ratchet_secret: request
                .initial_ratched_secret
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
        request: Request<UpdateClientRequest>,
    ) -> Result<Response<UpdateClientResponse>, Status> {
        let request = request.into_inner();
        let params = UpdateClientRecordParams {
            sender: request.sender.ok_or_missing_field("sender")?.try_into()?,
            client_record_auth_key: request
                .client_record_auth_key
                .ok_or_missing_field("client_record_auth_key")?
                .into(),
            queue_encryption_key: request
                .queue_encryption_key
                .ok_or_missing_field("queue_encryption_key")?
                .into(),
            encrypted_push_token: request
                .encrypted_push_token
                .map(|token| token.try_into())
                .transpose()?,
        };
        self.qs.qs_update_client_record(params).await?;
        Ok(Response::new(UpdateClientResponse {}))
    }

    async fn delete_client(
        &self,
        request: Request<DeleteClientRequest>,
    ) -> Result<Response<DeleteClientResponse>, Status> {
        let request = request.into_inner();
        let params = DeleteClientRecordParams {
            sender: request.sender.ok_or_missing_field("sender")?.try_into()?,
        };
        self.qs.qs_delete_client_record(params).await?;
        Ok(Response::new(DeleteClientResponse {}))
    }

    async fn publish_key_packages(
        &self,
        request: Request<PublishKeyPackagesRequest>,
    ) -> Result<Response<PublishKeyPackagesResponse>, Status> {
        let request = request.into_inner();
        let params = PublishKeyPackagesParams {
            sender: request
                .client_id
                .ok_or_missing_field("client_id")?
                .try_into()?,
            key_packages: request
                .key_packages
                .into_iter()
                .map(|key_package| key_package.try_into())
                .collect::<Result<Vec<_>, _>>()
                .invalid_tls("key_packages")?,
        };
        self.qs.qs_publish_key_packages(params).await?;
        Ok(Response::new(PublishKeyPackagesResponse {}))
    }

    async fn key_package(
        &self,
        request: Request<KeyPackageRequest>,
    ) -> Result<Response<KeyPackageResponse>, Status> {
        let request = request.into_inner();
        let params = KeyPackageParams {
            sender: request.sender.ok_or_missing_field("sender")?.into(),
        };
        let response = self.qs.qs_key_package(params).await?;
        Ok(Response::new(KeyPackageResponse {
            key_package: Some(response.key_package.try_into().tls_failed("key_package")?),
        }))
    }

    async fn qs_encryption_key(
        &self,
        _request: Request<QsEncryptionKeyRequest>,
    ) -> Result<Response<QsEncryptionKeyResponse>, Status> {
        let response = self.qs.qs_encryption_key().await?;
        Ok(Response::new(QsEncryptionKeyResponse {
            encryption_key: Some(response.encryption_key.into()),
        }))
    }

    type ListenStream = Pin<Box<dyn Stream<Item = Result<QueueEvent, Status>> + Send + 'static>>;

    async fn listen(
        &self,
        request: Request<Streaming<ListenRequest>>,
    ) -> Result<Response<Self::ListenStream>, Status> {
        let mut requests = request.into_inner();

        let request = requests
            .next()
            .await
            .ok_or(ListenQueueProtocolViolation::MissingInitRequest)??;
        let Some(listen_request::Request::Init(InitListenRequest {
            client_id,
            sequence_number_start,
        })) = request.request
        else {
            return Err(ListenQueueProtocolViolation::MissingInitRequest.into());
        };

        let client_id = client_id.ok_or_missing_field("client_id")?.try_into()?;

        let queue_messages = self
            .qs
            .queues
            .listen(client_id, sequence_number_start)
            .await?;
        let responses = queue_messages.map(|message| match message {
            Some(event) => event,
            None => QueueEvent {
                event: Some(queue_event::Event::Empty(QueueEmpty {})),
            },
        });

        tokio::spawn(Self::process_listen_queue_requests_task(
            self.qs.queues.clone(),
            client_id,
            requests,
        ));

        Ok(Response::new(Box::pin(responses.map(Ok))))
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
