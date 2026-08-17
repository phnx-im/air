// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{collections::HashMap, ops::Deref};

use aircommon::{
    credentials::{
        AsIntermediateCredential, AsIntermediateCredentialBody, UserCredential,
        VerifiableUserCredential,
    },
    crypto::{hash::Hash, signatures::signable::Verifiable},
};
use anyhow::{Context, Result, ensure};
use openmls::prelude::SignaturePublicKey;

pub(crate) mod persistence;

#[derive(Debug, Clone)]
pub(crate) struct StorableUserCredential {
    user_credential: UserCredential,
}

impl From<UserCredential> for StorableUserCredential {
    fn from(user_credential: UserCredential) -> Self {
        Self { user_credential }
    }
}

impl From<StorableUserCredential> for UserCredential {
    fn from(storable_user_credential: StorableUserCredential) -> Self {
        storable_user_credential.user_credential
    }
}

impl Deref for StorableUserCredential {
    type Target = UserCredential;

    fn deref(&self) -> &Self::Target {
        &self.user_credential
    }
}

impl StorableUserCredential {
    pub(crate) fn new(user_credential: UserCredential) -> Self {
        Self { user_credential }
    }

    pub(crate) fn verify(
        verifiable_user_credential: VerifiableUserCredential,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<Self> {
        let as_credential = as_credentials
            .get(verifiable_user_credential.signer_fingerprint())
            .context("Missing AS credential")?;
        let user_credential = verifiable_user_credential.verify(as_credential.verifying_key())?;
        Ok(Self { user_credential })
    }
}

pub(crate) trait VerifiableUserCredentialExt: Sized {
    fn verify_and_validate(
        self,
        leaf_signature_key: &SignaturePublicKey,
        old_credential: Option<&Self>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<StorableUserCredential>;
}

impl VerifiableUserCredentialExt for VerifiableUserCredential {
    fn verify_and_validate(
        self,
        leaf_signature_key: &SignaturePublicKey,
        old_credential: Option<&Self>,
        as_credentials: &HashMap<Hash<AsIntermediateCredentialBody>, AsIntermediateCredential>,
    ) -> Result<StorableUserCredential> {
        // Verify the leaf credential
        let as_credential = as_credentials
            .get(self.signer_fingerprint())
            .context("Missing AS credential")?;
        let user_credential: UserCredential = self.verify(as_credential.verifying_key())?;

        // Check if the user credential matches the given public key
        ensure!(
            user_credential.verifying_key().as_slice() == leaf_signature_key.as_slice(),
            "User credential does not match leaf public key"
        );

        // If it's an update, ensure that the UserId in the new credential
        // matches the UserId in the old credential
        if let Some(old_credential) = old_credential {
            ensure!(
                user_credential.user_id() == old_credential.user_id(),
                "UserId in new credential does not match UserId in old credential"
            );
        }

        Ok(StorableUserCredential::from(user_credential))
    }
}
