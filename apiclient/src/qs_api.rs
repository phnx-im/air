// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{
        RatchetEncryptionKey,
        kdf::keys::RatchetSecret,
        signatures::keys::{QsClientSigningKey, QsClientVerifyingKey, QsUserSigningKey},
    },
    identifiers::{QsClientId, QsUserId},
    messages::{
        FriendshipToken,
        client_qs::{
            CreateClientRecordResponse, CreateUserRecordResponse, EncryptionKeyResponse,
            KeyPackageResponseIn,
        },
        push_token::EncryptedPushToken,
    },
};
use airprotos::{
    common::v1::{StatusDetails, StatusDetailsCode},
    queue_service::v1::{
        AckListenRequest, FetchListenRequest, InitListenRequest, QueueEvent, listen_request,
    },
};
use airprotos::{
    queue_service::v1::{
        CreateClientRequest, CreateUserRequest, DeleteClientRequest, DeleteUserRequest,
        KeyPackageRequest, ListenRequest, PublishKeyPackagesRequest, QsEncryptionKeyRequest,
        UpdateClientRequest, UpdateUserRequest,
    },
    validation::{MissingFieldError, MissingFieldExt},
};
use mls_assist::openmls::prelude::KeyPackage;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::{
    ApiClient,
    util::{CancellableStream, CancellingStream},
};

