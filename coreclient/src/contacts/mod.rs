// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::iter;

use aircommon::{
    credentials::VerifiableClientCredential,
    crypto::{
        aead::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{UserId, Username},
    messages::{FriendshipToken, client_as::ConnectionOfferHash},
};
use openmls::{prelude::KeyPackage, versions::ProtocolVersion};
use openmls_rust_crypto::RustCrypto;
use sqlx::SqliteConnection;

use crate::{
    ChatId,
    clients::api_clients::ApiClients,
    groups::client_auth_info::StorableClientCredential,
    key_stores::{as_credentials::AsCredentials, indexed_keys::StorableIndexedKey},
    user_profiles::IndexedUserProfile,
};
use anyhow::{Context, Result, bail};

pub(crate) mod persistence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub user_id: UserId,
    // Encryption key for WelcomeAttributionInfos
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) friendship_token: FriendshipToken,
    // ID of the connection chat with this contact.
    pub chat_id: ChatId,
}

#[derive(Debug, Clone)]
pub(crate) struct ContactAddInfos {
    pub key_package: KeyPackage,
    pub user_profile_key: UserProfileKey,
}

impl Contact {
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
        let verifiable_client_credential = VerifiableClientCredential::from_basic_credential(
            verified_key_package.leaf_node().credential(),
        )?;

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
            incoming_client_credential.user_id(),
        )
        .await?
        .context("Client credential not found")?;
        if current_client_credential.fingerprint() != incoming_client_credential.fingerprint() {
            bail!("Client credential does not match");
        }

        let user_profile = IndexedUserProfile::load(&mut *connection, &self.user_id)
            .await?
            .context("User profile not found")?;
        let user_profile_key =
            UserProfileKey::load(&mut *connection, user_profile.decryption_key_index()).await?;

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

/// Partial contact established via a username
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct UsernameContact {
    pub username: Username,
    pub chat_id: ChatId,
    pub friendship_package_ear_key: FriendshipPackageEarKey,
    pub connection_offer_hash: ConnectionOfferHash,
}

impl UsernameContact {
    pub(crate) fn new(
        username: Username,
        chat_id: ChatId,
        friendship_package_ear_key: FriendshipPackageEarKey,
        connection_offer_hash: ConnectionOfferHash,
    ) -> Self {
        Self {
            username,
            chat_id,
            friendship_package_ear_key,
            connection_offer_hash,
        }
    }
}

/// Partial contact established via a targeted message
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub struct TargetedMessageContact {
    pub user_id: UserId,
    pub chat_id: ChatId,
    pub friendship_package_ear_key: FriendshipPackageEarKey,
}

impl TargetedMessageContact {
    pub(crate) fn new(
        user_id: UserId,
        chat_id: ChatId,
        friendship_package_ear_key: FriendshipPackageEarKey,
    ) -> Self {
        Self {
            user_id,
            chat_id,
            friendship_package_ear_key,
        }
    }
}

pub enum ContactType {
    Full(Contact),
    Partial(PartialContact),
}

pub enum PartialContactType {
    Handle(Username),
    TargetedMessage(UserId),
}

impl std::fmt::Debug for PartialContactType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartialContactType::Handle(username) => f
                .debug_tuple("Handle")
                .field(&username.plaintext())
                .finish(),
            PartialContactType::TargetedMessage(user_id) => {
                f.debug_tuple("TargetedMessage").field(user_id).finish()
            }
        }
    }
}

pub enum PartialContact {
    Username(UsernameContact),
    TargetedMessage(TargetedMessageContact),
}

impl PartialContact {
    pub(crate) fn friendship_package_ear_key(&self) -> &FriendshipPackageEarKey {
        match self {
            PartialContact::Username(contact) => &contact.friendship_package_ear_key,
            PartialContact::TargetedMessage(contact) => &contact.friendship_package_ear_key,
        }
    }
}
