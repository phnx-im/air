// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::messages::client_as::{AsCredentialsParams, AsCredentialsResponse};

use crate::{
    auth_service::{
        AuthService,
        credentials::{intermediate_signing_key::IntermediateCredential, signing_key::Credential},
    },
    errors::auth_service::AsCredentialsError,
};

impl AuthService {
    pub(crate) async fn as_credentials(
        &self,
        _params: AsCredentialsParams,
    ) -> Result<AsCredentialsResponse, AsCredentialsError> {
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
        Ok(AsCredentialsResponse {
            as_credentials,
            as_intermediate_credentials,
            // We don't support revocation yet
            revoked_credentials: vec![],
        })
    }
}
