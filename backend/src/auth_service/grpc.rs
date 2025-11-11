// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io;

use airprotos::{
    auth_service::v1::{auth_service_server, *},
    validation::MissingFieldExt,
};
use displaydoc::Display;
use futures_util::stream::BoxStream;

use aircommon::{
    credentials::keys,
    crypto::{
        indexed_aead::keys::UserProfileKeyIndex,
        signatures::{
            private_keys::{SignatureVerificationError, VerifyingKeyBehaviour},
            signable::{Verifiable, VerifiedStruct},
        },
    },
    identifiers,
    messages::{
        client_as::AsCredentialsParams,
        client_as_out::{
            GetUserProfileParams, MergeUserProfileParamsTbs, RegisterUserParamsIn,
            StageUserProfileParamsTbs,
        },
    },
};
use privacypass::{amortized_tokens::AmortizedBatchTokenRequest, private_tokens::Ristretto255};
use tls_codec::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tonic::{Code, Request, Response, Status, Streaming, async_trait};
use tracing::error;

use crate::util::find_cause;

use super::{
    AuthService,
    client_record::ClientRecord,
    user_handles::{ConnectHandleProtocol, UserHandleQueues, UserHandleRecord},
};

pub struct GrpcAs {
    inner: AuthService,
}

impl GrpcAs {
    pub fn new(inner: AuthService) -> Self {
        tracing::debug!("new");
        Self { inner }
    }

    async fn verify_user_auth<R, P>(&self, request: R) -> Result<(identifiers::UserId, P), Status>
    where
        R: WithUserId + Verifiable,
        P: VerifiedStruct<R>,
    {
        let user_id = request.user_id()?;
        let client_verifying_key = self.load_client_verifying_key(&user_id).await?;
        let payload = self.verify_request(request, &client_verifying_key)?;
        Ok((user_id, payload))
    }

    async fn load_client_verifying_key(
        &self,
        user_id: &identifiers::UserId,
    ) -> Result<keys::ClientVerifyingKey, Status> {
        let client_record = ClientRecord::load(&self.inner.db_pool, user_id)
            .await
            .map_err(|error| {
                error!(%error, ?user_id, "failed to load client");
                Status::internal("database error")
            })?
            .ok_or_else(|| Status::not_found("unknown client"))?;
        Ok(client_record.credential.verifying_key().clone())
    }

    async fn verify_handle_auth<R, P>(
        &self,
        request: R,
    ) -> Result<(identifiers::UserHandleHash, P), Status>
    where
        R: WithUserHandleHash + Verifiable,
        P: VerifiedStruct<R>,
    {
        let hash = request.user_handle_hash()?;
        let verifying_key = self.load_handle_verifying_key(hash).await?;
        let payload = self.verify_request(request, &verifying_key)?;
        Ok((hash, payload))
    }

    async fn load_handle_verifying_key(
        &self,
        hash: identifiers::UserHandleHash,
    ) -> Result<keys::HandleVerifyingKey, Status> {
        UserHandleRecord::load_verifying_key(&self.inner.db_pool, &hash)
            .await
            .map_err(|error| {
                error!(%error, "failed to load verifying key");
                Status::internal("database error")
            })?
            .ok_or_else(|| Status::not_found("unknown handle"))
    }

    fn verify_request<R, P>(
        &self,
        request: R,
        verifying_key: impl VerifyingKeyBehaviour,
    ) -> Result<P, Status>
    where
        R: Verifiable,
        P: VerifiedStruct<R>,
    {
        request.verify(verifying_key).map_err(|error| match error {
            SignatureVerificationError::VerificationFailure => {
                Status::unauthenticated("invalid signature")
            }
            SignatureVerificationError::LibraryError(_) => Status::internal("unrecoverable error"),
        })
    }

