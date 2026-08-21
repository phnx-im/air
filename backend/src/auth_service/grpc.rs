// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, io};

use airprotos::{
    auth_service::v1::{auth_service_server, *},
    common::v1::{ClientMetadata, UserId},
    signed::{SignedRequest, VerifiableRequest},
    validation::MissingFieldExt,
};
use chrono::Utc;
use displaydoc::Display;
use futures_util::stream::BoxStream;
use metrics::counter;

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
    utils::CancellableStream,
};
use privacypass::{
    amortized_tokens::{AmortizedBatchTokenRequest, AmortizedToken},
    private_tokens::Ristretto255,
};
use prost::Message;
use semver::Version;
use tls_codec::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::{StreamExt, wrappers::ReceiverStream};
use tonic::{Code, Request, Response, Status, Streaming, async_trait};
use tracing::{error, warn};

use crate::{
    auth_service::{
        client_api::{privacy_pass::TokenBatchIssuance, user::RegistrationOutcome},
        invitation_code_record::{CODES_PER_DAY, InvitationCodeRecord},
        registration_gate::GateDecision,
        usernames::ConnectUsernameProtocol,
    },
    client_ip::ClientIp,
    errors::auth_service::RedeemTokenError,
    util::{find_cause, select_until_first_ends},
};

use super::{
    AuthService,
    usernames::{UsernameQueues, UsernameRecord},
};

pub struct GrpcAs {
    inner: AuthService,
}

impl GrpcAs {
    pub fn new(inner: AuthService) -> Self {
        Self { inner }
    }

    async fn verify_user_auth<R, P, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
    ) -> Result<(identifiers::UserId, P), Status>
    where
        R: WithUserId + VerifiableRequest,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let user_id = request.inner().user_id()?;
        let client_verifying_key = self.load_client_verifying_key(&user_id).await?;
        let payload = self.verify_request(request, &client_verifying_key)?;
        Ok((user_id, payload))
    }

    async fn load_client_verifying_key(
        &self,
        user_id: &identifiers::UserId,
    ) -> Result<keys::ClientVerifyingKey, Status> {
        self.inner
            .load_client_verifying_key(user_id)
            .await
            .map_err(|error| {
                error!(%error, ?user_id, "failed to load client");
                Status::internal("database error")
            })?
            .ok_or_else(|| Status::not_found("unknown client"))
    }

    async fn verify_username_auth<R, P, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
    ) -> Result<(identifiers::UsernameHash, P), Status>
    where
        R: WithUsernameHash + VerifiableRequest + fmt::Debug,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let hash = request.inner().username_hash()?;
        let verifying_key = self.load_username_verifying_key(hash).await?;
        let payload = self.verify_request(request, &verifying_key)?;
        Ok((hash, payload))
    }

    async fn load_username_verifying_key(
        &self,
        hash: identifiers::UsernameHash,
    ) -> Result<keys::UsernameVerifyingKey, Status> {
        UsernameRecord::load_verifying_key(&self.inner.db_pool, &hash)
            .await
            .map_err(|error| {
                error!(%error, "failed to load verifying key");
                Status::internal("database error")
            })?
            .ok_or_else(|| Status::not_found("unknown username"))
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

    async fn process_listen_username_requests_task(
        queues: UsernameQueues,
        mut requests: Streaming<ListenUsernameRequest>,
        responses_tx: mpsc::Sender<Status>,
    ) {
        while let Some(request) = requests.next().await {
            if let Err(error) = Self::process_listen_username_request(&queues, request).await {
                if let Code::Unknown = error.code()
                    && let Some(h2_error) = find_cause::<h2::Error>(&error)
                    && let Some(io_error) = h2_error.get_io()
                    && io_error.kind() == io::ErrorKind::BrokenPipe
                {
                    // Client closed connection => not an error
                    continue;
                } else {
                    // We report the error to the client, but don't stop processing requests.
                    error!(%error, "error processing listen handle request");
                    let _ = responses_tx.send(error).await;
                }
            }
        }
    }

    async fn process_listen_username_request(
        queues: &UsernameQueues,
        request: Result<ListenUsernameRequest, Status>,
    ) -> Result<(), Status> {
        let request = request?;
        let Some(listen_username_request::Request::Ack(ack_request)) = request.request else {
            return Err(ListenHandleProtocolViolation::OnlyAckRequestAllowed.into());
        };
        let Some(message_id) = ack_request.message_id else {
            return Err(ListenHandleProtocolViolation::MissingMessageId.into());
        };
        queues.ack(message_id.into()).await?;
        Ok(())
    }

    fn verify_client_version(
        &self,
        client_metadata: Option<&ClientMetadata>,
    ) -> Result<Option<Version>, Status> {
        let client_version_req = self.inner.client_version_req.as_ref();
        crate::version::verify_client_version(client_version_req, client_metadata)
    }

    /// The challenge kinds this server verifies, on the wire.
    fn challenge_types(&self) -> Vec<i32> {
        self.inner
            .accepted_challenges()
            .into_iter()
            .map(|kind| ChallengeType::from(kind).into())
            .collect()
    }

    /// Turns a registration down for carrying no challenge the gate accepts.
    fn challenge_required(&self) -> RegisterUserResponse {
        RegisterUserResponse {
            outcome: Some(register_user_response::Outcome::ChallengeRequired(
                ChallengeRequired {
                    accepted_challenges: self.challenge_types(),
                },
            )),
        }
    }
}

