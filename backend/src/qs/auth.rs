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
use airprotos::queue_service::v1::{
    CreateClientPayload, CreateClientRequest, DeleteClientPayload, DeleteClientRequest,
    DeleteUserPayload, DeleteUserRequest, InitListenPayload, InitListenRequest,
    PublishKeyPackagesPayload, PublishKeyPackagesRequest, QsClientId, QsUserId,
    UpdateClientPayload, UpdateClientRequest, UpdateUserPayload, UpdateUserRequest,
};
use tonic::Status;
use tracing::error;

use crate::qs::{client_record::QsClientRecord, grpc::GrpcQs, user_record::UserRecord};

impl GrpcQs {
    /// Verifies request with QS user authentication.
    pub(super) async fn verify_user_auth<R, P>(&self, request: R) -> Result<P, Status>
    where
        R: WithQsUserId<Payload = P> + Verifiable,
        P: VerifiedStruct<R>,
    {
        match request.user_id() {
            // Support for legacy clients which use don't authentication.
            None => Ok(request.into_unverified_payload()),
            Some(user_id) => {
                let user_id = user_id?;
                let verifying_key = UserRecord::load_verifying_key(&self.qs.db_pool, &user_id)
                    .await
                    .map_err(|error| {
                        error!(%error, "failed to load user verifying key");
                        Status::internal("database error")
                    })?
                    .ok_or_else(|| Status::not_found("unknown QS user"))?;
                self.verify_request(request, &verifying_key)
            }
        }
    }

    /// Verifies request with QS client authentication.
    pub(super) async fn verify_client_auth<R, P>(&self, request: R) -> Result<P, Status>
    where
        R: WithQsClientId<Payload = P> + Verifiable,
        P: VerifiedStruct<R>,
    {
        match request.client_id() {
            // Support for legacy clients which don't use authentication.
            None => Ok(request.into_unverified_payload()),
            Some(client_id) => {
                let client_id = client_id?;
                dbg!(&client_id);
                let verifying_key = dbg!(
                    QsClientRecord::load_verifying_key(&self.qs.db_pool, &client_id)
                        .await
                        .map_err(|error| {
                            error!(%error, "failed to load client verifying key");
                            Status::internal("database error")
                        })?
                        .ok_or_else(|| Status::not_found("unknown QS client"))?
                );
                self.verify_request(request, &verifying_key)
            }
        }
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

/// QS requests that contain a user id.
pub(super) trait WithQsUserId {
    type Payload;

    fn user_id_proto(&self) -> Option<QsUserId>;

    fn user_id(&self) -> Option<Result<identifiers::QsUserId, Status>> {
        // For now, not having a user id is not an error. We still have old clients that don't use
        // authentication.
        let user_id = self.user_id_proto()?;
        Some(user_id.try_into().map_err(From::from))
    }

    /// Converts the fields of the request into a payload.
    ///
    /// This method is used for requests coming from legacy clients that don't use authentication.
    fn into_unverified_payload(self) -> Self::Payload;
}

impl WithQsUserId for UpdateUserRequest {
    type Payload = UpdateUserPayload;

    fn user_id_proto(&self) -> Option<QsUserId> {
        self.payload.as_ref()?.sender
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            sender,
            user_record_auth_key,
            friendship_token,
            ..
        } = self;
        UpdateUserPayload {
            client_metadata,
            sender,
            user_record_auth_key,
            friendship_token,
        }
    }
}

impl WithQsUserId for DeleteUserRequest {
    type Payload = DeleteUserPayload;

    fn user_id_proto(&self) -> Option<QsUserId> {
        self.payload.as_ref()?.sender
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            sender,
            ..
        } = self;
        DeleteUserPayload {
            client_metadata,
            sender,
        }
    }
}

impl WithQsUserId for CreateClientRequest {
    type Payload = CreateClientPayload;

    fn user_id_proto(&self) -> Option<QsUserId> {
        self.payload.as_ref()?.sender
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            initial_ratched_secret,
            ..
        } = self;
        CreateClientPayload {
            client_metadata,
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            initial_ratched_secret,
        }
    }
}

/// QS requests that contain a client id.
pub(super) trait WithQsClientId {
    type Payload;

    fn client_id_proto(&self) -> Option<QsClientId>;

    fn client_id(&self) -> Option<Result<identifiers::QsClientId, Status>> {
        // For now, not having a client id is not an error. We still have old clients that don't use
        // authentication.
        let user_id = self.client_id_proto()?;
        Some(user_id.try_into().map_err(From::from))
    }

    /// Converts the fields of the request into a payload.
    ///
    /// This method is used for requests coming from legacy clients that don't use authentication.
    fn into_unverified_payload(self) -> Self::Payload;
}

impl WithQsClientId for UpdateClientRequest {
    type Payload = UpdateClientPayload;

    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.sender
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
            ..
        } = self;
        UpdateClientPayload {
            client_metadata,
            sender,
            client_record_auth_key,
            queue_encryption_key,
            encrypted_push_token,
        }
    }
}

impl WithQsClientId for DeleteClientRequest {
    type Payload = DeleteClientPayload;

    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.sender
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            sender,
            ..
        } = self;
        DeleteClientPayload {
            client_metadata,
            sender,
        }
    }
}

impl WithQsClientId for PublishKeyPackagesRequest {
    type Payload = PublishKeyPackagesPayload;

    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.client_id
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            client_id,
            key_packages,
            ..
        } = self;
        PublishKeyPackagesPayload {
            client_metadata,
            client_id,
            key_packages,
        }
    }
}

impl WithQsClientId for InitListenRequest {
    type Payload = InitListenPayload;

    fn client_id_proto(&self) -> Option<QsClientId> {
        self.payload.as_ref()?.client_id
    }

    fn into_unverified_payload(self) -> Self::Payload {
        let Self {
            client_metadata,
            client_id,
            sequence_number_start,
            ..
        } = self;
        InitListenPayload {
            client_metadata,
            client_id,
            sequence_number_start,
        }
    }
}