    async fn process_listen_handle_requests_task(
        queues: UserHandleQueues,
        mut requests: Streaming<ListenHandleRequest>,
    ) {
        while let Some(request) = requests.next().await {
            if let Err(error) = Self::process_listen_handle_request(&queues, request).await {
                if let Code::Unknown = error.code()
                    && let Some(h2_error) = find_cause::<h2::Error>(&error)
                    && let Some(io_error) = h2_error.get_io()
                    && io_error.kind() == io::ErrorKind::BrokenPipe
                {
                    return; // Client closed connection => not an error
                }

                // We report the error, but don't stop processing requests.
                // TODO(#466): Send this to the client.
                error!(%error, "error processing listen request");
            }
        }
    }

    async fn process_listen_handle_request(
        queues: &UserHandleQueues,
        request: Result<ListenHandleRequest, Status>,
    ) -> Result<(), Status> {
        let request = request?;
        let Some(listen_handle_request::Request::Ack(ack_request)) = request.request else {
            return Err(ListenHandleProtocolViolation::OnlyAckRequestAllowed.into());
        };
        let Some(message_id) = ack_request.message_id else {
            return Err(ListenHandleProtocolViolation::MissingMessageId.into());
        };
        queues.ack(message_id.into()).await?;
        Ok(())
    }
}

#[async_trait]
impl auth_service_server::AuthService for GrpcAs {
    async fn register_user(
        &self,
        request: Request<RegisterUserRequest>,
    ) -> Result<Response<RegisterUserResponse>, Status> {
        let request = request.into_inner();
        let params = RegisterUserParamsIn {
            client_payload: request
                .client_credential_payload
                .ok_or_missing_field("client_payload")?
                .try_into()?,
            encrypted_user_profile: request
                .encrypted_user_profile
                .ok_or_missing_field("encrypted_user_profile")?
                .try_into()?,
        };
        let response = self.inner.as_init_user_registration(params).await?;
        Ok(Response::new(RegisterUserResponse {
            client_credential: Some(response.client_credential.into()),
        }))
    }

