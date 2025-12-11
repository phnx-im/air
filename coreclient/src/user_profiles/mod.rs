// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! This module provides structs and functions to interact with users in the
//! various groups a Client is a member of.

use std::{fmt, mem};

use aircommon::{
    LibraryError,
    credentials::keys::{ClientKeyType, ClientSignature, PreliminaryClientKeyType},
    crypto::{
        ear::{EarDecryptable, EarEncryptable},
        indexed_aead::{
            ciphertexts::{IndexDecryptable, IndexEncryptable},
            keys::{UserProfileKey, UserProfileKeyIndex, UserProfileKeyType},
        },
        signatures::{
            private_keys::SignatureVerificationError,
            signable::{Signable, Signature, SignedStruct, Verifiable, VerifiedStruct},
        },
    },
    identifiers::UserId,
    messages::client_as_out::{EncryptedUserProfileCtype, GetUserProfileResponse},
};
use display_name::BaseDisplayName;
pub use display_name::{DisplayName, DisplayNameError};
use sealed::Seal;
use serde::{Deserialize, Serialize};
use sqlx::{
    Database, Decode, Encode, Sqlite, SqliteConnection, encode::IsNull, error::BoxDynError,
};
use thiserror::Error;
use tls_codec::{Serialize as _, TlsDeserializeBytes, TlsSerialize, TlsSize};
use tracing::info;

use crate::{
    clients::api_clients::ApiClients, groups::ProfileInfo,
    key_stores::indexed_keys::StorableIndexedKey as _, store::StoreNotifier,
    user_profiles::process::ExistingUserProfile,
};

pub mod display_name;
pub(crate) mod generate;
pub(crate) mod persistence;
pub(crate) mod process;
#[cfg(test)]
mod tests;
pub(crate) mod update;

const USER_PROFILE_LABEL: &str = "UserProfile";

impl Signable for IndexedUserProfile {
    type SignedOutput = SignedUserProfile;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }

    fn label(&self) -> &str {
        USER_PROFILE_LABEL
    }
}

// User profiles need to be signable by both the client credential and the
// preliminary client credential.

impl SignedStruct<IndexedUserProfile, PreliminaryClientKeyType> for SignedUserProfile {
    fn from_payload(
        payload: IndexedUserProfile,
        signature: Signature<PreliminaryClientKeyType>,
    ) -> Self {
        Self {
            tbs: payload,
            signature: signature.convert(),
        }
    }
}

impl SignedStruct<IndexedUserProfile, ClientKeyType> for SignedUserProfile {
    fn from_payload(payload: IndexedUserProfile, signature: Signature<ClientKeyType>) -> Self {
        Self {
            tbs: payload,
            signature,
        }
    }
}

#[derive(Debug, TlsSize, TlsSerialize)]
pub(crate) struct SignedUserProfile {
    tbs: IndexedUserProfile,
    signature: ClientSignature,
}

impl Verifiable for VerifiableUserProfile {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tbs.tls_serialize_detached()
    }

    fn signature(&self) -> impl AsRef<[u8]> {
        &self.signature
    }

    fn label(&self) -> &str {
        USER_PROFILE_LABEL
    }
}

mod sealed {
    #[derive(Default)]
    pub struct Seal;
}

impl VerifiedStruct<VerifiableUserProfile> for UnvalidatedUserProfile {
    type SealingType = Seal;

    fn from_verifiable(verifiable: VerifiableUserProfile, _seal: Self::SealingType) -> Self {
        verifiable.tbs
    }
}

#[derive(Debug, Clone, PartialEq, Eq, TlsSize, TlsSerialize, TlsDeserializeBytes)]
pub(crate) struct VerifiableUserProfile {
    tbs: UnvalidatedUserProfile,
    signature: ClientSignature,
}

