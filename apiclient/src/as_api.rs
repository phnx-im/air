// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Client API for the authentication service (AS)

use std::convert::identity;

use aircommon::{
    LibraryError,
    credentials::{
        UserCredentialPayload,
        keys::{UserSigningKey, UsernameSigningKey},
    },
    crypto::{indexed_aead::keys::UserProfileKeyIndex, signatures::signable::Signable},
    identifiers::{UserId, Username, UsernameHash},
    messages::{
        client_as::{
            BatchedTokenKeyResponse, ConnectionOfferMessage, SerializedToken,
            SerializedTokenRequest, SerializedTokenResponse,
        },
        client_as_out::{
            AsCredentialsResponseIn, EncryptedUserProfile, GetUserProfileResponse,
            RegisterUserResponseIn, UsernameDeleteResponse,
        },
        connection_package::ConnectionPackage,
        connection_package::VersionedConnectionPackageIn,
        push_token::{PushToken, PushTokenOperator},
    },
    registration::{ChallengeKind, NewAdmissionSession, RegistrationChallenge, RegistrationInfo},
    time::Duration,
};
use airprotos::{
    auth_service::v1::{
        AckListenUsernameRequest, AdmissionSession as ProtoAdmissionSession, AsCredentialsRequest,
        ChallengeType, CheckInvitationCodeRequest, CheckUsernameExistsRequest,
        ConnectUsernameRequest, ConnectUsernameResponse, CreateAdmissionSessionRequest,
        CreateUsernamePayload, DeleteUserPayload, DeleteUsernamePayload,
        EnqueueConnectionOfferStep, FetchConnectionPackageStep, GetInvitationCodesRequest,
        GetRegistrationInfoRequest, GetUserProfileRequest, InitListenUsernamePayload,
        InvitationCode, IssueTokenBatchPayload, IssueTokenBatchResponse, ListenUsernameRequest,
        MergeUserProfilePayload, OperationType, PublishConnectionPackagesPayload, PushPlatform,
        RefreshUsernamePayload, RegisterUserRequest, RegisterUserResponse, ReportSpamPayload,
        StageUserProfilePayload, UsernameQueueMessage, connect_username_request,
        connect_username_response, issue_token_batch_response, listen_username_request,
        register_user_response,
    },
    common::v1::{StatusDetails, StatusDetailsCode},
};
use futures_util::{FutureExt, future::BoxFuture};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Code, Request, Status};
use tracing::error;
use uuid::Uuid;

use crate::ApiClient;