    async fn delete_user(
        &self,
        request: Request<DeleteUserRequest>,
    ) -> Result<Response<DeleteUserResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, DeleteUserPayload>(request)
            .await?;
        let payload_user_id: identifiers::UserId =
            payload.user_id.ok_or_missing_field("user_id")?.try_into()?;
        if payload_user_id != user_id {
            return Err(Status::invalid_argument("only possible to delete own user"));
        }
        self.inner.as_delete_user(&user_id).await?;
        Ok(Response::new(DeleteUserResponse {}))
    }

    async fn publish_connection_packages(
        &self,
        request: Request<PublishConnectionPackagesRequest>,
    ) -> Result<Response<PublishConnectionPackagesResponse>, Status> {
        let request = request.into_inner();

        let hash = request
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .hash
            .clone()
            .ok_or_missing_field("hash")?;

        let hash: identifiers::UserHandleHash = hash.try_into()?;
        let handle_verifying_key = self.load_handle_verifying_key(hash).await?;
        let payload = self.verify_request::<_, PublishConnectionPackagesPayload>(
            request,
            &handle_verifying_key,
        )?;
        let connection_packages = payload
            .connection_packages
            .into_iter()
            .map(|package| package.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        self.inner
            .as_publish_connection_packages_for_handle(&hash, connection_packages)
            .await?;

        Ok(Response::new(PublishConnectionPackagesResponse {}))
    }

    async fn as_credentials(
        &self,
        _request: Request<AsCredentialsRequest>,
    ) -> Result<Response<AsCredentialsResponse>, Status> {
        let response = self.inner.as_credentials(AsCredentialsParams {}).await?;
        Ok(Response::new(AsCredentialsResponse {
            as_credentials: response
                .as_credentials
                .into_iter()
                .map(From::from)
                .collect(),
            as_intermediate_credentials: response
                .as_intermediate_credentials
                .into_iter()
                .map(From::from)
                .collect(),
            revoked_credentials: response
                .revoked_credentials
                .into_iter()
                .map(From::from)
                .collect(),
        }))
    }

    async fn stage_user_profile(
        &self,
        request: Request<StageUserProfileRequest>,
    ) -> Result<Response<StageUserProfileResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, StageUserProfilePayload>(request)
            .await?;
        let params = StageUserProfileParamsTbs {
            user_id,
            user_profile: payload
                .encrypted_user_profile
                .ok_or_missing_field("encrypted_user_profile")?
                .try_into()?,
        };
        self.inner.as_stage_user_profile(params).await?;
        Ok(Response::new(StageUserProfileResponse {}))
    }

    async fn merge_user_profile(
        &self,
        request: Request<MergeUserProfileRequest>,
    ) -> Result<Response<MergeUserProfileResponse>, Status> {
        let request = request.into_inner();
        let (user_id, _payload) = self
            .verify_user_auth::<_, MergeUserProfilePayload>(request)
            .await?;
        let params = MergeUserProfileParamsTbs { user_id };
        self.inner.as_merge_user_profile(params).await?;
        Ok(Response::new(MergeUserProfileResponse {}))
    }

    async fn get_user_profile(
        &self,
        request: Request<GetUserProfileRequest>,
    ) -> Result<Response<GetUserProfileResponse>, Status> {
        let request = request.into_inner();
        let user_id = request.user_id.ok_or_missing_field("user_id")?.try_into()?;
        let key_index = UserProfileKeyIndex::from_bytes(request.key_index.try_into().map_err(
            |bytes: Vec<u8>| {
                Status::invalid_argument(format!("invalid key index length: {}", bytes.len()))
            },
        )?);
        let params = GetUserProfileParams { user_id, key_index };
        let response = self.inner.as_get_user_profile(params).await?;
        Ok(Response::new(GetUserProfileResponse {
            encrypted_user_profile: Some(response.encrypted_user_profile.into()),
        }))
    }

    async fn issue_tokens(
        &self,
        request: Request<IssueTokensRequest>,
    ) -> Result<Response<IssueTokensResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, IssueTokensPayload>(request)
            .await?;

        let token_request: AmortizedBatchTokenRequest<Ristretto255> =
            AmortizedBatchTokenRequest::tls_deserialize_exact(payload.token_request.as_slice())
                .map_err(|_| Status::invalid_argument("invalid token request"))?;

        let token_response = self
            .inner
            .as_issue_tokens(&user_id, token_request)
            .await?
            .tls_serialize_detached()
            .map_err(|_| Status::internal("failed to serialize token response"))?;

        Ok(Response::new(IssueTokensResponse { token_response }))
    }

    async fn report_spam(
        &self,
        request: Request<ReportSpamRequest>,
    ) -> Result<Response<ReportSpamResponse>, Status> {
        let request = request.into_inner();
        let (_user_id, _payload) = self
            .verify_user_auth::<_, ReportSpamPayload>(request)
            .await?;

        // TODO: forward to the spam reporting service

        Ok(Response::new(ReportSpamResponse {}))
    }

    async fn create_handle(
        &self,
        request: Request<CreateHandleRequest>,
    ) -> Result<Response<CreateHandleResponse>, Status> {
        let request = request.into_inner();

        let verifying_key = request
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .verifying_key
            .clone()
            .ok_or_missing_field("verifying_key")?
            .into();
        let payload = self.verify_request::<_, CreateHandlePayload>(request, &verifying_key)?;

        let hash = payload.hash.ok_or_missing_field("hash")?.try_into()?;

        self.inner
            .as_create_handle(verifying_key, payload.plaintext, hash)
            .await?;

        Ok(Response::new(CreateHandleResponse {}))
    }

    async fn delete_handle(
        &self,
        request: Request<DeleteHandleRequest>,
    ) -> Result<Response<DeleteHandleResponse>, Status> {
        let request = request.into_inner();

        let (hash, _payload) = self
            .verify_handle_auth::<_, DeleteHandlePayload>(request)
            .await?;

        self.inner.as_delete_handle(hash).await?;

        Ok(Response::new(DeleteHandleResponse {}))
    }

    async fn refresh_handle(
        &self,
        request: Request<RefreshHandleRequest>,
    ) -> Result<Response<RefreshHandleResponse>, Status> {
        let request = request.into_inner();

        let (hash, _payload) = self
            .verify_handle_auth::<_, RefreshHandlePayload>(request)
            .await?;

        self.inner.as_refresh_handle(hash).await?;

        Ok(Response::new(RefreshHandleResponse {}))
    }

    type ConnectHandleStream = BoxStream<'static, Result<ConnectResponse, Status>>;

    async fn connect_handle(
        &self,
        request: Request<Streaming<ConnectRequest>>,
    ) -> Result<Response<Self::ConnectHandleStream>, Status> {
        let incoming = request.into_inner();
        let (outgoing_tx, outgoing_rx) = mpsc::channel(1);

        // protocol
        tokio::spawn(
            self.inner
                .clone()
                .connect_handle_protocol(incoming, outgoing_tx),
        );

        let outgoing = tokio_stream::wrappers::ReceiverStream::new(outgoing_rx);
        Ok(Response::new(Box::pin(outgoing)))
    }

    type ListenHandleStream = BoxStream<'static, Result<ListenHandleResponse, Status>>;

    async fn listen_handle(
        &self,
        request: Request<Streaming<ListenHandleRequest>>,
    ) -> Result<Response<Self::ListenHandleStream>, Status> {
        let mut requests = request.into_inner();

        let request = requests
            .next()
            .await
            .ok_or(ListenHandleProtocolViolation::MissingInitRequest)??;
        let Some(listen_handle_request::Request::Init(init_request)) = request.request else {
            return Err(ListenHandleProtocolViolation::MissingInitRequest.into());
        };

        let (hash, _payload) = self
            .verify_handle_auth::<_, InitListenHandlePayload>(init_request)
            .await?;

        let messages = self.inner.handle_queues.listen(hash).await?;

        tokio::spawn(Self::process_listen_handle_requests_task(
            self.inner.handle_queues.clone(),
            requests,
        ));

        let responses = Box::pin(messages.map(|message| Ok(ListenHandleResponse { message })));

        Ok(Response::new(responses))
    }
}

