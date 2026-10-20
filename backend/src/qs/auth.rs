// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::signatures::{
        private_keys::{SignatureVerificationError, VerifyingKeyBehaviour},
        signable::{Verifiable, VerifiedStruct},
    },
    identifiers,
};
use airprotos::{
    queue_service::v1::{
        CreateClientRequest, DeleteClientRequest, DeleteUserRequest, InitListenRequest,
        PublishApqKeyPackagesRequest, PublishKeyPackagesRequest, QsClientId, QsUserId,
        StageKeyPackagesRequest, UpdateClientRequest, UpdateUserRequest,
    },
    signed::{SignedRequest, VerifiableRequest},
};
use tonic::Status;
use tracing::error;

use crate::qs::{client_record::QsClientRecord, grpc::GrpcQs, user_record::UserRecord};

impl GrpcQs {
    /// Verifies request with QS user authentication.
    pub(super) async fn verify_user_auth<R, P, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
    ) -> Result<P, Status>
    where
        R: WithQsUserId + VerifiableRequest,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let user_id = request.inner().user_id()?;
        let verifying_key = UserRecord::load_verifying_key(&self.qs.db_pool, &user_id)
            .await
            .map_err(|error| {
                error!(%error, "failed to load user verifying key");
                Status::internal("database error")
            })?
            .ok_or_else(|| Status::not_found("unknown QS user"))?;
        self.verify_request(request, &verifying_key)
    }

    /// Verifies request with QS client authentication.
    pub(super) async fn verify_client_auth<R, P, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
    ) -> Result<P, Status>
    where
        R: WithQsClientId + VerifiableRequest,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let (payload, _) = self.verify_client_auth_with_user_id(request).await?;
        Ok(payload)
    }

    /// Verifies request with QS client authentication.
    ///
    /// Also returns the user of the client.
    pub(super) async fn verify_client_auth_with_user_id<R, P, const TAG: u32>(
        &self,
        request: SignedRequest<R, TAG>,
    ) -> Result<(P, identifiers::QsUserId), Status>
    where
        R: WithQsClientId + VerifiableRequest,
        P: VerifiedStruct<SignedRequest<R, TAG>>,
    {
        let client_id = request.inner().client_id()?;
        let (verifying_key, user_id) =
            QsClientRecord::load_verifying_key_and_user_id(&self.qs.db_pool, &client_id)
                .await
                .map_err(|error| {
                    error!(%error, "failed to load client verifying key");
                    Status::internal("database error")
                })?
                .ok_or_else(|| Status::not_found("unknown QS client"))?;
        let payload = self.verify_request(request, &verifying_key)?;
        Ok((payload, user_id))
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
}

/// QS requests that are authenticated by a QS user.
pub(super) trait WithQsUserId {
    fn user_id_proto(&self) -> Option<QsUserId>;

    fn user_id(&self) -> Result<identifiers::QsUserId, Status> {
        let user_id = self
            .user_id_proto()
            .ok_or_else(|| Status::unauthenticated("missing QS user id"))?;
        Ok(user_id.try_into()?)
    }
}

impl WithQsUserId for UpdateUserRequest {
    fn user_id_proto(&self) -> Option<QsUserId> {
        self.payload.as_ref()?.sender
    }
}

impl WithQsUserId for DeleteUserRequest {
    fn user_id_proto(&self) -> Option<QsUserId> {
        self.payload.as_ref()?.sender
    }
}

impl WithQsUserId for CreateClientRequest {
    fn user_id_proto(&self) -> Option<QsUserId> {
        self.payload.as_ref()?.sender
    }
}

/// QS requests that are authenticated by a QS client.
pub(super) trait WithQsClientId {
    fn client_id_proto(&self) -> Option<QsClientId>;

    fn client_id(&self) -> Result<identifiers::QsClientId, Status> {
        let client_id = self
            .client_id_proto()
            .ok_or_else(|| Status::unauthenticated("missing QS client id"))?;
        Ok(client_id.try_into()?)
    }
}

impl WithQsClientId for UpdateClientRequest {
    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.sender
    }
}

impl WithQsClientId for DeleteClientRequest {
    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.sender
    }
}

impl WithQsClientId for PublishKeyPackagesRequest {
    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.client_id
    }
}

impl WithQsClientId for StageKeyPackagesRequest {
    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.client_id
    }
}

impl WithQsClientId for PublishApqKeyPackagesRequest {
    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.client_id
    }
}

impl WithQsClientId for InitListenRequest {
    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.client_id
    }
}
