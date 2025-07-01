// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::identity;

use futures_util::{FutureExt, future::BoxFuture};
use phnxcommon::{
    LibraryError,
    credentials::{
        ClientCredentialPayload,
        keys::{ClientSigningKey, HandleSigningKey},
    },
    crypto::{indexed_aead::keys::UserProfileKeyIndex, signatures::signable::Signable},
    identifiers::{UserHandle, UserHandleHash, UserId},
    messages::{
        client_as::{ConnectionOfferMessage, ConnectionPackage},
        client_as_out::{
            AsCredentialsResponseIn, ConnectionPackageIn, EncryptedUserProfile,
            GetUserProfileResponse, RegisterUserResponseIn,
        },
    },
};
use phnxprotos::auth_service::v1::{
    AckListenHandleRequest, AsCredentialsRequest, ConnectRequest, ConnectResponse,
    CreateHandlePayload, DeleteHandlePayload, DeleteUserPayload, EnqueueConnectionOfferStep,
    FetchConnectionPackageStep, GetUserProfileRequest, HandleQueueMessage, InitListenHandlePayload,
    ListenHandleRequest, MergeUserProfilePayload, PublishConnectionPackagesPayload,
    RegisterUserRequest, StageUserProfilePayload, connect_request, connect_response,
    listen_handle_request,
};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::Request;
use tracing::error;
use uuid::Uuid;

pub mod grpc;

use crate::ApiClient;

#[derive(Error, Debug)]
pub enum AsRequestError {
    #[error("Library Error")]
    LibraryError,
    #[error("Received an unexpected response type")]
    UnexpectedResponse,
    #[error(transparent)]
    Tonic(#[from] tonic::Status),
}

impl AsRequestError {
    pub fn is_not_found(&self) -> bool {
        match self {
            AsRequestError::Tonic(status) => status.code() == tonic::Code::NotFound,
            _ => false,
        }
    }
}

impl From<LibraryError> for AsRequestError {
    fn from(_: LibraryError) -> Self {
        AsRequestError::LibraryError
    }
}

impl ApiClient {
    pub async fn as_register_user(
        &self,
        client_payload: ClientCredentialPayload,
        encrypted_user_profile: EncryptedUserProfile,
    ) -> Result<RegisterUserResponseIn, AsRequestError> {
        let request = RegisterUserRequest {
            client_credential_payload: Some(client_payload.into()),
            encrypted_user_profile: Some(encrypted_user_profile.into()),
        };
        let response = self
            .as_grpc_client
            .client()
            .register_user(Request::new(request))
            .await?
            .into_inner();
        Ok(RegisterUserResponseIn {
            client_credential: response
                .client_credential
                .ok_or_else(|| {
                    error!("missing `client_credential` in response");
                    AsRequestError::UnexpectedResponse
                })?
                .try_into()
                .map_err(|error| {
                    error!(%error, "invalid client_credential in response");
                    AsRequestError::UnexpectedResponse
                })?,
        })
    }

    pub async fn as_get_user_profile(
        &self,
        user_id: UserId,
        key_index: UserProfileKeyIndex,
    ) -> Result<GetUserProfileResponse, AsRequestError> {
        let request = GetUserProfileRequest {
            user_id: Some(user_id.into()),
            key_index: key_index.into_bytes().to_vec(),
        };
        let response = self
            .as_grpc_client
            .client()
            .get_user_profile(request)
            .await?
            .into_inner();
        Ok(GetUserProfileResponse {
            encrypted_user_profile: response
                .encrypted_user_profile
                .ok_or_else(|| {
                    error!("missing `encrypted_user_profile` in response");
                    AsRequestError::UnexpectedResponse
                })?
                .try_into()
                .map_err(|error| {
                    error!(%error, "invalid encrypted_user_profile in response");
                    AsRequestError::UnexpectedResponse
                })?,
        })
    }