#[derive(Debug, thiserror::Error, Display)]
enum ListenHandleProtocolViolation {
    /// Missing initial request
    MissingInitRequest,
    /// Only ack request allowed
    OnlyAckRequestAllowed,
    /// Missing message id in ack request
    MissingMessageId,
}

impl From<ListenHandleProtocolViolation> for Status {
    fn from(error: ListenHandleProtocolViolation) -> Self {
        Status::failed_precondition(error.to_string())
    }
}

trait WithUserId {
    fn user_id_proto(&self) -> Option<UserId>;

    fn user_id(&self) -> Result<identifiers::UserId, Status> {
        Ok(self
            .user_id_proto()
            .ok_or_missing_field("user_id")?
            .try_into()?)
    }
}

impl WithUserId for DeleteUserRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.user_id.clone()
    }
}

impl WithUserId for StageUserProfileRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.user_id.clone()
    }
}

impl WithUserId for MergeUserProfileRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.user_id.clone()
    }
}

impl WithUserId for IssueTokensRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.user_id.clone()
    }
}

impl WithUserId for ReportSpamRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.reporter_id.clone()
    }
}

trait WithUserHandleHash {
    fn user_handle_hash_proto(&self) -> Option<UserHandleHash>;

    fn user_handle_hash(&self) -> Result<identifiers::UserHandleHash, Status> {
        Ok(self
            .user_handle_hash_proto()
            .ok_or_missing_field("user_handle_hash")?
            .try_into()?)
    }
}

impl WithUserHandleHash for CreateHandleRequest {
    fn user_handle_hash_proto(&self) -> Option<UserHandleHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

impl WithUserHandleHash for DeleteHandleRequest {
    fn user_handle_hash_proto(&self) -> Option<UserHandleHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

impl WithUserHandleHash for RefreshHandleRequest {
    fn user_handle_hash_proto(&self) -> Option<UserHandleHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

impl WithUserHandleHash for InitListenHandleRequest {
    fn user_handle_hash_proto(&self) -> Option<UserHandleHash> {
        self.payload.as_ref()?.hash.clone()
    }
}