#[derive(Error, Debug)]
pub enum QsRequestError {
    #[error(transparent)]
    Tls(#[from] tls_codec::Error),
    #[error("received an unexpected response")]
    UnexpectedResponse,
    #[error(transparent)]
    Tonic(#[from] tonic::Status),
    #[error("missing field in response: {0}")]
    MissingField(#[from] MissingFieldError<&'static str>),
}

impl QsRequestError {
    pub fn is_unsupported_version(&self) -> bool {
        match self {
            Self::Tonic(status) => {
                status.code() == tonic::Code::FailedPrecondition
                    && StatusDetails::from_status(status)
                        .map(|details| details.code() == StatusDetailsCode::VersionUnsupported)
                        .unwrap_or(false)
            }
            _ => false,
        }
    }
}

impl ApiClient {
    pub async fn qs_create_user(
        &self,
        friendship_token: FriendshipToken,
        client_record_auth_key: QsClientVerifyingKey,
        queue_encryption_key: RatchetEncryptionKey,
        encrypted_push_token: Option<EncryptedPushToken>,
        initial_ratchet_key: RatchetSecret,
        signing_key: &QsUserSigningKey,
    ) -> Result<CreateUserRecordResponse, QsRequestError> {
        let request = CreateUserRequest {
            client_metadata: Some(self.metadata().clone()),
            user_record_auth_key: Some(signing_key.verifying_key().clone().into()),
            friendship_token: Some(friendship_token.into()),
            client_record_auth_key: Some(client_record_auth_key.into()),
            queue_encryption_key: Some(queue_encryption_key.into()),
            encrypted_push_token: encrypted_push_token.map(From::from),
            initial_ratched_secret: Some(initial_ratchet_key.into()),
        };
        let response = self
            .qs_grpc_client()
            .create_user(request)
            .await?
            .into_inner();
        Ok(CreateUserRecordResponse {
            user_id: response
                .user_id
                .ok_or_missing_field("user_id")?
                .try_into()
                .map_err(|error| {
                    error!(%error, "invalid user_id in response");
                    QsRequestError::UnexpectedResponse
                })?,
            qs_client_id: response
                .client_id
                .ok_or_missing_field("client_id")?
                .try_into()
                .map_err(|error| {
                    error!(%error, "invalid client_id in response");
                    QsRequestError::UnexpectedResponse
                })?,
        })
    }

    pub async fn qs_update_user(
        &self,
        sender: QsUserId,
        friendship_token: FriendshipToken,
        signing_key: &QsUserSigningKey,
    ) -> Result<(), QsRequestError> {
        let request = UpdateUserRequest {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(sender.into()),
            user_record_auth_key: Some(signing_key.verifying_key().clone().into()),
            friendship_token: Some(friendship_token.into()),
        };
        self.qs_grpc_client().update_user(request).await?;
        Ok(())
    }

    pub async fn qs_delete_user(
        &self,
        sender: QsUserId,
        _signing_key: &QsUserSigningKey,
    ) -> Result<(), QsRequestError> {
        let request = DeleteUserRequest {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(sender.into()),
        };
        self.qs_grpc_client().delete_user(request).await?;
        Ok(())
    }

    pub async fn qs_create_client(
        &self,
        sender: QsUserId,
        client_record_auth_key: QsClientVerifyingKey,
        queue_encryption_key: RatchetEncryptionKey,
        encrypted_push_token: Option<EncryptedPushToken>,
        initial_ratchet_key: RatchetSecret,
        _signing_key: &QsUserSigningKey,
    ) -> Result<CreateClientRecordResponse, QsRequestError> {
        let request = CreateClientRequest {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(sender.into()),
            client_record_auth_key: Some(client_record_auth_key.into()),
            queue_encryption_key: Some(queue_encryption_key.into()),
            encrypted_push_token: encrypted_push_token.map(|token| token.into()),
            initial_ratched_secret: Some(initial_ratchet_key.into()),
        };
        let response = self
            .qs_grpc_client()
            .create_client(request)
            .await?
            .into_inner();
        Ok(CreateClientRecordResponse {
            qs_client_id: response
                .client_id
                .ok_or_missing_field("client_id")?
                .try_into()
                .map_err(|error| {
                    error!(%error, "invalid client_id in response");
                    QsRequestError::UnexpectedResponse
                })?,
        })
    }

    pub async fn qs_update_client(
        &self,
        sender: QsClientId,
        queue_encryption_key: RatchetEncryptionKey,
        encrypted_push_token: Option<EncryptedPushToken>,
        signing_key: &QsClientSigningKey,
    ) -> Result<(), QsRequestError> {
        let request = UpdateClientRequest {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(sender.into()),
            client_record_auth_key: Some(signing_key.verifying_key().clone().into()),
            queue_encryption_key: Some(queue_encryption_key.into()),
            encrypted_push_token: encrypted_push_token.map(|token| token.into()),
        };
        self.qs_grpc_client().update_client(request).await?;
        Ok(())
    }

    pub async fn qs_delete_client(
        &self,
        sender: QsClientId,
        _signing_key: &QsClientSigningKey,
    ) -> Result<(), QsRequestError> {
        let request = DeleteClientRequest {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(sender.into()),
        };
        self.qs_grpc_client().delete_client(request).await?;
        Ok(())
    }

    pub async fn qs_publish_key_packages(
        &self,
        sender: QsClientId,
        key_packages: Vec<KeyPackage>,
        _signing_key: &QsClientSigningKey,
    ) -> Result<(), QsRequestError> {
        let request = PublishKeyPackagesRequest {
            client_metadata: Some(self.metadata().clone()),
            client_id: Some(sender.into()),
            key_packages: key_packages
                .into_iter()
                .map(|key_package| key_package.try_into())
                .collect::<Result<Vec<_>, _>>()?,
        };
        self.qs_grpc_client().publish_key_packages(request).await?;
        Ok(())
    }

    pub async fn qs_key_package(
        &self,
        sender: FriendshipToken,
    ) -> Result<KeyPackageResponseIn, QsRequestError> {
        let request = KeyPackageRequest {
            client_metadata: Some(self.metadata().clone()),
            sender: Some(sender.into()),
        };
        let response = self
            .qs_grpc_client()
            .key_package(request)
            .await?
            .into_inner();
        let key_package = response
            .key_package
            .ok_or_missing_field("key_package")?
            .try_into()
            .map_err(|error| {
                error!(%error, "invalid key_package in response");
                QsRequestError::UnexpectedResponse
            })?;
        Ok(KeyPackageResponseIn { key_package })
    }

    pub async fn qs_encryption_key(&self) -> Result<EncryptionKeyResponse, QsRequestError> {
        let request = QsEncryptionKeyRequest {
            client_metadata: Some(self.metadata().clone()),
        };
        let response = self
            .qs_grpc_client()
            .qs_encryption_key(request)
            .await?
            .into_inner();
        let encryption_key = response
            .encryption_key
            .ok_or_missing_field("encryption_key")?
            .into();
        Ok(EncryptionKeyResponse { encryption_key })
    }

    /// Listens to a queue.
    ///
    /// Returns a stream of [`QueueEvent`]s and a [`ListenResponder`].
    ///
    /// The connection to server is bound to the lifetime of the stream. When the stream has ended
    /// or is dropped, the connection is closed. In this case, the [`ListenResponder`] is closed.
    pub async fn listen_queue(
        &self,
        queue_id: QsClientId,
        sequence_number_start: u64,
    ) -> Result<(impl Stream<Item = QueueEvent> + use<>, ListenResponder), QsRequestError> {
        let init_request = InitListenRequest {
            client_metadata: Some(self.metadata().clone()),
            client_id: Some(queue_id.into()),
            sequence_number_start,
        };
        let init_request = ListenRequest {
            request: Some(listen_request::Request::Init(init_request)),
        };

        const RESPONSE_CHANNEL_BUFFER_SIZE: usize = 16; // not too big for applying backpressure
        let (tx, rx) = mpsc::channel::<ListenRequest>(RESPONSE_CHANNEL_BUFFER_SIZE);

        // Cancels the requests stream when the responses stream is ended or was dropped
        let cancel = CancellationToken::new();

        let requests = CancellableStream::new(
            tokio_stream::once(init_request).chain(ReceiverStream::new(rx)),
            cancel.clone(),
        );

        let response = self.qs_grpc_client().listen(requests).await?;
        let responses = response.into_inner().map_while(|response| {
            response
                .inspect_err(|status| error!(?status, "terminating listen stream due to an error"))
                .ok()
        });

        let stream = CancellingStream::new(responses, cancel);
        let responder = ListenResponder { tx };

        Ok((stream, responder))
    }
}

#[derive(Debug, Clone)]
pub struct ListenResponder {
    tx: mpsc::Sender<ListenRequest>,
}

#[derive(Debug, thiserror::Error)]
#[error("listen responder is closed")]
pub struct ListenResponderClosedError;

impl ListenResponder {
    pub async fn ack(&self, up_to_sequence_number: u64) -> Result<(), ListenResponderClosedError> {
        self.tx
            .send(ListenRequest {
                request: Some(listen_request::Request::Ack(AckListenRequest {
                    up_to_sequence_number,
                })),
            })
            .await
            .map_err(|_| ListenResponderClosedError)
    }

    pub async fn fetch(&self) -> Result<(), ListenResponderClosedError> {
        self.tx
            .send(ListenRequest {
                request: Some(listen_request::Request::Fetch(FetchListenRequest {})),
            })
            .await
            .map_err(|_| ListenResponderClosedError)
    }
}