#[async_trait]
impl auth_service_server::AuthService for GrpcAs {
    async fn check_invitation_code(
        &self,
        request: Request<CheckInvitationCodeRequest>,
    ) -> Result<Response<CheckInvitationCodeResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;

        let code = request
            .invitation_code
            .ok_or_missing_field("invitation_code")?;

        if !InvitationCodeRecord::validate_code(&code.code) {
            return Err(Status::invalid_argument("invalid invitation code"));
        }

        if self.inner.unredeemable_code.as_deref() == Some(&code.code) {
            counter!("air_invitation_codes_checked_total", "is_valid" => "true").increment(1);
            return Ok(Response::new(CheckInvitationCodeResponse {
                is_valid: true,
            }));
        }

        let record = InvitationCodeRecord::load(&self.inner.db_pool, &code.code)
            .await
            .map_err(|error| {
                error!(%error, "failed to load invitation code");
                Status::internal("database error")
            })?;

        let is_valid = record.filter(|r| !r.redeemed).is_some();

        counter!(
            "air_invitation_codes_checked_total",
            "is_valid" => if is_valid { "true" } else { "false" },
        )
        .increment(1);

        Ok(Response::new(CheckInvitationCodeResponse { is_valid }))
    }

    async fn get_invitation_codes(
        &self,
        request: Request<GetInvitationCodesRequest>,
    ) -> Result<Response<GetInvitationCodesResponse>, Status> {
        // note: this endpoint is anonymous by design
        let request = request.into_inner();

        // Check len of request.tokens
        if request.tokens.len() > 10 {
            return Err(Status::invalid_argument("too many tokens requested"));
        }

        let tokens: Result<Vec<_>, _> = request
            .tokens
            .into_iter()
            .map(|bytes| AmortizedToken::<Ristretto255>::tls_deserialize_exact(bytes.as_slice()))
            .collect();

        let tokens = tokens.map_err(|error| {
            warn!(%error, "failed to deserialise token");
            Status::invalid_argument("invalid token")
        })?;

        let mut txn = self.inner.db_pool.begin().await.map_err(|error| {
            error!(%error, "failed to start txn");
            Status::internal("database error")
        })?;

        let codes_today = InvitationCodeRecord::lock_and_count_codes_issued_today(&mut txn)
            .await
            .map_err(|error| {
                error!(%error, "failed to lock table and count codes issued today");
                Status::internal("database error")
            })?;

        if codes_today + (tokens.len() as u64) > CODES_PER_DAY {
            return Err(Status::resource_exhausted("too many codes generated today"));
        }

        let mut redeem_error: Option<RedeemTokenError> = None;
        let mut invitation_codes = Vec::new();
        for token in tokens {
            // redeem the token
            if let Err(error) = self
                .inner
                .as_redeem_token(txn.as_mut(), token, OperationType::GetInviteCode)
                .await
            {
                warn!(%error, "failed to redeem token to get invitation code");
                redeem_error = Some(error);
                continue;
            }

            // if the token could be redeemed, issue a new invite code
            let code = InvitationCodeRecord::generate(txn.as_mut())
                .await
                .map_err(|error| {
                    error!(%error, "database error");
                    Status::internal("database error")
                })?;

            invitation_codes.push(InvitationCode { code });
            counter!("air_invitation_codes_issued_total").increment(1);
        }

        // We only fail when nothing at all could be issued: the client sends a single token and
        // relies on an error status to classify it as burned and drop it. Returning before the
        // commit also rolls back the nonce state of the failed redemptions.
        if invitation_codes.is_empty()
            && let Some(error) = redeem_error
        {
            return Err(error.into());
        }

        txn.commit().await.map_err(|error| {
            error!(%error, "failed to commit transaction");
            Status::internal("database error")
        })?;

        Ok(Response::new(GetInvitationCodesResponse {
            invitation_codes,
        }))
    }

    async fn register_user(
        &self,
        request: Request<RegisterUserRequest>,
    ) -> Result<Response<RegisterUserResponse>, Status> {
        use register_user_response::Outcome;

        let client_ip = request.extensions().get::<ClientIp>().copied();
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;

        let decision = self
            .inner
            .registration_gate()
            .admit(&self.inner.db_pool, client_ip)
            .await;
        report_gate_reason(decision);

        // A response the gate did not ask for is left untouched, so a gate that
        // opens between discovery and registration does not spend the code the
        // client already entered.
        let challenge = if decision.challenge_required() {
            let Some(challenge) = self.inner.select_challenge(&request) else {
                return Ok(Response::new(self.challenge_required()));
            };
            Some(challenge)
        } else {
            None
        };
        let gated = challenge.is_some();

        let params = RegisterUserParamsIn {
            client_payload: request
                .user_credential_payload
                .ok_or_missing_field("client_payload")?
                .try_into()?,
            encrypted_user_profile: request
                .encrypted_user_profile
                .ok_or_missing_field("encrypted_user_profile")?
                .try_into()?,
        };
        let outcome = self
            .inner
            .as_init_user_registration(params, challenge)
            .await?;

        let outcome = match outcome {
            RegistrationOutcome::Registered(response) => {
                counter!("air_registrations_total", "gated" => gated.to_string()).increment(1);
                Outcome::UserCredential(response.user_credential.into())
            }
            RegistrationOutcome::ChallengeRejected => {
                Outcome::ChallengeRejected(ChallengeRejected {})
            }
        };

        Ok(Response::new(RegisterUserResponse {
            outcome: Some(outcome),
        }))
    }

    async fn get_registration_info(
        &self,
        request: Request<GetRegistrationInfoRequest>,
    ) -> Result<Response<GetRegistrationInfoResponse>, Status> {
        let client_ip = request.extensions().get::<ClientIp>().copied();

        let decision = self
            .inner
            .registration_gate()
            .decide(&self.inner.db_pool, client_ip)
            .await;

        Ok(Response::new(GetRegistrationInfoResponse {
            challenge_required: decision.challenge_required(),
            accepted_challenges: self.challenge_types(),
        }))
    }

    async fn delete_user(
        &self,
        request: Request<SignedRequest<DeleteUserRequest>>,
    ) -> Result<Response<DeleteUserResponse>, Status> {
        let signed_request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, DeleteUserPayload, _>(signed_request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;
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
        request: Request<SignedRequest<PublishConnectionPackagesRequest>>,
    ) -> Result<Response<PublishConnectionPackagesResponse>, Status> {
        let request = request.into_inner();

        let hash = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .hash
            .clone()
            .ok_or_missing_field("hash")?;

        let hash: identifiers::UsernameHash = hash.try_into()?;
        let username_verifying_key = self.load_username_verifying_key(hash).await?;
        let payload = self.verify_request::<_, PublishConnectionPackagesPayload>(
            request,
            &username_verifying_key,
        )?;
        self.verify_client_version(payload.client_metadata.as_ref())?;
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
        request: Request<AsCredentialsRequest>,
    ) -> Result<Response<AsCredentialsResponse>, Status> {
        self.verify_client_version(request.into_inner().client_metadata.as_ref())?;
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
            batched_token_keys: response
                .batched_token_keys
                .into_iter()
                .map(|k| BatchedTokenKey {
                    token_key_id: k.token_key_id.into(),
                    public_key: k.public_key,
                    operation_type: k.operation_type,
                    is_current: k.is_current,
                })
                .collect(),
        }))
    }

    async fn stage_user_profile(
        &self,
        request: Request<SignedRequest<StageUserProfileRequest>>,
    ) -> Result<Response<StageUserProfileResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, StageUserProfilePayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;
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
        request: Request<SignedRequest<MergeUserProfileRequest>>,
    ) -> Result<Response<MergeUserProfileResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, MergeUserProfilePayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;
        let params = MergeUserProfileParamsTbs { user_id };
        self.inner.as_merge_user_profile(params).await?;
        Ok(Response::new(MergeUserProfileResponse {}))
    }

    async fn get_user_profile(
        &self,
        request: Request<GetUserProfileRequest>,
    ) -> Result<Response<GetUserProfileResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;
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
        request: Request<SignedRequest<IssueTokensRequest>>,
    ) -> Result<Response<IssueTokensResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, IssueTokensPayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let operation_type = payload
            .operation_type
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid operation type"))?;

        let token_request: AmortizedBatchTokenRequest<Ristretto255> =
            AmortizedBatchTokenRequest::tls_deserialize_exact(payload.token_request.as_slice())
                .map_err(|_| Status::invalid_argument("invalid token request"))?;

        let token_response = self
            .inner
            .as_issue_tokens(&user_id, operation_type, token_request, Utc::now())
            .await?
            .tls_serialize_detached()
            .map_err(|_| Status::internal("failed to serialize token response"))?;

        Ok(Response::new(IssueTokensResponse { token_response }))
    }

    async fn issue_token_batch(
        &self,
        request: Request<SignedRequest<IssueTokenBatchRequest>>,
    ) -> Result<Response<IssueTokenBatchResponse>, Status> {
        let request = request.into_inner();
        let (user_id, payload) = self
            .verify_user_auth::<_, IssueTokenBatchPayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let operation_type = payload
            .operation_type
            .try_into()
            .map_err(|_| Status::invalid_argument("invalid operation type"))?;

        let outcome = match self
            .inner
            .as_issue_token_batch(
                &user_id,
                operation_type,
                payload.allowance_epoch,
                &payload.token_request,
                Utc::now(),
            )
            .await?
        {
            TokenBatchIssuance::Issued(token_response) => {
                let token_response = token_response
                    .tls_serialize_detached()
                    .map_err(|_| Status::internal("failed to serialize token response"))?;
                issue_token_batch_response::Outcome::TokenResponse(token_response)
            }
            TokenBatchIssuance::Conflict => {
                issue_token_batch_response::Outcome::IssuanceConflict(IssuanceConflict {})
            }
            TokenBatchIssuance::InvalidEpoch { current_epoch } => {
                issue_token_batch_response::Outcome::InvalidAllowanceEpoch(InvalidAllowanceEpoch {
                    current_epoch,
                })
            }
        };

        Ok(Response::new(IssueTokenBatchResponse {
            outcome: Some(outcome),
        }))
    }

    async fn report_spam(
        &self,
        request: Request<SignedRequest<ReportSpamRequest>>,
    ) -> Result<Response<ReportSpamResponse>, Status> {
        let request = request.into_inner();
        let (_user_id, payload) = self
            .verify_user_auth::<_, ReportSpamPayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        // TODO: forward to the spam reporting service

        Ok(Response::new(ReportSpamResponse {}))
    }

    async fn check_username_exists(
        &self,
        request: Request<CheckUsernameExistsRequest>,
    ) -> Result<Response<CheckUsernameExistsResponse>, Status> {
        let request = request.into_inner();
        self.verify_client_version(request.client_metadata.as_ref())?;
        let hash = request.hash.ok_or_missing_field("hash")?.try_into()?;

        let exists = self.inner.as_check_username_exists(&hash).await?;

        Ok(Response::new(CheckUsernameExistsResponse { exists }))
    }

    async fn create_username(
        &self,
        request: Request<SignedRequest<CreateUsernameRequest>>,
    ) -> Result<Response<CreateUsernameResponse>, Status> {
        let request = request.into_inner();

        let verifying_key = request
            .inner()
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?
            .verifying_key
            .clone()
            .ok_or_missing_field("verifying_key")?
            .into();
        let payload = self.verify_request::<_, CreateUsernamePayload>(request, &verifying_key)?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let hash = payload.hash.ok_or_missing_field("hash")?.try_into()?;

        let token = payload
            .token
            .map(|bytes| AmortizedToken::<Ristretto255>::tls_deserialize_exact(bytes.as_slice()))
            .transpose()
            .map_err(|_| Status::invalid_argument("invalid token"))?;

        self.inner
            .as_create_username(verifying_key, payload.plaintext, hash, token)
            .await?;

        Ok(Response::new(CreateUsernameResponse {}))
    }

    async fn delete_username(
        &self,
        request: Request<SignedRequest<DeleteUsernameRequest>>,
    ) -> Result<Response<DeleteUsernameResponse>, Status> {
        let request = request.into_inner();

        let (hash, payload) = self
            .verify_username_auth::<_, DeleteUsernamePayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let token_request = payload
            .token_request
            .map(|bytes| {
                AmortizedBatchTokenRequest::<Ristretto255>::tls_deserialize_exact(bytes.as_slice())
            })
            .transpose()
            .map_err(|_| Status::invalid_argument("invalid token request"))?;

        let token_response = self.inner.as_delete_username(hash, token_request).await?;

        let token_response_bytes = token_response
            .map(|resp| resp.tls_serialize_detached())
            .transpose()
            .map_err(|_| Status::internal("failed to serialize token response"))?;

        Ok(Response::new(DeleteUsernameResponse {
            token_response: token_response_bytes,
        }))
    }

    async fn refresh_username(
        &self,
        request: Request<SignedRequest<RefreshUsernameRequest>>,
    ) -> Result<Response<RefreshUsernameResponse>, Status> {
        let request = request.into_inner();

        let (hash, payload) = self
            .verify_username_auth::<_, RefreshUsernamePayload, _>(request)
            .await?;
        self.verify_client_version(payload.client_metadata.as_ref())?;

        let token = payload
            .token
            .map(|bytes| AmortizedToken::<Ristretto255>::tls_deserialize_exact(bytes.as_slice()))
            .transpose()
            .map_err(|_| Status::invalid_argument("invalid token"))?;

        self.inner.as_refresh_username(hash, token).await?;

        Ok(Response::new(RefreshUsernameResponse {}))
    }

    type ConnectUsernameStream = BoxStream<'static, Result<ConnectUsernameResponse, Status>>;

    async fn connect_username(
        &self,
        request: Request<Streaming<ConnectUsernameRequest>>,
    ) -> Result<Response<Self::ConnectUsernameStream>, Status> {
        let incoming = request.into_inner();
        let (outgoing_tx, outgoing_rx) = mpsc::channel(1);

        // protocol
        tokio::spawn(
            self.inner
                .clone()
                .connect_username_protocol(incoming, outgoing_tx),
        );

        let outgoing = tokio_stream::wrappers::ReceiverStream::new(outgoing_rx);
        Ok(Response::new(Box::pin(outgoing)))
    }

    type ListenUsernameStream = BoxStream<'static, Result<ListenUsernameResponse, Status>>;

    async fn listen_username(
        &self,
        request: Request<Streaming<ListenUsernameRequest>>,
    ) -> Result<Response<Self::ListenUsernameStream>, Status> {
        let mut requests = request.into_inner();

        let request = requests
            .next()
            .await
            .ok_or(ListenHandleProtocolViolation::MissingInitRequest)??;
        let Some(listen_username_request::Request::Init(init_request)) = request.request else {
            return Err(ListenHandleProtocolViolation::MissingInitRequest.into());
        };

        let payload = init_request
            .payload
            .as_ref()
            .ok_or_missing_field("payload")?;
        self.verify_client_version(payload.client_metadata.as_ref())?;
        let init_payload_bytes = payload.encode_to_vec();
        let signed_request: SignedRequest<InitListenUsernameRequest> =
            SignedRequest::new(init_request, init_payload_bytes.into());
        let (hash, _payload) = self
            .verify_username_auth::<_, InitListenUsernamePayload, _>(signed_request)
            .await?;

        let messages = self.inner.username_queues.listen(hash).await?;

        const REQUESTS_RESPONSE_CHANNEL_BUFFER_SIZE: usize = 16; // not too big for applying backpressure
        let (requests_responses_tx, requests_responses_rx) =
            mpsc::channel::<Status>(REQUESTS_RESPONSE_CHANNEL_BUFFER_SIZE);

        tokio::spawn(self.inner.stop.clone().run_until_cancelled_owned(
            Self::process_listen_username_requests_task(
                self.inner.username_queues.clone(),
                requests,
                requests_responses_tx,
            ),
        ));

        let responses = select_until_first_ends(
            CancellableStream::new(messages, self.inner.stop.clone())
                .map(|message| Ok(ListenUsernameResponse { message })),
            ReceiverStream::new(requests_responses_rx).map(Err),
        );

        Ok(Response::new(Box::pin(responses)))
    }
}

