// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::{AsCredential, AsCredentialBody, AsIntermediateCredential},
    crypto::hash::Hash,
    messages::client_as::BatchedTokenKeyResponse,
};

use crate::{
    auth_service::{
        AuthService,
        credentials::{intermediate_signing_key::IntermediateCredential, signing_key::Credential},
        privacy_pass::load_batched_token_keys,
    },
    errors::auth_service::AsCredentialsError,
};

pub(crate) struct AsCredentialsResponse {
    pub(crate) as_credentials: Vec<AsCredential>,
    pub(crate) as_intermediate_credentials: Vec<AsIntermediateCredential>,
    pub(crate) revoked_credentials: Vec<Hash<AsCredentialBody>>,
    pub(crate) batched_token_keys: Vec<BatchedTokenKeyResponse>,
}

impl AuthService {
    pub(crate) async fn as_credentials(&self) -> Result<AsCredentialsResponse, AsCredentialsError> {
        let as_credentials = Credential::load_all(&self.db_pool).await.map_err(|e| {
            tracing::error!("Error loading AS credentials: {:?}", e);
            AsCredentialsError::StorageError
        })?;
        let as_intermediate_credentials = IntermediateCredential::load_all(&self.db_pool)
            .await
            .map_err(|e| {
                tracing::error!("Error loading intermediate credentials: {:?}", e);
                AsCredentialsError::StorageError
            })?;
        let batched_token_keys = load_batched_token_keys(&self.db_pool)
            .await
            .map_err(|e| {
                tracing::error!("Error loading batched token keys: {:?}", e);
                AsCredentialsError::StorageError
            })?
            .into_iter()
            .map(|k| BatchedTokenKeyResponse {
                public_key: k.public_key,
                token_key_id: k.token_key_id,
                operation_type: k.operation_type as i32,
                is_current: k.is_current,
            })
            .collect();

        Ok(AsCredentialsResponse {
            as_credentials,
            as_intermediate_credentials,
            // We don't support revocation yet
            revoked_credentials: vec![],
            batched_token_keys,
        })
    }
}
