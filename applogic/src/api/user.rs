// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! User features

use std::{cmp::Reverse, sync::LazyLock};

use aircommon::{
    DEFAULT_PORT_GRPC, crypto::ear::keys::DatabaseKek, identifiers::UserId,
    messages::push_token::PushTokenOperator,
};
use aircoreclient::{
    Asset, UserProfile,
    clients::{
        CoreUser,
        store::{ClientRecord, ClientRecordState},
    },
    open_client_db,
};
use anyhow::{Context, Result};
use flutter_rust_bridge::frb;
use tracing::error;

pub(crate) use aircommon::messages::push_token::PushToken;
use url::{Host, Url};
use uuid::Uuid;

use super::types::{UiClientRecord, UiUserId, UiUserProfile};

// TODO: This needs to be changed
static DEFAULT_DATABASE_KEK: LazyLock<DatabaseKek> =
    LazyLock::new(|| DatabaseKek::from_bytes(*b"default_db_kek______with_padding"));

/// Platform specific push token
pub enum PlatformPushToken {
    Apple(String),
    Google(String),
}

impl From<PlatformPushToken> for PushToken {
    fn from(platform_push_token: PlatformPushToken) -> Self {
        match platform_push_token {
            PlatformPushToken::Apple(token) => PushToken::new(PushTokenOperator::Apple, token),
            PlatformPushToken::Google(token) => PushToken::new(PushTokenOperator::Google, token),
        }
    }
}

/// The user of the app
///
/// Reponsible for loading or creating/registering the user.
// TODO: Most likely, it makes sense to move this to the `user_cubit` module. The loading and
// creation can be free functions there. The other functionality can be attach to the `UserCubit`.
//
// See <https://github.com/phnx-im/air/issues/297>
pub struct User {
    pub(crate) user: CoreUser,
}

impl User {
    pub(crate) fn from_core_user(core_user: CoreUser) -> Self {
        Self { user: core_user }
    }

    /// Creates a new user with a generated `uuid` at the domain described by `address`.
    pub async fn new(
        address: String,
        path: String,
        push_token: Option<PlatformPushToken>,
        display_name: String,
        profile_picture: Option<Vec<u8>>,
    ) -> Result<User> {
        let mut server_url: Url = address.parse()?;
        let domain = server_url.host().context("missing host in server url")?;

        let user_id = UserId::new(Uuid::new_v4(), domain.to_owned().into());

        if domain == Host::Domain("air.ms") {
            // TODO: For now, we redirect to the production server manually.
            server_url.set_host(Some("prod.air.ms"))?;
        }

        let user = CoreUser::new(
            user_id,
            server_url,
            DEFAULT_PORT_GRPC,
            &path,
            push_token.map(|p| p.into()),
            &DEFAULT_DATABASE_KEK,
        )
        .await?;

        let user_profile = UserProfile {
            user_id: user.user_id().clone(),
            display_name: display_name.parse()?,
            profile_picture: profile_picture.map(Asset::Value),
        };

        if let Err(error) = CoreUser::set_own_user_profile(&user, user_profile).await {
            error!(%error, "Could not set own user profile");
        }

        Ok(Self { user })
    }

    /// Loads all client records from the air database
    ///
    /// Also tries to load user profile from the client database. In case the client database
    /// cannot be opened, the client record is skipped.
    pub async fn load_client_records(db_path: String) -> Result<Vec<UiClientRecord>> {
        let mut ui_records = Vec::new();
        for record in ClientRecord::load_all_from_air_db(&db_path).await? {
            match load_ui_record(&db_path, &record).await {
                Ok(record) => ui_records.push(record),
                Err(error) => {
                    error!(%error, ?record.user_id, "failed to load client record");
                }
            }
        }
        ui_records.reverse();
        Ok(ui_records)
    }

    pub async fn load(db_path: String, user_id: UiUserId) -> anyhow::Result<Self> {
        let user = CoreUser::load(user_id.into(), &db_path, &DEFAULT_DATABASE_KEK).await?;
        Ok(Self { user: user.clone() })
    }

    /// Loads the default user from the given database path
    ///
    /// Returns in this order:
    /// * the default most recent user with finished registation, or if none
    /// * the most recent user with finished registration, or if none
    /// * the most recent user, if any.
    pub async fn load_default(path: String) -> Result<Option<Self>> {
        let mut records = ClientRecord::load_all_from_air_db(&path).await?;
        records.sort_unstable_by_key(|record| {
            let is_finished = matches!(record.client_record_state, ClientRecordState::Finished);
            Reverse((record.is_default, is_finished, record.created_at))
        });

        let mut loaded_user = None;
        for client_record in records {
            let user_id = client_record.user_id;
            match CoreUser::load(user_id.clone(), &path, &DEFAULT_DATABASE_KEK).await {
                Ok(user) => {
                    loaded_user = Some(user);
                    break;
                }
                Err(error) => error!(?user_id, %error, "Failed to load user"),
            };
        }

        let Some(user) = loaded_user else {
            return Ok(None);
        };

        Ok(Some(Self { user }))
    }

    /// Update the push token.
    #[frb(positional)]
    pub async fn update_push_token(&self, push_token: Option<PlatformPushToken>) -> Result<()> {
        self.user
            .update_push_token(push_token.map(|p| p.into()))
            .await?;
        Ok(())
    }

    /// Total number of unread messages across all chats
    #[frb(getter, type_64bit_int)]
    pub async fn global_unread_messages_count(&self) -> usize {
        self.user
            .global_unread_messages_count()
            .await
            .unwrap_or_default()
    }

    /// The unique identifier of the logged in user
    #[frb(getter, sync)]
    pub fn user_id(&self) -> UiUserId {
        self.user.user_id().clone().into()
    }
}

async fn load_ui_record(db_path: &str, record: &ClientRecord) -> anyhow::Result<UiClientRecord> {
    let pool = open_client_db(&record.user_id, db_path, &DEFAULT_DATABASE_KEK).await?;
    let user_profile = UserProfile::load(&pool, &record.user_id)
        .await?
        .map(UiUserProfile::from_profile)
        .unwrap_or_else(|| UiUserProfile::from_user_id(record.user_id.clone()));
    Ok(UiClientRecord {
        user_id: record.user_id.clone().into(),
        created_at: record.created_at,
        user_profile,
        is_finished: record.client_record_state == ClientRecordState::Finished,
    })
}
