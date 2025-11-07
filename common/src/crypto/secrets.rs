// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module contains the [`Secret`] struct, which is meant to be
//! accessible only within the [`crate::crypto`] module. This
//! module should stay private, such that the [`Secret`] struct can stay public
//! and the type checker happy.
use std::{fmt::Display, ops::Deref};

use rand_chacha::rand_core::{RngCore as _, SeedableRng as _};
use secrecy::{
    CloneableSecret, SerializableSecret,
    zeroize::{Zeroize, ZeroizeOnDrop},
};
use serde::{Deserialize, Serialize};
use sqlx::{Database, Decode, Encode, Type, encode::IsNull, error::BoxDynError};
use tls_codec::{TlsDeserializeBytes, TlsSerialize, TlsSize};

use super::RandomnessError;

/// Struct that contains a (symmetric) secret of fixed length LENGTH.
#[derive(
    TlsSerialize, TlsDeserializeBytes, TlsSize, Clone, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct Secret<const LENGTH: usize> {
    #[serde(with = "serde_bytes")]
    secret: [u8; LENGTH],
}

impl<const LENGTH: usize> From<[u8; LENGTH]> for Secret<LENGTH> {
    fn from(secret: [u8; LENGTH]) -> Self {
        Self { secret }
    }
}

impl<const LENGTH: usize> Secret<LENGTH> {
    /// Get the internal secret value
    pub fn secret(&self) -> &[u8; LENGTH] {
        &self.secret
    }

    pub(super) fn into_secret(self) -> [u8; LENGTH] {
        self.secret
    }

    /// Generate a fresh, random secret.
    pub fn random() -> Result<Self, RandomnessError> {
        let mut secret = [0; LENGTH];
        // TODO: Use a proper rng provider.
        rand_chacha::ChaCha20Rng::from_entropy()
            .try_fill_bytes(secret.as_mut_slice())
            .map_err(|_| RandomnessError::InsufficientRandomness)?;
        Ok(Self { secret })
    }
}

// Ensure that secrets are wiped from memory securely upon being dropped.
impl<const LENGTH: usize> Zeroize for Secret<LENGTH> {
    fn zeroize(&mut self) {
        self.secret.zeroize()
    }
}

impl<const LENGTH: usize> ZeroizeOnDrop for Secret<LENGTH> {}

// Ensures that secrets are not printed in debug outputs.
impl<const LENGTH: usize> std::fmt::Debug for Secret<LENGTH> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret: [[REDACTED]]").finish()
    }
}

// Ensures that secrets are not printed in format strings.
impl<const LENGTH: usize> Display for Secret<LENGTH> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[[REDACTED]]")
    }
}

impl<const LENGTH: usize, DB: Database> Type<DB> for Secret<LENGTH>
where
    Vec<u8>: Type<DB>,
{
    fn type_info() -> <DB as Database>::TypeInfo {
        <Vec<u8> as Type<DB>>::type_info()
    }
}

impl<'q, const LENGTH: usize, DB: Database> Encode<'q, DB> for Secret<LENGTH>
where
    Box<[u8]>: Encode<'q, DB>,
{
    fn encode_by_ref(
        &self,
        buf: &mut <DB as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes: Box<[u8]> = self.secret.into();
        Encode::<DB>::encode(bytes, buf)
    }
}

impl<'r, const LENGTH: usize, DB: Database> Decode<'r, DB> for Secret<LENGTH>
where
    &'r [u8]: Decode<'r, DB>,
{
    fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes: &[u8] = Decode::<DB>::decode(value)?;
        Ok(Secret {
            secret: bytes.try_into()?,
        })
    }
}

#[derive(Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub(super) struct SecretBytes(#[serde(with = "serde_bytes")] Vec<u8>);

impl From<Vec<u8>> for SecretBytes {
    fn from(secret: Vec<u8>) -> Self {
        Self(secret)
    }
}

impl Deref for SecretBytes {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Zeroize for SecretBytes {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

impl ZeroizeOnDrop for SecretBytes {}

// Ensures that secrets are not printed in debug outputs.
impl std::fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Secret: [[REDACTED]]").finish()
    }
}

// Ensures that secrets are not printed in format strings.
impl Display for SecretBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[[REDACTED]]")
    }
}

impl SerializableSecret for SecretBytes {}
impl CloneableSecret for SecretBytes {}