/// Publishes why the registration gate closed for this request. Whether the
/// decision is enforced is up to the caller.
fn report_gate_reason(decision: GateDecision) {
    if let GateDecision::ChallengeRequired(reason) = decision {
        counter!("air_registration_challenge_required_total", "reason" => reason.as_str())
            .increment(1);
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

impl WithUserId for IssueTokenBatchRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.user_id.clone()
    }
}

impl WithUserId for ReportSpamRequest {
    fn user_id_proto(&self) -> Option<UserId> {
        self.payload.as_ref()?.reporter_id.clone()
    }
}

trait WithUsernameHash {
    fn username_hash_proto(&self) -> Option<UsernameHash>;

    fn username_hash(&self) -> Result<identifiers::UsernameHash, Status> {
        Ok(self
            .username_hash_proto()
            .ok_or_missing_field("username_hash")?
            .try_into()?)
    }
}

impl WithUsernameHash for CreateUsernameRequest {
    fn username_hash_proto(&self) -> Option<UsernameHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

impl WithUsernameHash for DeleteUsernameRequest {
    fn username_hash_proto(&self) -> Option<UsernameHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

impl WithUsernameHash for RefreshUsernameRequest {
    fn username_hash_proto(&self) -> Option<UsernameHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

impl WithUsernameHash for InitListenUsernameRequest {
    fn username_hash_proto(&self) -> Option<UsernameHash> {
        self.payload.as_ref()?.hash.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use airprotos::{
        auth_service::v1::{
            GetInvitationCodesRequest, OperationType,
            auth_service_server::AuthService as AuthServiceRpc,
        },
        common::v1::{StatusDetails, StatusDetailsCode},
    };
    use chrono::Utc;
    use privacypass::{
        amortized_tokens::AmortizedBatchTokenRequest,
        auth::authenticate::TokenChallenge,
        common::private::{PrivateCipherSuite, PublicKey, deserialize_public_key},
        private_tokens::Ristretto255,
    };
    use sqlx::PgPool;
    use tls_codec::Serialize;
    use tokio_util::sync::CancellationToken;
    use tonic::{Code, Request};

    use crate::{
        air_service::BackendService,
        auth_service::{
            AuthService, client_record::persistence::tests::store_random_client_record,
            privacy_pass::load_batched_token_keys,
            user_record::persistence::tests::store_random_user_record,
        },
        settings::{RegistrationPolicy, RegistrationSettings},
    };

    use super::*;

    /// Creates an AuthService (which bootstraps the VOPRF keys) and returns the
    /// public key used for `GetInviteCode` tokens.
    async fn setup(pool: &PgPool) -> anyhow::Result<(AuthService, PublicKey<Ristretto255>)> {
        let service = AuthService::initialize(
            pool.clone(),
            "example.com".parse()?,
            None,
            CancellationToken::new(),
        )
        .await?;

        let public_key = load_batched_token_keys(pool)
            .await?
            .into_iter()
            .find(|record| record.operation_type == OperationType::GetInviteCode)
            .map(|record| deserialize_public_key::<Ristretto255>(&record.public_key))
            .expect("no GetInviteCode key")?;

        Ok((service, public_key))
    }

    fn build_challenge() -> TokenChallenge {
        TokenChallenge::new(
            Ristretto255::token_type(),
            "example.com",
            None,
            &["example.com".to_string()],
        )
    }

    /// Issues a single `GetInviteCode` token for a fresh user and returns its
    /// wire encoding. The allowance is one token per user per day, so every
    /// token needs its own user.
    async fn mint_token(
        service: &AuthService,
        public_key: PublicKey<Ristretto255>,
        pool: &PgPool,
    ) -> anyhow::Result<Vec<u8>> {
        let user_record = store_random_user_record(pool).await?;
        store_random_client_record(pool, user_record.user_id().clone()).await?;

        let challenge = build_challenge();
        let (token_request, token_state) =
            AmortizedBatchTokenRequest::<Ristretto255>::new(public_key, &challenge, 1)?;

        let token_response = service
            .as_issue_tokens(
                user_record.user_id(),
                OperationType::GetInviteCode,
                token_request,
                Utc::now(),
            )
            .await?;

        let token = token_response
            .issue_tokens(&token_state)?
            .into_iter()
            .next()
            .expect("no token issued");

        Ok(token.tls_serialize_detached()?)
    }

    async fn get_invitation_codes(
        grpc_as: &GrpcAs,
        tokens: Vec<Vec<u8>>,
    ) -> Result<Vec<String>, tonic::Status> {
        let response = grpc_as
            .get_invitation_codes(Request::new(GetInvitationCodesRequest { tokens }))
            .await?;
        Ok(response
            .into_inner()
            .invitation_codes
            .into_iter()
            .map(|code| code.code)
            .collect())
    }

    #[sqlx::test]
    async fn get_invitation_codes_success(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup(&pool).await?;
        let token = mint_token(&service, public_key, &pool).await?;
        let grpc_as = GrpcAs::new(service);

        let codes = get_invitation_codes(&grpc_as, vec![token]).await?;
        assert_eq!(codes.len(), 1);

        Ok(())
    }

    /// A token signed with a key that was rotated out is rejected with a status
    /// detail telling the client to fetch new keys.
    #[sqlx::test]
    async fn get_invitation_codes_stale_key_errors(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup(&pool).await?;
        let token = mint_token(&service, public_key, &pool).await?;

        sqlx::query("DELETE FROM as_batched_key WHERE operation_type = $1")
            .bind(OperationType::GetInviteCode as i16)
            .execute(&pool)
            .await?;

        let grpc_as = GrpcAs::new(service);
        let status = get_invitation_codes(&grpc_as, vec![token])
            .await
            .expect_err("stale key should be rejected");

        assert_eq!(status.code(), Code::Unauthenticated);
        let details = StatusDetails::from_status(&status).expect("no status details");
        assert_eq!(details.code(), StatusDetailsCode::UnknownTokenKeyId);

        Ok(())
    }

    #[sqlx::test]
    async fn get_invitation_codes_double_spend_errors(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup(&pool).await?;
        let token = mint_token(&service, public_key, &pool).await?;
        let grpc_as = GrpcAs::new(service);

        let codes = get_invitation_codes(&grpc_as, vec![token.clone()]).await?;
        assert_eq!(codes.len(), 1);

        let status = get_invitation_codes(&grpc_as, vec![token])
            .await
            .expect_err("double spend should be rejected");

        assert_eq!(status.code(), Code::Unauthenticated);
        assert_ne!(
            StatusDetails::from_status(&status).map(|details| details.code()),
            Some(StatusDetailsCode::UnknownTokenKeyId)
        );

        Ok(())
    }

    /// One spent token among valid ones does not fail the whole request.
    #[sqlx::test]
    async fn get_invitation_codes_partial_success(pool: PgPool) -> anyhow::Result<()> {
        let (service, public_key) = setup(&pool).await?;
        let valid_token = mint_token(&service, public_key, &pool).await?;
        let spent_token = mint_token(&service, public_key, &pool).await?;
        let grpc_as = GrpcAs::new(service);

        let codes = get_invitation_codes(&grpc_as, vec![spent_token.clone()]).await?;
        assert_eq!(codes.len(), 1);

        let codes = get_invitation_codes(&grpc_as, vec![valid_token, spent_token]).await?;
        assert_eq!(codes.len(), 1);

        Ok(())
    }

    #[sqlx::test]
    async fn get_invitation_codes_no_tokens_ok(pool: PgPool) -> anyhow::Result<()> {
        let (service, _public_key) = setup(&pool).await?;
        let grpc_as = GrpcAs::new(service);

        let codes = get_invitation_codes(&grpc_as, Vec::new()).await?;
        assert!(codes.is_empty());

        Ok(())
    }

    async fn registration_info(
        pool: &PgPool,
        policy: RegistrationPolicy,
        client_ip: Option<ClientIp>,
    ) -> anyhow::Result<GetRegistrationInfoResponse> {
        let (mut service, _public_key) = setup(pool).await?;
        service.set_registration_settings(RegistrationSettings {
            policy,
            ..Default::default()
        });
        let grpc_as = GrpcAs::new(service);

        let mut request = Request::new(GetRegistrationInfoRequest::default());
        if let Some(client_ip) = client_ip {
            request.extensions_mut().insert(client_ip);
        }

        let response = grpc_as.get_registration_info(request).await?;
        Ok(response.into_inner())
    }

    fn some_client_ip() -> Option<ClientIp> {
        Some(ClientIp::new(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1))))
    }

