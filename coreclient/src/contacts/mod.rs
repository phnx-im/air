// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::iter;

use aircommon::{
    component::AirFeatures,
    credentials::VerifiableClientCredential,
    crypto::{
        aead::keys::{FriendshipPackageEarKey, WelcomeAttributionInfoEarKey},
        indexed_aead::keys::UserProfileKey,
    },
    identifiers::{UserId, Username},
    messages::{FriendshipToken, client_as::ConnectionOfferHash},
};
use apqmls::messages::ApqKeyPackage;
use openmls::{prelude::KeyPackage, versions::ProtocolVersion};
use openmls_rust_crypto::RustCrypto;

use crate::{
    ChatId,
    clients::api_clients::ApiClients,
    db_access::{ReadConnection, WriteConnection},
    groups::{Group, client_auth_info::StorableClientCredential},
    key_stores::{as_credentials::AsCredentials, indexed_keys::StorableIndexedKey},
    user_profiles::IndexedUserProfile,
};
use anyhow::{Context, Result, bail, ensure};

pub(crate) mod persistence;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contact {
    pub user_id: UserId,
    /// Encryption key for WelcomeAttributionInfos
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) friendship_token: FriendshipToken,
    /// ID of the connection chat with this contact.
    pub chat_id: ChatId,
    /// Features supported by the contact
    ///
    /// `None` means that the features are not yet loaded. Load on demand with
    /// [`Contact::augment_supported_features`].
    pub supported_features: Option<AirFeatures>,
}

#[derive(Debug, Clone)]
pub(crate) struct ContactAddInfos {
    pub key_package: ContactKeyPackage,
    pub user_profile_key: UserProfileKey,
}

#[derive(Debug, Clone)]
pub(crate) enum ContactKeyPackage {
    Traditional(Box<KeyPackage>),
    Apq(Box<ApqKeyPackage>),
}

impl Contact {
    pub(crate) async fn fetch_add_infos(
        &self,
        mut connection: impl WriteConnection,
        api_clients: &ApiClients,
        is_apq: bool,
    ) -> Result<ContactAddInfos> {
        let invited_user_domain = self.user_id.domain();

        let key_package = if is_apq {
            let key_package_in = api_clients
                .get(invited_user_domain)?
                .qs_apq_key_package(self.friendship_token.clone())
                .await?;
            let key_package = key_package_in.validate(&RustCrypto::default())?;
            ContactKeyPackage::Apq(key_package.into())
        } else {
            let key_package_in = api_clients
                .get(invited_user_domain)?
                .qs_key_package(self.friendship_token.clone())
                .await?
                .key_package;
            let key_package =
                key_package_in.validate(&RustCrypto::default(), ProtocolVersion::default())?;
            ContactKeyPackage::Traditional(key_package.into())
        };

        // Verify the client credential
        let client_credential = match &key_package {
            ContactKeyPackage::Traditional(key_package) => key_package.leaf_node().credential(),
            ContactKeyPackage::Apq(key_package) => {
                let t_credential = key_package.t_credential();
                let pq_credential = key_package.pq_credential();
                ensure!(
                    t_credential == pq_credential,
                    "APQ key packages must have the same credentials"
                );
                t_credential
            }
        };
        let verifiable_client_credential =
            VerifiableClientCredential::from_basic_credential(client_credential)?;
        let as_credential = connection
            .with_transaction(async |txn| {
                AsCredentials::fetch_for_verification(
                    txn,
                    api_clients,
                    iter::once(&verifiable_client_credential),
                )
                .await
            })
            .await?;
        let incoming_client_credential =
            StorableClientCredential::verify(verifiable_client_credential, &as_credential)?;

        // Check that the client credential is the same as the one we have on file.
        let current_client_credential = StorableClientCredential::load_by_user_id(
            &mut connection,
            incoming_client_credential.user_id(),
        )
        .await?
        .context("Client credential not found")?;
        if current_client_credential.fingerprint() != incoming_client_credential.fingerprint() {
            bail!("Client credential does not match");
        }

        let user_profile = IndexedUserProfile::load(&mut connection, &self.user_id)
            .await?
            .context("User profile not found")?;
        let user_profile_key =
            UserProfileKey::load(connection, user_profile.decryption_key_index()).await?;

        Ok(ContactAddInfos {
            key_package,
            user_profile_key,
        })
    }

    pub(crate) fn wai_ear_key(&self) -> &WelcomeAttributionInfoEarKey {
        &self.wai_ear_key
    }

    /// Augment the supported features from the contact's connection group.
    pub(crate) async fn augment_supported_features(
        &mut self,
        connection: impl ReadConnection,
    ) -> sqlx::Result<()> {
        if let Some(group) = Group::load_by_connection_user_id(connection, &self.user_id).await?
            && let Some(air_component) = group.member_air_component(&self.user_id)
        {
            self.supported_features = Some(air_component.features);
        }
        Ok(())
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