/// Errors that can occur when sending requests to the AS.
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
    /// Returns whether the error is a gRPC not found error.
    pub fn is_not_found(&self) -> bool {
        match self {
            AsRequestError::Tonic(status) => status.code() == Code::NotFound,
            _ => false,
        }
    }

    /// Returns whether the error is a gRPC failed precondition error with a version unsupported
    /// code.
    pub fn is_unsupported_version(&self) -> bool {
        match self {
            AsRequestError::Tonic(status) => {
                status.code() == Code::FailedPrecondition
                    && StatusDetails::from_status(status)
                        .map(|details| details.code() == StatusDetailsCode::VersionUnsupported)
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Returns true if the token was rejected because the key ID is unknown.
    pub fn is_unknown_token_key_id(&self) -> bool {
        match self {
            AsRequestError::Tonic(status) => {
                status.code() == Code::Unauthenticated
                    && StatusDetails::from_status(status)
                        .map(|d| d.code() == StatusDetailsCode::UnknownTokenKeyId)
                        .unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Returns true if the error is likely due to a network issue and we can't
    /// be sure whether the server received the request.
    pub fn is_network_error(&self) -> bool {
        if let Self::Tonic(status) = self {
            // TODO: Also handle unknown errors here but downcast them to io::Error
            matches!(status.code(), Code::Unavailable | Code::DeadlineExceeded)
        } else {
            false
        }
    }

    /// Returns true if the error means the user exceeded some quota or limit.
    pub fn is_resource_exhausted(&self) -> bool {
        if let Self::Tonic(status) = self {
            matches!(status.code(), Code::ResourceExhausted)
        } else {
            false
        }
    }
}

/// What the server did with a registration.
#[derive(Debug)]
pub enum RegistrationOutcome {
    Registered(RegisterUserResponseIn),
    /// The gate is closed and the request carried no challenge of an accepted
    /// kind. Kinds this build does not know are dropped.
    ChallengeRequired(Vec<ChallengeKind>),
    /// The server turned down the challenge response the request carried.
    ChallengeRejected,
}

impl TryFrom<RegisterUserResponse> for RegistrationOutcome {
    type Error = AsRequestError;

    fn try_from(response: RegisterUserResponse) -> Result<Self, Self::Error> {
        use register_user_response::Outcome;
        match response.outcome {
            Some(Outcome::UserCredential(credential)) => {
                Ok(Self::Registered(RegisterUserResponseIn {
                    user_credential: credential.try_into().map_err(|error| {
                        error!(%error, "invalid user_credential in response");
                        AsRequestError::UnexpectedResponse
                    })?,
                }))
            }
            Some(Outcome::ChallengeRequired(detail)) => Ok(Self::ChallengeRequired(
                detail
                    .accepted_challenges()
                    .filter_map(ChallengeType::known_kind)
                    .collect(),
            )),
            Some(Outcome::ChallengeRejected(_)) => Ok(Self::ChallengeRejected),
            None => {
                error!("missing `outcome` in response");
                Err(AsRequestError::UnexpectedResponse)
            }
        }
    }
}

impl From<LibraryError> for AsRequestError {
    fn from(_: LibraryError) -> Self {
        AsRequestError::LibraryError
    }
}

impl ApiClient {
    pub async fn as_check_invitation_code(&self, code: String) -> Result<bool, AsRequestError> {
        let request = CheckInvitationCodeRequest {
            client_metadata: Some(self.metadata().clone()),
            invitation_code: Some(InvitationCode { code }),
        };
        let response = self
            .as_grpc_client()
            .check_invitation_code(request)
            .await?
            .into_inner();
        Ok(response.is_valid)
    }

    pub async fn as_get_invitation_codes(
        &self,
        tokens: impl IntoIterator<Item = SerializedToken>,
    ) -> Result<Vec<InvitationCode>, AsRequestError> {
        let request = GetInvitationCodesRequest {
            tokens: tokens.into_iter().map(|t| t.into_bytes()).collect(),
        };

        let response = self
            .as_grpc_client()
            .get_invitation_codes(request)
            .await?
            .into_inner();

        Ok(response.invitation_codes)
    }

    /// Asks the server whether registering with it needs a challenge right now.
    pub async fn as_get_registration_info(&self) -> Result<RegistrationInfo, AsRequestError> {
        let request = GetRegistrationInfoRequest {
            client_metadata: Some(self.metadata().clone()),
        };
        let response = self
            .as_grpc_client()
            .get_registration_info(request)
            .await?
            .into_inner();
        Ok(RegistrationInfo {
            challenge_required: response.challenge_required,
            accepted_challenges: response
                .accepted_challenges()
                .filter_map(ChallengeType::known_kind)
                .collect(),
        })
    }

    /// Opens an admission session, which sends a challenge to the push
    /// endpoint. The answer says nothing about whether one is on its way.
    pub async fn as_create_admission_session(
        &self,
        push_token: &PushToken,
    ) -> Result<NewAdmissionSession, AsRequestError> {
        let platform = match push_token.operator() {
            PushTokenOperator::Apple => PushPlatform::Apple,
            PushTokenOperator::Google => PushPlatform::Google,
        };
        let request = CreateAdmissionSessionRequest {
            client_metadata: Some(self.metadata().clone()),
            platform: platform.into(),
            push_token: push_token.token().to_owned(),
        };
        let response = self
            .as_grpc_client()
            .create_admission_session(request)
            .await?
            .into_inner();

        Ok(NewAdmissionSession {
            session_id: response
                .session_id
                .ok_or_else(|| {
                    error!("missing `session_id` in response");
                    AsRequestError::UnexpectedResponse
                })?
                .into(),
            lifetime: Duration::seconds(response.lifetime_seconds.into()),
        })
    }

    pub async fn as_register_user(
        &self,
        client_payload: UserCredentialPayload,
        encrypted_user_profile: EncryptedUserProfile,
        challenge: Option<RegistrationChallenge>,
    ) -> Result<RegistrationOutcome, AsRequestError> {
        let mut invitation_code = None;
        let mut admission_session = None;
        match challenge {
            Some(RegistrationChallenge::InvitationCode(code)) => {
                invitation_code = Some(InvitationCode { code });
            }
            Some(RegistrationChallenge::AdmissionSession(session)) => {
                admission_session = Some(ProtoAdmissionSession {
                    session_id: Some(session.session_id.into()),
                    challenge: session.challenge,
                });
            }
            None => {}
        }
        let request = RegisterUserRequest {
            client_metadata: Some(self.metadata().clone()),
            user_credential_payload: Some(client_payload.into()),
            encrypted_user_profile: Some(encrypted_user_profile.into()),
            invitation_code,
            admission_session,
        };
        self.as_grpc_client()
            .register_user(Request::new(request))
            .await?
            .into_inner()
            .try_into()
    }

    pub async fn as_get_user_profile(
        &self,
        user_id: UserId,
        key_index: UserProfileKeyIndex,
    ) -> Result<GetUserProfileResponse, AsRequestError> {
        let request = GetUserProfileRequest {
            client_metadata: Some(self.metadata().clone()),
            user_id: Some(user_id.into()),
            key_index: key_index.into_bytes().to_vec(),
        };
        let response = self
            .as_grpc_client()
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
        signing_key: &UserSigningKey,
        encrypted_user_profile: EncryptedUserProfile,
    ) -> Result<(), AsRequestError> {
        let payload = StageUserProfilePayload {
            client_metadata: Some(self.metadata().clone()),
            user_id: Some(user_id.into()),
            encrypted_user_profile: Some(encrypted_user_profile.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client().stage_user_profile(request).await?;
        Ok(())
    }

    pub async fn as_merge_user_profile(
        &self,
        user_id: UserId,
        signing_key: &UserSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = MergeUserProfilePayload {
            client_metadata: Some(self.metadata().clone()),
            user_id: Some(user_id.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client().merge_user_profile(request).await?;
        Ok(())
    }

    pub async fn as_delete_user(
        &self,
        user_id: UserId,
        signing_key: &UserSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = DeleteUserPayload {
            client_metadata: Some(self.metadata().clone()),
            user_id: Some(user_id.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client().delete_user(request).await?;
        Ok(())
    }

    pub async fn as_publish_connection_packages_for_username(
        &self,
        hash: UsernameHash,
        connection_packages: Vec<ConnectionPackage>,
        signing_key: &UsernameSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = PublishConnectionPackagesPayload {
            client_metadata: Some(self.metadata().clone()),
            hash: Some(hash.into()),
            connection_packages: connection_packages.into_iter().map(From::from).collect(),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client()
            .publish_connection_packages(request)
            .await?;
        Ok(())
    }

    pub async fn as_report_spam(
        &self,
        reporter_id: UserId,
        spammer_id: UserId,
        signing_key: &UserSigningKey,
    ) -> Result<(), AsRequestError> {
        let payload = ReportSpamPayload {
            client_metadata: Some(self.metadata().clone()),
            reporter_id: Some(reporter_id.into()),
            spammer_id: Some(spammer_id.into()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client().report_spam(request).await?;
        Ok(())
    }

    pub async fn as_connect_username(
        &self,
        hash: UsernameHash,
    ) -> Result<(VersionedConnectionPackageIn, AsConnectionOfferResponder), AsRequestError> {
        // Step 1: Fetch connection package
        let fetch_request = ConnectUsernameRequest {
            step: Some(connect_username_request::Step::Fetch(
                FetchConnectionPackageStep {
                    client_metadata: Some(self.metadata().clone()),
                    hash: Some(hash.into()),
                },
            )),
        };

        // Step 2: Enqueue connection offer
        let (connection_offer_tx, connection_offer_rx) =
            oneshot::channel::<ConnectionOfferMessage>();
        let connection_offer_fut = async move {
            let connection_offer = connection_offer_rx.await.ok()?;
            Some(ConnectUsernameRequest {
                step: Some(connect_username_request::Step::Enqueue(
                    EnqueueConnectionOfferStep {
                        connection_offer: Some(connection_offer.into()),
                    },
                )),
            })
        };

        let requests = tokio_stream::once(Some(fetch_request))
            .chain(connection_offer_fut.into_stream())
            .filter_map(identity);
        let mut responses = self
            .as_grpc_client()
            .connect_username(requests)
            .await?
            .into_inner();

        let response = responses.next().await.ok_or_else(|| {
            error!("protocol violation: missing response");
            AsRequestError::UnexpectedResponse
        })??;

        let connection_package: VersionedConnectionPackageIn = match response {
            ConnectUsernameResponse {
                step: Some(connect_username_response::Step::FetchResponse(fetch)),
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
                ConnectUsernameResponse {
                    step: Some(connect_username_response::Step::EnqueueResponse(_)),
                } => Ok(()),
                _ => {
                    error!("protocol violation: expected connection offer response");
                    Err(AsRequestError::UnexpectedResponse)
                }
            }
        };

        let responder =
            AsConnectionOfferResponder::new(connection_offer_tx, connection_offer_response_fut);
        Ok((connection_package, responder))
    }

    pub async fn as_listen_username(
        &self,
        hash: UsernameHash,
        signing_key: &UsernameSigningKey,
    ) -> Result<
        (
            impl Stream<Item = Result<Option<UsernameQueueMessage>, Status>> + Send + use<>,
            AsListenUsernameResponder,
        ),
        AsRequestError,
    > {
        let init_payload = InitListenUsernamePayload {
            client_metadata: Some(self.metadata().clone()),
            hash: Some(hash.into()),
        };
        let init_request = init_payload.sign(signing_key)?;

        const ACK_CHANNEL_BUFFER_SIZE: usize = 16; // not too big for applying backpressure
        let (ack_tx, ack_rx) = mpsc::channel::<Uuid>(ACK_CHANNEL_BUFFER_SIZE);

        let requests = tokio_stream::once(ListenUsernameRequest {
            request: Some(listen_username_request::Request::Init(init_request)),
        })
        .chain(
            ReceiverStream::new(ack_rx).map(|message_id| ListenUsernameRequest {
                request: Some(listen_username_request::Request::Ack(
                    AckListenUsernameRequest {
                        message_id: Some(message_id.into()),
                    },
                )),
            }),
        );

        let responses = self
            .as_grpc_client()
            .listen_username(requests)
            .await?
            .into_inner();

        let responses = responses.map(|response| response.map(|response| response.message));

        let responder = AsListenUsernameResponder { tx: ack_tx };

        Ok((responses, responder))
    }

    pub async fn as_as_credentials(&self) -> Result<AsCredentialsResponseIn, AsRequestError> {
        let request = AsCredentialsRequest {
            client_metadata: Some(self.metadata().clone()),
        };
        let response = self
            .as_grpc_client()
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
                .map(TryFrom::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    error!(%error, "invalid AS intermediate credential");
                    AsRequestError::UnexpectedResponse
                })?,
            batched_token_keys: response
                .batched_token_keys
                .into_iter()
                .map(|k| {
                    let token_key_id = u8::try_from(k.token_key_id).map_err(|_| {
                        error!(
                            token_key_id = k.token_key_id,
                            "token_key_id does not fit in u8"
                        );
                        AsRequestError::UnexpectedResponse
                    })?;
                    Ok::<_, AsRequestError>(BatchedTokenKeyResponse {
                        operation_type: k.operation_type,
                        token_key_id,
                        public_key: k.public_key,
                        is_current: k.is_current,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    pub async fn as_check_username_exists(
        &self,
        username_hash: UsernameHash,
    ) -> Result<bool, AsRequestError> {
        let request = CheckUsernameExistsRequest {
            client_metadata: Some(self.metadata().clone()),
            hash: Some(username_hash.into()),
        };
        let response = self
            .as_grpc_client()
            .check_username_exists(request)
            .await?
            .into_inner();
        Ok(response.exists)
    }

    pub async fn as_create_username(
        &self,
        username: &Username,
        hash: UsernameHash,
        signing_key: &UsernameSigningKey,
        token: SerializedToken,
    ) -> Result<bool, AsRequestError> {
        let payload = CreateUsernamePayload {
            client_metadata: Some(self.metadata().clone()),
            verifying_key: Some(signing_key.verifying_key().clone().into()),
            plaintext: username.plaintext().into(),
            hash: Some(hash.into()),
            token: Some(token.into_bytes()),
        };
        let request = payload.sign(signing_key)?;
        match self.as_grpc_client().create_username(request).await {
            Ok(_) => Ok(true),
            Err(e) if e.code() == Code::AlreadyExists => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn as_refresh_username(
        &self,
        hash: UsernameHash,
        signing_key: &UsernameSigningKey,
        token: SerializedToken,
    ) -> Result<(), AsRequestError> {
        let payload = RefreshUsernamePayload {
            client_metadata: Some(self.metadata().clone()),
            hash: Some(hash.into()),
            token: Some(token.into_bytes()),
        };
        let request = payload.sign(signing_key)?;
        self.as_grpc_client().refresh_username(request).await?;
        Ok(())
    }

    pub async fn as_delete_username(
        &self,
        hash: UsernameHash,
        signing_key: &UsernameSigningKey,
        token_request: SerializedTokenRequest,
    ) -> Result<(UsernameDeleteResponse, Option<SerializedTokenResponse>), AsRequestError> {
        let payload = DeleteUsernamePayload {
            client_metadata: Some(self.metadata().clone()),
            hash: Some(hash.into()),
            token_request: Some(token_request.into_bytes()),
        };
        let request = payload.sign(signing_key)?;
        let res = self.as_grpc_client().delete_username(request).await;
        match res {
            Ok(response) => {
                let token_response = response
                    .into_inner()
                    .token_response
                    .map(SerializedTokenResponse::new);
                Ok((UsernameDeleteResponse::Success, token_response))
            }
            Err(status) => match status.code() {
                Code::NotFound => Ok((UsernameDeleteResponse::NotFound, None)),
                _ => Err(status.into()),
            },
        }
    }

    /// Requests the full token batch of an allowance epoch.
    ///
    /// The request is derived deterministically from the user's token seed, so
    /// the server answers a repeat of it for free. A conflict or a stale epoch
    /// is an expected outcome of the multi device seed agreement and arrives as
    /// a [`TokenBatchResponse`] variant rather than an error.
    pub async fn as_issue_token_batch(
        &self,
        operation_type: OperationType,
        user_id: UserId,
        signing_key: &UserSigningKey,
        allowance_epoch: u32,
        token_request: SerializedTokenRequest,
    ) -> Result<TokenBatchResponse, AsRequestError> {
        let payload = IssueTokenBatchPayload {
            client_metadata: Some(self.metadata().clone()),
            operation_type: operation_type.into(),
            user_id: Some(user_id.into()),
            token_request: token_request.into_bytes(),
            allowance_epoch,
        };
        let request = payload.sign(signing_key)?;
        let response = self
            .as_grpc_client()
            .issue_token_batch(request)
            .await?
            .into_inner();
        response.try_into()
    }
}

/// Outcome of a token batch issuance request.
#[derive(Debug)]
pub enum TokenBatchResponse {
    /// The serialized token response of the requested batch.
    Issued(SerializedTokenResponse),
    /// A different batch was already issued for this allowance epoch.
    ///
    /// The user's devices derived diverging token seeds.
    IssuanceConflict,
    /// The claimed allowance epoch is not current on the server.
    InvalidAllowanceEpoch { current_epoch: u32 },
}

impl TryFrom<IssueTokenBatchResponse> for TokenBatchResponse {
    type Error = AsRequestError;

    fn try_from(response: IssueTokenBatchResponse) -> Result<Self, Self::Error> {
        use issue_token_batch_response::Outcome;
        match response.outcome {
            Some(Outcome::TokenResponse(bytes)) => Ok(TokenBatchResponse::Issued(
                SerializedTokenResponse::new(bytes),
            )),
            Some(Outcome::IssuanceConflict(_)) => Ok(TokenBatchResponse::IssuanceConflict),
            Some(Outcome::InvalidAllowanceEpoch(detail)) => {
                Ok(TokenBatchResponse::InvalidAllowanceEpoch {
                    current_epoch: detail.current_epoch,
                })
            }
            None => Err(AsRequestError::UnexpectedResponse),
        }
    }
}

/// Sends responses to the AS listening stream.
#[derive(Debug)]
pub struct AsListenUsernameResponder {
    tx: mpsc::Sender<Uuid>,
}

impl AsListenUsernameResponder {
    /// Acknowledges that the client has received the message with the given id.
    ///
    /// The server can safely discard the message.
    pub async fn ack(&self, message_id: Uuid) {
        let _ = self.tx.send(message_id).await;
    }

    /// Half-closes the request stream and waits for the server to confirm that all previously sent
    /// requests were processed by closing the response stream with OK. For this stream, that means
    /// all sent acks are durable.
    ///
    /// `stream` must be the response stream corresponding to this responder.
    pub async fn close(
        self,
        stream: &mut (impl Stream<Item = Result<Option<UsernameQueueMessage>, Status>> + Unpin),
    ) {
        drop(self);
        crate::await_close_confirmation(stream).await;
    }
}

/// Sends a connection offer to the AS in the connect username protocol.
pub struct AsConnectionOfferResponder {
    tx: oneshot::Sender<ConnectionOfferMessage>,
    response: BoxFuture<'static, Result<(), AsRequestError>>,
}

impl AsConnectionOfferResponder {
    fn new(
        tx: oneshot::Sender<ConnectionOfferMessage>,
        response: impl Future<Output = Result<(), AsRequestError>> + Send + 'static,
    ) -> Self {
        Self {
            tx,
            response: Box::pin(response),
        }
    }

    /// Send the connection offer to the AS.
    pub async fn send(self, offer: ConnectionOfferMessage) -> Result<(), AsRequestError> {
        self.tx.send(offer).map_err(|_| {
            error!("failed to send connection offer: connection closed");
            AsRequestError::UnexpectedResponse
        })?;
        self.response.await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use aircommon::registration::ChallengeKind;
    use airprotos::{
        auth_service::v1::{
            ChallengeRejected, ChallengeRequired, ChallengeType, InvalidAllowanceEpoch,
            IssuanceConflict, IssueTokenBatchResponse, RegisterUserResponse,
            issue_token_batch_response::Outcome,
            register_user_response::Outcome as RegistrationOutcomeProto,
        },
        common::v1::{StatusDetails, StatusDetailsCode},
    };
    use tonic::Code;

    use super::{AsRequestError, RegistrationOutcome, TokenBatchResponse};

    fn convert(outcome: Option<Outcome>) -> Result<TokenBatchResponse, AsRequestError> {
        IssueTokenBatchResponse { outcome }.try_into()
    }

    /// Each issuance outcome maps to its own variant. Confusing them would make
    /// the client retry a diverged seed forever, or give up on a recoverable
    /// clock skew.
    #[test]
    fn issuance_outcomes_map_to_their_variants() {
        let issued = convert(Some(Outcome::TokenResponse(vec![1, 2, 3]))).unwrap();
        let TokenBatchResponse::Issued(response) = issued else {
            panic!("expected Issued, got {issued:?}");
        };
        assert_eq!(response.as_bytes(), [1, 2, 3]);

        let conflict = convert(Some(Outcome::IssuanceConflict(IssuanceConflict {}))).unwrap();
        let TokenBatchResponse::IssuanceConflict = conflict else {
            panic!("expected IssuanceConflict, got {conflict:?}");
        };

        let stale = convert(Some(Outcome::InvalidAllowanceEpoch(
            InvalidAllowanceEpoch { current_epoch: 679 },
        )))
        .unwrap();
        let TokenBatchResponse::InvalidAllowanceEpoch { current_epoch } = stale else {
            panic!("expected InvalidAllowanceEpoch, got {stale:?}");
        };
        assert_eq!(current_epoch, 679);
    }

    /// An empty oneof means the server sent something we cannot act on.
    #[test]
    fn missing_outcome_is_an_unexpected_response() {
        let error = convert(None).unwrap_err();
        let AsRequestError::UnexpectedResponse = error else {
            panic!("expected UnexpectedResponse, got {error:?}");
        };
    }

    /// Only a `FailedPrecondition` carrying the version detail counts as an
    /// unsupported version, so unrelated failures are not mistaken for one.
    #[test]
    fn unsupported_version_is_classified_by_its_detail() {
        let version_unsupported = AsRequestError::Tonic(
            StatusDetails {
                code: StatusDetailsCode::VersionUnsupported.into(),
                detail: None,
            }
            .to_status(Code::FailedPrecondition, "rejected"),
        );
        assert!(version_unsupported.is_unsupported_version());

        let unavailable = AsRequestError::Tonic(tonic::Status::unavailable("down"));
        assert!(!unavailable.is_unsupported_version());
    }

    fn convert_registration(
        outcome: Option<RegistrationOutcomeProto>,
    ) -> Result<RegistrationOutcome, AsRequestError> {
        RegisterUserResponse { outcome }.try_into()
    }

    #[test]
    fn registration_outcomes_map_to_their_variants() {
        let required = convert_registration(Some(RegistrationOutcomeProto::ChallengeRequired(
            ChallengeRequired {
                accepted_challenges: vec![ChallengeType::InvitationCode.into()],
            },
        )))
        .unwrap();
        let RegistrationOutcome::ChallengeRequired(accepted) = required else {
            panic!("expected ChallengeRequired, got {required:?}");
        };
        assert_eq!(accepted, [ChallengeKind::InvitationCode]);

        let rejected = convert_registration(Some(RegistrationOutcomeProto::ChallengeRejected(
            ChallengeRejected {},
        )))
        .unwrap();
        let RegistrationOutcome::ChallengeRejected = rejected else {
            panic!("expected ChallengeRejected, got {rejected:?}");
        };
    }

    #[test]
    fn unknown_challenge_kinds_are_dropped() {
        let required = convert_registration(Some(RegistrationOutcomeProto::ChallengeRequired(
            ChallengeRequired {
                accepted_challenges: vec![404, ChallengeType::InvitationCode.into()],
            },
        )))
        .unwrap();
        let RegistrationOutcome::ChallengeRequired(accepted) = required else {
            panic!("expected ChallengeRequired, got {required:?}");
        };
        assert_eq!(accepted, [ChallengeKind::InvitationCode]);
    }

    /// An empty oneof means the server sent something we cannot act on.
    #[test]
    fn missing_registration_outcome_is_an_unexpected_response() {
        let error = convert_registration(None).unwrap_err();
        let AsRequestError::UnexpectedResponse = error else {
            panic!("expected UnexpectedResponse, got {error:?}");
        };
    }
}
