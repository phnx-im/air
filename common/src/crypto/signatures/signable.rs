// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Copied from OpenMLS
//!
//! This module defines traits used for signing and verifying
//! structs.
//!
//! # Type-Enforced Verification
//!
//! This module contains four traits, each describing the property they enable
//! upon implementation: [`Signable`], [`SignedStruct`], [`Verifiable`] and
//! [`VerifiedStruct`].
//!
//! Each trait represents the state of a struct in a sender-receiver flow with
//! the following transitions.
//!
//! * the signer creates an instance of a struct that implements [`Signable`]
//! * the signer signs it, consuming the [`Signable`] struct and producing a [`SignedStruct`]
//! * the signer serializes the struct and sends it to the verifier
//! * the verifier deserializes the byte-string into a struct implementing [`Verifiable`]
//! * the verifier verifies the struct, consuming the [`Verifiable`] struct and producing a [`VerifiedStruct`]
//!
//! Using this process, we can ensure that only structs implementing
//! [`SignedStruct`] are sent over the wire and only structs implementing
//! [`VerifiedStruct`] are used on the verifier side as input for further
//! processing functions.
//!
//! For the type-safety to work, it is important that [`Signable`] and
//! [`SignedStruct`] are implemented by distinct structs. The same goes for
//! [`Verifiable`] and [`VerifiedStruct`]. In addition, only the
//! [`SignedStruct`] should implement the [`tls_codec::Serialize`] trait.
//! Similarly, only the [`Verifiable`] struct should implement the
//! [`tls_codec::Deserialize`] trait.

use std::{marker::PhantomData, vec};

use serde::{Deserialize, Serialize};
use tls_codec::{
    Serialize as TlsSerializeTrait, TlsDeserializeBytes, TlsSerialize, TlsSize, VLBytes,
};

use crate::LibraryError;