#[derive(Debug, Error)]
pub enum UserProfileValidationError {
    #[error("User profile is outdated")]
    OutdatedUserProfile { user_id: UserId, epoch: u64 },
    #[error("Mismatching user id")]
    MismatchingUserId { expected: UserId, actual: UserId },
    #[error(transparent)]
    InvalidSignature(#[from] SignatureVerificationError),
    #[error(transparent)]
    LibraryError(#[from] LibraryError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub user_id: UserId,
    pub display_name: DisplayName,
    pub profile_picture: Option<Asset>,
}

impl UserProfile {
    pub fn from_user_id(user_id: &UserId) -> Self {
        Self {
            user_id: user_id.clone(),
            display_name: DisplayName::from_user_id(user_id),
            profile_picture: None,
        }
    }

    /// Takes data from the user profile without cloning it.
    ///
    /// This user profiles is empty after this operation.
    pub fn take(&mut self) -> UserProfile {
        Self {
            user_id: self.user_id.clone(),
            display_name: mem::take(&mut self.display_name),
            profile_picture: mem::take(&mut self.profile_picture),
        }
    }

    pub(crate) async fn fetch_and_store(
        connection: &mut SqliteConnection,
        notifier: &mut StoreNotifier,
        api_clients: &ApiClients,
        profile_info: impl Into<ProfileInfo>,
    ) -> anyhow::Result<()> {
        let ProfileInfo {
            user_profile_key,
            client_credential,
        } = profile_info.into();
        let user_id = client_credential.identity();

        let api_client = api_clients.get(user_id.domain())?;

        // Phase 1: Check if the profile in the DB is up to date.
        let existing_user_profile = ExistingUserProfile::load(&mut *connection, user_id).await?;
        if existing_user_profile.matches_index(user_profile_key.index()) {
            return Ok(());
        }

        // Phase 2: Fetch the user profile from the server
        let GetUserProfileResponse {
            encrypted_user_profile,
        } = api_client
            .as_get_user_profile(
                client_credential.identity().clone(),
                user_profile_key.index().clone(),
            )
            .await?;

        let verifiable_user_profile =
            VerifiableUserProfile::decrypt_with_index(&user_profile_key, &encrypted_user_profile)?;
        let persistable_user_profile = existing_user_profile
            .process_decrypted_user_profile(verifiable_user_profile, &client_credential)?;

        // Phase 3: Store the user profile and key in the database
        user_profile_key.store(&mut *connection).await?;
        persistable_user_profile
            .persist(&mut *connection, notifier)
            .await?;
        if let Some(old_user_profile_index) = persistable_user_profile.old_profile_index() {
            // Delete the old user profile key
            UserProfileKey::delete(&mut *connection, old_user_profile_index).await?;
        }

        Ok(())
    }
}

impl From<IndexedUserProfile> for UserProfile {
    fn from(user_profile: IndexedUserProfile) -> Self {
        Self {
            user_id: user_profile.user_id,
            display_name: user_profile.display_name,
            profile_picture: user_profile.profile_picture,
        }
    }
}

/// A user profile contains information about a user, such as their display name
/// and profile picture.
#[derive(
    Debug, Clone, PartialEq, Eq, TlsSerialize, TlsDeserializeBytes, TlsSize, Serialize, Deserialize,
)]
pub(crate) struct BaseIndexedUserProfile<const VALIDATED: bool> {
    user_id: UserId,
    epoch: u64,
    decryption_key_index: UserProfileKeyIndex,
    display_name: BaseDisplayName<VALIDATED>,
    profile_picture: Option<Asset>,
}

pub(crate) type IndexedUserProfile = BaseIndexedUserProfile<true>;

pub(crate) type UnvalidatedUserProfile = BaseIndexedUserProfile<false>;

impl UnvalidatedUserProfile {
    /// Validates the display name and returns an [`IndexedUserProfile`].
    /// If the display name is invalid, it is replaced with a default
    /// based on the user id.
    pub fn validate_display_name(self) -> IndexedUserProfile {
        let display_name = self.display_name.validate().unwrap_or_else(|e| {
            info!(error = %e, "Invalid display name, generating default");
            DisplayName::from_user_id(&self.user_id)
        });
        IndexedUserProfile {
            user_id: self.user_id,
            epoch: self.epoch,
            decryption_key_index: self.decryption_key_index,
            display_name,
            profile_picture: self.profile_picture,
        }
    }
}

impl IndexedUserProfile {
    pub(crate) fn decryption_key_index(&self) -> &UserProfileKeyIndex {
        &self.decryption_key_index
    }
}

#[derive(
    TlsSerialize, TlsDeserializeBytes, TlsSize, Clone, Serialize, Deserialize, PartialEq, Eq,
)]
#[repr(u8)]
pub enum Asset {
    Value(Vec<u8>),
    // TODO: Assets by Reference
}

impl fmt::Debug for Asset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Value(bytes) => f
                .debug_struct("Asset")
                .field("bytes", &bytes.len())
                .finish(),
        }
    }
}

impl sqlx::Type<Sqlite> for Asset {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as sqlx::Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for Asset {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        match self {
            Asset::Value(value) => Encode::<Sqlite>::encode_by_ref(value, buf),
        }
    }
}

impl<'r> Decode<'r, Sqlite> for Asset {
    fn decode(value: <Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        Decode::<Sqlite>::decode(value).map(Asset::Value)
    }
}

impl Asset {
    pub fn value(&self) -> Option<&[u8]> {
        match self {
            Asset::Value(value) => Some(value),
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Asset::Value(value) => value,
        }
    }
}

#[derive(Debug, TlsSize, TlsSerialize)]
pub(crate) struct EncryptableUserProfile(SignedUserProfile);

impl EarEncryptable<UserProfileKey, EncryptedUserProfileCtype> for EncryptableUserProfile {}
impl EarDecryptable<UserProfileKey, EncryptedUserProfileCtype> for VerifiableUserProfile {}

impl IndexEncryptable<UserProfileKeyType, EncryptedUserProfileCtype> for EncryptableUserProfile {}
impl IndexDecryptable<UserProfileKeyType, EncryptedUserProfileCtype> for VerifiableUserProfile {}
