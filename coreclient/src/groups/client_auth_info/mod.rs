// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, ops::Deref};

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, ClientCredential,
        VerifiableClientCredential,
    },
    crypto::{
        hash::Hash,
        signatures::{private_keys::SignatureVerificationError, signable::Verifiable},
    },
};
use openmls::prelude::{BasicCredentialError, SignaturePublicKey};

use crate::key_stores::as_credentials::AsCredentialStoreError;

pub(crate) mod persistence;

#[derive(Debug, Clone)]
pub(crate) struct StorableClientCredential {
    client_credential: ClientCredential,
}

impl From<ClientCredential> for StorableClientCredential {
    fn from(client_credential: ClientCredential) -> Self {
        Self { client_credential }
    }
}

impl From<StorableClientCredential> for ClientCredential {
    fn from(storable_client_credential: StorableClientCredential) -> Self {
        storable_client_credential.client_credential
    }
}

impl Deref for StorableClientCredential {
    type Target = ClientCredential;

    fn deref(&self) -> &Self::Target {
        &self.client_credential
    }
}

impl StorableClientCredential {
    pub(crate) fn new(client_credential: ClientCredential) -> Self {
        Self { client_credential }
    }

    pub(crate) fn verify(
        verifiable_client_credential: VerifiableClientCredential,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Self, ClientCredentialError> {
        let as_credential = as_credentials
            .get(verifiable_client_credential.signer_fingerprint())
            .ok_or(ClientCredentialError::MissingAsCredential)?;
        let client_credential =
            verifiable_client_credential.verify(as_credential.verifying_key())?;
        Ok(Self { client_credential })
    }
}

pub(crate) trait VerifiableClientCredentialExt: Sized {
    fn verify_and_validate(
        self,
        leaf_signature_key: &SignaturePublicKey,
        old_credential: Option<&Self>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<StorableClientCredential, ClientCredentialError>;
}

impl VerifiableClientCredentialExt for VerifiableClientCredential {
    fn verify_and_validate(
        self,
        leaf_signature_key: &SignaturePublicKey,
        old_credential: Option<&Self>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<StorableClientCredential, ClientCredentialError> {
        // Verify the leaf credential
        let as_credential = as_credentials
            .get(self.signer_fingerprint())
            .ok_or(ClientCredentialError::MissingAsCredential)?;
        let client_credential: ClientCredential = self.verify(as_credential.verifying_key())?;

        // Check if the client credential matches the given public key
        if client_credential.verifying_key().as_slice() != leaf_signature_key.as_slice() {
            return Err(ClientCredentialError::LeafPublicKeyMismatch);
        }

        // If it's an update, ensure that the UserId in the new credential
        // matches the UserId in the old credential
        if let Some(old_credential) = old_credential
            && client_credential.user_id() != old_credential.user_id()
        {
            return Err(ClientCredentialError::UserIdMismatch);
        }

        Ok(StorableClientCredential::from(client_credential))
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ClientCredentialError {
    #[error(transparent)]
    Credential(#[from] BasicCredentialError),
    #[error(transparent)]
    AsCredential(#[from] AsCredentialStoreError),
    #[error(transparent)]
    Signature(#[from] SignatureVerificationError),
    #[error("Missing AS credential")]
    MissingAsCredential,
    #[error("Client credential does not match leaf public key")]
    LeafPublicKeyMismatch,
    #[error("UserId in new credential does not match UserId in old credential")]
    UserIdMismatch,
}