    #[sqlx::test]
    async fn registration_info_reports_an_open_gate(pool: PgPool) -> anyhow::Result<()> {
        let info = registration_info(&pool, RegistrationPolicy::Open, some_client_ip()).await?;

        assert!(!info.challenge_required);
        assert_eq!(
            info.accepted_challenges,
            vec![i32::from(ChallengeType::InvitationCode)]
        );

        Ok(())
    }

    #[sqlx::test]
    async fn registration_info_reports_a_closed_gate(pool: PgPool) -> anyhow::Result<()> {
        let info = registration_info(&pool, RegistrationPolicy::Required, some_client_ip()).await?;

        assert!(info.challenge_required);

        Ok(())
    }

    /// Nothing has registered yet, so the adaptive gate is still open.
    #[sqlx::test]
    async fn registration_info_reports_an_untripped_adaptive_gate(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let info = registration_info(&pool, RegistrationPolicy::Adaptive, some_client_ip()).await?;

        assert!(!info.challenge_required);

        Ok(())
    }

    /// Without an address the per-address threshold cannot apply, so the
    /// adaptive gate reports closed rather than guessing.
    #[sqlx::test]
    async fn registration_info_without_an_address_reports_closed(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let info = registration_info(&pool, RegistrationPolicy::Adaptive, None).await?;

        assert!(info.challenge_required);

        Ok(())
    }
}
