// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use phnxtypes::{
    LibraryError,
    credentials::keys::PreliminaryClientSigningKey,
    crypto::{indexed_aead::keys::UserProfileKeyIndex, signatures::signable::Signable as _},
    identifiers::QualifiedUserName,
};
use sqlx::SqliteExecutor;

use crate::store::StoreNotifier;

use super::{Asset, DisplayName, EncryptableUserProfile, IndexedUserProfile, SignedUserProfile};

pub(crate) struct NewUserProfile(SignedUserProfile);

impl NewUserProfile {
    /// Creates a new [`NewUserProfile`] with the given data and stores it in
    /// the database.
    pub(crate) fn new(
        signing_key: &PreliminaryClientSigningKey,
        user_name: QualifiedUserName,
        decryption_key_index: UserProfileKeyIndex,
        display_name: DisplayName,
        profile_picture: Option<Asset>,
    ) -> Result<Self, LibraryError> {
        let profile = IndexedUserProfile {
            user_name,
            epoch: 0,
            decryption_key_index,
            display_name,
            profile_picture,
        };
        let signed_profile = profile.sign(signing_key)?;
        Ok(NewUserProfile(signed_profile))
    }

    pub(crate) async fn store(
        self,
        executor: impl SqliteExecutor<'_>,
        notifier: &mut StoreNotifier,
    ) -> sqlx::Result<EncryptableUserProfile> {
        let NewUserProfile(profile) = self;
        profile.tbs.store(executor, notifier).await?;
        Ok(EncryptableUserProfile(profile))
    }
}