use super::private_keys::{
    Convertible, SignatureVerificationError, SigningKey, VerifyingKeyBehaviour,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Signature<KT> {
    #[serde(with = "serde_bytes")]
    bytes: Vec<u8>,
    _phantom: PhantomData<KT>,
}

impl<KT> Signature<KT> {
    pub fn empty() -> Self {
        Self::from_bytes(vec![])
    }

    pub(crate) fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            _phantom: PhantomData,
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// Convert the signature of one key type to that of another.
    pub fn convert<TargetKT>(self) -> Signature<TargetKT>
    where
        KT: Convertible<TargetKT>,
    {
        Signature {
            bytes: self.bytes,
            _phantom: PhantomData,
        }
    }

    #[cfg(any(feature = "test_utils", test))]
    pub fn new_for_test(value: Vec<u8>) -> Self {
        Self::from_bytes(value)
    }
}

impl<KT> AsRef<[u8]> for Signature<KT> {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

/// This trait must be implemented by all structs that contain a self-signature.
pub trait SignedStruct<T, KT> {
    /// Build a signed struct version from the payload struct.
    fn from_payload(payload: T, signature: Signature<KT>) -> Self;
}

/// Labeled signature content.
#[derive(Debug, Clone, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct SignContent {
    label: VLBytes,
    content: VLBytes,
}

const SIGN_LABEL_PREFIX: &str = "Air Protocol";

impl From<(&str, &[u8])> for SignContent {
    fn from((label, content): (&str, &[u8])) -> Self {
        let label_string = SIGN_LABEL_PREFIX.to_owned() + label;
        let label = label_string.as_bytes().into();
        Self {
            label,
            content: content.into(),
        }
    }
}

pub enum SigningError {
    SerializationError,
}

/// This trait must be implemented by all structs that contain a verified
/// self-signature.
pub trait VerifiedStruct<T> {
    /// This type is used to prevent users of the trait from bypassing `verify`
    /// by simply calling `from_verifiable`. `Seal` should be a dummy type
    /// defined in a private module as follows:
    /// ```
    /// mod private_mod {
    ///     pub struct Seal;
    ///
    ///     impl Default for Seal {
    ///         fn default() -> Self {
    ///             Seal {}
    ///         }
    ///     }
    /// }
    /// ```
    type SealingType: Default;

    /// Build a verified struct version from the payload struct. This function
    /// is only meant to be called by the implementation of the `Verifiable`
    /// trait corresponding to this `VerifiedStruct`.
    #[doc(hidden)]
    fn from_verifiable(verifiable: T, _seal: Self::SealingType) -> Self;
}

/// The `Signable` trait is implemented by all struct that are being signed.
/// The implementation has to provide the `unsigned_payload` function.
pub trait Signable: Sized + std::fmt::Debug {
    /// The type of the object once it's signed.
    type SignedOutput;

    /// Return the unsigned, serialized payload that should be signed.
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error>;

    /// Return the string label used for labeled signing.
    fn label(&self) -> &str;

    /// Sign the payload.
    ///
    /// Returns a `Signature`.
    fn sign<KT>(self, signing_key: &SigningKey<KT>) -> Result<Self::SignedOutput, LibraryError>
    where
        Self::SignedOutput: SignedStruct<Self, KT>,
    {
        let payload = self
            .unsigned_payload()
            .map_err(LibraryError::missing_bound_check)?;
        let sign_content: SignContent = (self.label(), payload.as_slice()).into();
        let serialized_sign_content = sign_content
            .tls_serialize_detached()
            .map_err(LibraryError::missing_bound_check)?;
        let signature = signing_key.sign(&serialized_sign_content)?;
        Ok(Self::SignedOutput::from_payload(self, signature))
    }
}

/// The verifiable trait must be implemented by any struct that is signed with
/// a credential. The actual `verify` method is provided.
/// The `unsigned_payload` and `signature` functions have to be implemented for
/// each struct, returning the serialized payload and the signature respectively.
///
/// Note that `Verifiable` should not be implemented on the same struct as
/// `Signable`. If this appears to be necessary, it is probably a sign that the
/// struct implementing them aren't well defined. Not that both traits define an
/// `unsigned_payload` function.
pub trait Verifiable: Sized + std::fmt::Debug {
    /// Return the unsigned, serialized payload that should be verified.
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error>;

    /// A reference to the signature to be verified.
    fn signature(&self) -> impl AsRef<[u8]>;

    /// Return the string label used for labeled verification.
    fn label(&self) -> &str;

    /// Verifies the payload against the given `credential`.
    /// The signature is fetched via the [`Verifiable::signature()`] function and
    /// the payload via [`Verifiable::unsigned_payload()`].
    ///
    /// Returns `Ok(Self::VerifiedOutput)` if the signature is valid and
    /// `CredentialError::InvalidSignature` otherwise.
    fn verify<T>(
        self,
        signature_public_key: impl VerifyingKeyBehaviour,
    ) -> Result<T, SignatureVerificationError>
    where
        T: VerifiedStruct<Self>,
    {
        let payload = self
            .unsigned_payload()
            .map_err(LibraryError::missing_bound_check)?;
        let sign_content: SignContent = (self.label(), payload.as_slice()).into();
        let serialized_sign_content = sign_content
            .tls_serialize_detached()
            .map_err(LibraryError::missing_bound_check)?;
        signature_public_key.verify(&serialized_sign_content, self.signature().as_ref())?;
        Ok(T::from_verifiable(self, T::SealingType::default()))
    }
}

mod trait_impls {
    use sqlx::{Database, Decode, Encode, Type};

    use super::*;

    impl<KT, DB: Database> Type<DB> for Signature<KT>
    where
        Vec<u8>: Type<DB>,
    {
        fn type_info() -> DB::TypeInfo {
            <Vec<u8> as Type<DB>>::type_info()
        }
    }

    impl<'r, KT, DB: Database> Decode<'r, DB> for Signature<KT>
    where
        &'r [u8]: Decode<'r, DB>,
    {
        fn decode(value: <DB as Database>::ValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
            let bytes: &'r [u8] = Decode::<DB>::decode(value)?;
            Ok(Signature {
                bytes: bytes.to_vec(),
                _phantom: PhantomData,
            })
        }
    }

    impl<'q, KT, DB: Database> Encode<'q, DB> for Signature<KT>
    where
        Vec<u8>: Encode<'q, DB>,
    {
        fn encode_by_ref(
            &self,
            buf: &mut <DB as Database>::ArgumentBuffer<'q>,
        ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
            self.bytes.encode_by_ref(buf)
        }
    }

    impl<KT> Clone for Signature<KT> {
        fn clone(&self) -> Self {
            Self {
                bytes: self.bytes.clone(),
                _phantom: PhantomData,
            }
        }
    }

    impl<KT> PartialEq for Signature<KT> {
        fn eq(&self, other: &Self) -> bool {
            self.bytes == other.bytes
        }
    }

    impl<KT> Eq for Signature<KT> {}

    impl<KT> tls_codec::Size for Signature<KT> {
        fn tls_serialized_len(&self) -> usize {
            self.bytes.tls_serialized_len()
        }
    }

    impl<KT> tls_codec::Serialize for Signature<KT> {
        fn tls_serialize<W: std::io::Write>(
            &self,
            writer: &mut W,
        ) -> Result<usize, tls_codec::Error> {
            self.bytes.tls_serialize(writer)
        }
    }

    impl<KT> tls_codec::DeserializeBytes for Signature<KT> {
        fn tls_deserialize_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), tls_codec::Error> {
            let (bytes, remaining) = Vec::<u8>::tls_deserialize_bytes(bytes)?;
            Ok((
                Signature {
                    bytes,
                    _phantom: PhantomData,
                },
                remaining,
            ))
        }
    }
}
