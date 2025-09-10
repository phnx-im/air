// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::iter;

use aircommon::{
    LibraryError,
    crypto::{
        ear::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
        indexed_aead::keys::{UserProfileKey, UserProfileKeyIndex},
        kdf::keys::ConnectionKey,
    },
    identifiers::{UserHandle, UserId},
    messages::{FriendshipToken, client_as::ConnectionOfferHash},
};
use openmls::{prelude::KeyPackage, versions::ProtocolVersion};
use openmls_rust_crypto::RustCrypto;
use sqlx::SqliteConnection;

use crate::{
    ChatId,
    clients::{api_clients::ApiClients, connection_offer::FriendshipPackage},
    groups::client_auth_info::StorableClientCredential,
    key_stores::{as_credentials::AsCredentials, indexed_keys::StorableIndexedKey},
};
use anyhow::{Context, Result, bail};

pub(crate) mod persistence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub user_id: UserId,
    // Encryption key for WelcomeAttributionInfos
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) friendship_token: FriendshipToken,
    pub(crate) connection_key: ConnectionKey,
    pub(crate) user_profile_key_index: UserProfileKeyIndex,
    // ID of the connection conversation with this contact.
    pub(crate) conversation_id: ChatId,
}

#[derive(Debug, Clone)]
pub(crate) struct ContactAddInfos {
    pub key_package: KeyPackage,
    pub user_profile_key: UserProfileKey,
}

impl Contact {
    pub(crate) fn from_friendship_package(
        user_id: UserId,
        conversation_id: ChatId,
        friendship_package: FriendshipPackage,
    ) -> Result<Self, LibraryError> {
        let user_profile_key = UserProfileKey::from_base_secret(
            friendship_package.user_profile_base_secret,
            &user_id,
        )?;
        let contact = Self {
            user_id,
            wai_ear_key: friendship_package.wai_ear_key,
            friendship_token: friendship_package.friendship_token,
            connection_key: friendship_package.connection_key,
            conversation_id,
            user_profile_key_index: user_profile_key.index().clone(),
        };
        Ok(contact)
    }

    pub(crate) async fn fetch_add_infos(
        &self,
        connection: &mut SqliteConnection,
        api_clients: &ApiClients,
    ) -> Result<ContactAddInfos> {
        let invited_user_domain = self.user_id.domain();

        let key_package_response = api_clients
            .get(invited_user_domain)?
            .qs_key_package(self.friendship_token.clone())
            .await?;

        let key_package_in = key_package_response.key_package;

        // Verify the KeyPackage
        let verified_key_package =
            key_package_in.validate(&RustCrypto::default(), ProtocolVersion::default())?;
        let verifiable_client_credential = verified_key_package
            .leaf_node()
            .credential()
            .clone()
            .try_into()?;

        let as_credential = AsCredentials::fetch_for_verification(
            connection,
            api_clients,
            iter::once(&verifiable_client_credential),
        )
        .await?;

        // Verify the client credential
        let incoming_client_credential =
            StorableClientCredential::verify(verifiable_client_credential, &as_credential)?;

        // Check that the client credential is the same as the one we have on file.
        let current_client_credential = StorableClientCredential::load_by_user_id(
            &mut *connection,
            incoming_client_credential.identity(),
        )
        .await?
        .context("Client credential not found")?;
        if current_client_credential.fingerprint() != incoming_client_credential.fingerprint() {
            bail!("Client credential does not match");
        }

        let user_profile_key =
            UserProfileKey::load(&mut *connection, &self.user_profile_key_index).await?;

        let add_info = ContactAddInfos {
            key_package: verified_key_package,
            user_profile_key,
        };
        Ok(add_info)
    }

    pub(crate) fn wai_ear_key(&self) -> &WelcomeAttributionInfoEarKey {
        &self.wai_ear_key
    }
}

/// Partial contact established via a user handle
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct HandleContact {
    pub handle: UserHandle,
    pub conversation_id: ChatId,
    pub friendship_package_ear_key: FriendshipPackageEarKey,
    // This is Optional only for backwards compatibility
    pub connection_offer_hash: ConnectionOfferHash,
}

impl HandleContact {
    pub(crate) fn new(
        handle: UserHandle,
        conversation_id: ChatId,
        friendship_package_ear_key: FriendshipPackageEarKey,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Self {
        Self {
            handle,
            conversation_id,
            friendship_package_ear_key,
            connection_offer_hash,
        }
    }
}