    pub async fn as_stage_user_profile(
        &self,
        user_id: UserId,
        signing_key: &ClientSigningKey,
        encrypted_user_profile: EncryptedUserProfile,
    ) -> Result<(), AsRequestError> {
        let payload = StageUserProfilePayload {
            user_id: Some(user_id.into()),
            encrypted_user_profile: Some(encrypted_user_profile.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client
            .client()
            .stage_user_profile(request)
            .await?;
        Ok(())
    }

    pub async fn as_merge_user_profile(
        &self,
        user_id: UserId,
        signing_key: &ClientSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = MergeUserProfilePayload {
            user_id: Some(user_id.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client
            .client()
            .merge_user_profile(request)
            .await?;
        Ok(())
    }

    pub async fn as_delete_user(
        &self,
        user_id: UserId,
        signing_key: &ClientSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = DeleteUserPayload {
            user_id: Some(user_id.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client.client().delete_user(request).await?;
        Ok(())
    }

    pub async fn as_publish_connection_packages_for_handle(
        &self,
        hash: UserHandleHash,
        connection_packages: Vec<ConnectionPackage>,
        signing_key: &HandleSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = PublishConnectionPackagesPayload {
            hash: Some(hash.into()),
            connection_packages: connection_packages.into_iter().map(From::from).collect(),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client
            .client()
            .publish_connection_packages(request)
            .await?;
        Ok(())
    }

    pub async fn as_connect_handle(
        &self,
        handle: &UserHandle,
    ) -> Result<(ConnectionPackageIn, ConnectionOfferResponder), AsRequestError> {
        let hash = handle.hash().map_err(|error| {
            error!(%error, "failed to hash user handle");
            AsRequestError::LibraryError
        })?;

        // Step 1: Fetch connection package
        let fetch_request = ConnectRequest {
            step: Some(connect_request::Step::Fetch(FetchConnectionPackageStep {
                hash: Some(hash.into()),
            })),
        };

        // Step 2: Enqueue connection offer
        let (connection_offer_tx, connection_offer_rx) =
            oneshot::channel::<ConnectionOfferMessage>();
        let connection_offer_fut = async move {
            let connection_offer = connection_offer_rx.await.ok()?;
            Some(ConnectRequest {
                step: Some(connect_request::Step::Enqueue(EnqueueConnectionOfferStep {
                    connection_offer: Some(connection_offer.into()),
                })),
            })
        };

        let requests = tokio_stream::once(Some(fetch_request))
            .chain(connection_offer_fut.into_stream())
            .filter_map(identity);
        let mut responses = self
            .as_grpc_client
            .client()
            .connect_handle(requests)
            .await?
            .into_inner();

        let response = responses.next().await.ok_or_else(|| {
            error!("protocol violation: missing response");
            AsRequestError::UnexpectedResponse
        })??;

        let connection_package: ConnectionPackageIn = match response {
            ConnectResponse {
                step: Some(connect_response::Step::FetchResponse(fetch)),
            } => fetch
                .connection_package
                .ok_or_else(|| {
                    error!("protocol violation: missing connection package");
                    AsRequestError::UnexpectedResponse
                })?
                .try_into()
                .map_err(|error| {
                    error!(%error, "invalid connection package");
                    AsRequestError::UnexpectedResponse
                })?,
            _ => {
                error!("protocol violation: expected fetch response");
                return Err(AsRequestError::UnexpectedResponse);
            }
        };

        let connection_offer_response_fut = async move {
            let response = responses.next().await.ok_or_else(|| {
                error!("protocol violation: missing connection offer response");
                AsRequestError::UnexpectedResponse
            })??;
            match response {
                ConnectResponse {
                    step: Some(connect_response::Step::EnqueueResponse(_)),
                } => Ok(()),
                _ => {
                    error!("protocol violation: expected connection offer response");
                    Err(AsRequestError::UnexpectedResponse)
                }
            }
        };

        let responder =
            ConnectionOfferResponder::new(connection_offer_tx, connection_offer_response_fut);
        Ok((connection_package, responder))
    }

    pub async fn as_listen_handle(
        &self,
        hash: UserHandleHash,
        signing_key: &HandleSigningKey,
    ) -> Result<
        (
            impl Stream<Item = Option<HandleQueueMessage>> + Send + use<>,
            ListenHandleResponder,
        ),
        AsRequestError,
    > {
        let init_payload = InitListenHandlePayload {
            hash: Some(hash.into()),
        };
        let init_request = init_payload.sign(signing_key)?;

        const ACK_CHANNEL_BUFFER_SIZE: usize = 16; // not too big for applying backpressure
        let (ack_tx, ack_rx) = mpsc::channel::<Uuid>(ACK_CHANNEL_BUFFER_SIZE);

        let requests = tokio_stream::once(ListenHandleRequest {
            request: Some(listen_handle_request::Request::Init(init_request)),
        })
        .chain(
            ReceiverStream::new(ack_rx).map(|message_id| ListenHandleRequest {
                request: Some(listen_handle_request::Request::Ack(
                    AckListenHandleRequest {
                        message_id: Some(message_id.into()),
                    },
                )),
            }),
        );

        let responses = self
            .as_grpc_client
            .client()
            .listen_handle(requests)
            .await?
            .into_inner();

        let responses = responses.map_while(move |response| {
            let response = response
                .inspect_err(|error| {
                    error!(%error, "stop handle listen stream");
                })
                .ok()?;
            Some(response.message)
        });

        let responder = ListenHandleResponder { tx: ack_tx };

        Ok((responses, responder))
    }

    pub async fn as_as_credentials(&self) -> Result<AsCredentialsResponseIn, AsRequestError> {
        let request = AsCredentialsRequest {};
        let response = self
            .as_grpc_client
            .client()
            .as_credentials(request)
            .await?
            .into_inner();
        Ok(AsCredentialsResponseIn {
            as_credentials: response
                .as_credentials
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    error!(%error, "invalid AS credential");
                    AsRequestError::UnexpectedResponse
                })?,
            as_intermediate_credentials: response
                .as_intermediate_credentials
                .into_iter()
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    error!(%error, "invalid AS intermediate credential");
                    AsRequestError::UnexpectedResponse
                })?,
            revoked_credentials: response
                .revoked_credentials
                .into_iter()
                .map(From::from)
                .collect(),
        })
    }

    pub async fn as_create_handle(
        &self,
        user_handle: &UserHandle,
        hash: UserHandleHash,
        signing_key: &HandleSigningKey,
    ) -> Result<bool, AsRequestError> {
        let payload = CreateHandlePayload {
            verifying_key: Some(signing_key.verifying_key().clone().into()),
            plaintext: user_handle.plaintext().into(),
            hash: Some(hash.into()),
        };
        let request = payload.sign(signing_key)?;
        match self.as_grpc_client.client().create_handle(request).await {
            Ok(_) => Ok(true),
            Err(e) if e.code() == tonic::Code::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn as_delete_handle(
        &self,
        hash: UserHandleHash,
        signing_key: &HandleSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = DeleteHandlePayload {
            hash: Some(hash.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client.client().delete_handle(request).await?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct ListenHandleResponder {
    tx: mpsc::Sender<Uuid>,
}

impl ListenHandleResponder {
    pub async fn ack(&self, message_id: Uuid) {
        let _ = self.tx.send(message_id).await;
    }
}

pub struct ConnectionOfferResponder {
    tx: oneshot::Sender<ConnectionOfferMessage>,
    response: BoxFuture<'static, Result<(), AsRequestError>>,
}

impl ConnectionOfferResponder {
    pub fn new(
        tx: oneshot::Sender<ConnectionOfferMessage>,
        response: impl Future<Output = Result<(), AsRequestError>> + Send + 'static,
    ) -> Self {
        Self {
            tx,
            response: Box::pin(response),
        }
    }

    pub async fn send(self, offer: ConnectionOfferMessage) -> Result<(), AsRequestError> {
        self.tx.send(offer).map_err(|_| {
            error!("failed to send connection offer: connection closed");
            AsRequestError::UnexpectedResponse
        })?;
        self.response.await
    }
}
