// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Logged-in user feature

use std::sync::Arc;

pub(crate) use aircommon::identifiers::UsernameHash;
use aircommon::identifiers::{UserId, Username};
pub(crate) use aircoreclient::InviteUsersError;
use aircoreclient::{Asset, ChatId, ContactType, PartialContact, clients::CoreUser, store::Store};
use anyhow::ensure;
use flutter_rust_bridge::frb;
use qs::QueueContext;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use username::{UsernameBackgroundTasks, UsernameContext};

use crate::api::types::UiContact;
use crate::{
    StreamSink,
    api::navigation_cubit::HomeNavigationState,
    notifications::NotificationService,
    util::{Cubit, CubitCore, spawn_from_sync},
};

use super::{
    navigation_cubit::{NavigationCubitBase, NavigationState},
    notifications::NotificationContent,
    types::{UiUserId, UiUsername},
    user::User,
};

mod qs;
mod username;

const DELETE_ACCOUNT_CONFIRMATION_TEXT: &str = "delete";

/// State of the [`UserCubit`] which is the logged in user
///
/// Opaque, cheaply cloneable, copy-on-write type
///
/// Note: This has a prefix `Ui` to avoid conflicts with the `User`.
//
// TODO: Currently, frb does not support exposing eq and hash to Dart. When it is possible, we
// should do it, to minimize the amount of UI rebuilds in Flutter.
//
// See:
// * <https://github.com/phnx-im/air/issues/247>
// * <https://github.com/fzyzcjy/flutter_rust_bridge/issues/2238>
#[frb(opaque)]
#[derive(Debug, Clone)]
pub struct UiUser {
    inner: Arc<UiUserInner>,
}

#[frb(ignore)]
#[derive(Debug, Clone)]
struct UiUserInner {
    user_id: UserId,
    usernames: Vec<Username>,
    unsupported_version: bool,
}

impl UiUser {
    fn new(inner: Arc<UiUserInner>) -> Self {
        Self { inner }
    }

    /// Loads state in the background
    fn spawn_load(state_tx: watch::Sender<UiUser>, core_user: CoreUser) {
        spawn_from_sync(async move {
            match core_user.usernames().await {
                Ok(usernames) => {
                    state_tx.send_modify(|state| {
                        let inner = Arc::make_mut(&mut state.inner);
                        inner.usernames = usernames;
                    });
                }
                Err(error) => {
                    error!(%error, "failed to load usernames");
                }
            }
        });
    }

    #[frb(getter, sync)]
    pub fn user_id(&self) -> UiUserId {
        self.inner.user_id.clone().into()
    }

    #[frb(getter, sync)]
    pub fn usernames(&self) -> Vec<UiUsername> {
        self.inner
            .usernames
            .iter()
            .cloned()
            .map(From::from)
            .collect()
    }

    #[frb(getter, sync)]
    pub fn unsupported_version(&self) -> bool {
        self.inner.unsupported_version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    MobileBackground,
    DesktopBackground,
    Foreground,
}

/// Provides access to the logged in user and their profile.
///
/// Also listens to queue service messages and fetches updates from the server. The lifetime of the
/// listening stream is tied to the lifetime of the cubit.
///
/// This cubit should not be created more than once, because the logged in user exists in the
/// system only once.
///
/// Allows other cubits to listen to the messages fetched from the server. In this regard, it is
/// special because it is a construction entry point of other cubits.
#[frb(opaque)]
pub struct UserCubitBase {
    core: CubitCore<UiUser>,
    context: CubitContext,
    app_state_tx: watch::Sender<AppState>,
    background_listen_username_tasks: UsernameBackgroundTasks,
    cancel: CancellationToken,
}

impl UserCubitBase {
    #[frb(sync)]
    pub fn new(user: &User, navigation: &NavigationCubitBase) -> Self {
        let core_user = user.user.clone();

        let core = CubitCore::with_initial_state(UiUser::new(Arc::new(UiUserInner {
            user_id: user.user.user_id().clone(),
            usernames: Vec::new(),
            unsupported_version: false,
        })));

        UiUser::spawn_load(core.state_tx().clone(), core_user.clone());

        let navigation_state = navigation.subscribe();
        let notification_service = navigation.notification_service.clone();

        let (app_state_tx, app_state) = watch::channel(AppState::Foreground);

        let cancel = CancellationToken::new();

        let context = CubitContext {
            state_tx: core.state_tx().clone(),
            core_user,
            app_state,
            navigation_state,
            notification_service,
        };

        // emit persisted store notifications
        context.spawn_emit_stored_notifications(cancel.clone());

        // start background task listening for incoming messages
        QueueContext::new(context.clone())
            .into_task(cancel.clone())
            .spawn();

        // start background tasks listening for incoming username messages
        let background_listen_username_tasks =
            UsernameContext::spawn_loading(context.clone(), cancel.clone());

        Self {
            core,
            context,
            app_state_tx,
            background_listen_username_tasks,
            cancel: cancel.clone(),
        }
    }

    #[frb(ignore)]
    pub(crate) fn core_user(&self) -> &CoreUser {
        &self.context.core_user
    }

    #[frb(ignore)]
    pub(crate) fn notification_service(&self) -> &NotificationService {
        &self.context.notification_service
    }

    // Cubit interface

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    pub fn close(&self) {
        self.core.close();
        self.cancel.cancel();
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> UiUser {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<UiUser>) {
        self.core.stream(sink).await;
    }

    // Cubit methods

    /// Set the display name and/or profile picture of the user.
    pub async fn set_profile(
        &self,
        display_name: Option<String>,
        profile_picture: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        let display_name = display_name.map(|s| s.parse()).transpose()?;
        let profile_picture = profile_picture.map(Asset::Value);

        let mut profile = self.context.core_user.own_user_profile().await?;
        if let Some(value) = display_name {
            profile.display_name = value;
        }
        if let Some(value) = profile_picture {
            profile.profile_picture = Some(value);
        }
        self.context.core_user.set_own_user_profile(profile).await?;

        Ok(())
    }

    /// Adds multiple users to the chat with the given [`ChatId`].
    ///
    /// If one of the users cannot be added, an error is returned and the chat is not modified,
    /// that is, other users are *not* added to the chat too.
    //
    // Note: We use the `Result<Option<_>, _>` return type because FRB does not support generics
    // and so we cannot propagate the result directly.
    #[frb(positional)]
    pub async fn add_users_to_chat(
        &self,
        chat_id: ChatId,
        user_ids: Vec<UiUserId>,
    ) -> anyhow::Result<Option<InviteUsersError>> {
        let user_ids: Vec<_> = user_ids.into_iter().map(From::from).collect();
        Ok(self
            .context
            .core_user
            .invite_users(chat_id, &user_ids)
            .await?
            .err())
    }

    #[frb(positional)]
    pub async fn remove_user_from_chat(
        &self,
        chat_id: ChatId,
        user_id: UiUserId,
    ) -> anyhow::Result<()> {
        self.context
            .core_user
            .remove_users(chat_id, vec![user_id.into()])
            .await?;
        Ok(())
    }

    #[frb(positional)]
    pub async fn delete_chat(&self, chat_id: ChatId) -> anyhow::Result<()> {
        self.context
            .core_user
            .delete_chat(chat_id)
            .await
            .inspect_err(|error| {
                error!(%error, "failed to delete conversion; skipping");
            })
            .ok();
        self.context.core_user.erase_chat(chat_id).await?;
        Ok(())
    }

    #[frb(positional)]
    pub async fn leave_chat(&self, chat_id: ChatId) -> anyhow::Result<()> {
        self.context.core_user.leave_chat(chat_id).await
    }

    #[frb(getter)]
    pub async fn contacts(&self) -> anyhow::Result<Vec<UiContact>> {
        let contacts = self
            .context
            .core_user
            .contacts_with_supported_features()
            .await?;
        Ok(contacts.into_iter().map(From::from).collect())
    }

    pub async fn contact(&self, user_id: UiUserId) -> anyhow::Result<Option<UiContact>> {
        let Some(contact) = Store::contact(&self.context.core_user, &user_id.into()).await? else {
            return Ok(None);
        };
        match contact {
            ContactType::Full(contact) => Ok(Some(contact.into())),
            ContactType::Partial(PartialContact::TargetedMessage(contact)) => {
                Ok(Some(contact.into()))
            }
            ContactType::Partial(PartialContact::Username(_)) => Ok(None),
        }
    }

    pub async fn addable_contacts(&self, chat_id: ChatId) -> anyhow::Result<Vec<UiContact>> {
        let Some(members) = self.context.core_user.chat_participants(chat_id).await else {
            return Ok(vec![]);
        };
        let mut contacts = self.contacts().await.unwrap_or_default();
        // Retain only those contacts that are not already in the chat
        contacts.retain(|contact| {
            !members
                .iter()
                .any(|member| member.uuid() == contact.user_id.uuid)
        });
        Ok(contacts)
    }

    pub fn set_app_state(&self, _app_state: AppState) {
        let app_state = _app_state;
        debug!(?app_state, "app state changed");
        let _no_receivers = self.app_state_tx.send(app_state);
    }

    pub async fn add_username(&self, username: UiUsername) -> anyhow::Result<bool> {
        let username = Username::new(username.plaintext)?;
        let Some(record) = self
            .context
            .core_user
            .add_username(username.clone())
            .await?
        else {
            return Ok(false);
        };

        // add username to UI state
        self.core.state_tx().send_modify(|state| {
            let inner = Arc::make_mut(&mut state.inner);
            inner.usernames.push(username);
        });

        // start background listen stream for the username
        UsernameContext::new(self.context.clone(), record)
            .into_task(
                self.cancel.child_token(),
                &self.background_listen_username_tasks,
            )
            .spawn();

        Ok(true)
    }

    pub async fn remove_username(&self, username: UiUsername) -> anyhow::Result<()> {
        let username = Username::new(username.plaintext)?;
        self.context.core_user.remove_username(&username).await?;

        // remove username from UI state
        self.core.state_tx().send_if_modified(|state| {
            let inner = Arc::make_mut(&mut state.inner);
            let Some(idx) = inner.usernames.iter().position(|u| u == &username) else {
                error!("username is not found");
                return false;
            };
            inner.usernames.remove(idx);
            true
        });

        // stop background listen stream for the username
        self.background_listen_username_tasks.remove(username);

        Ok(())
    }

    pub async fn report_spam(&self, spammer_id: UiUserId) -> anyhow::Result<()> {
        self.context.core_user.report_spam(spammer_id.into()).await
    }

    pub async fn block_contact(&self, user_id: UiUserId) -> anyhow::Result<()> {
        self.context.core_user.block_contact(user_id.into()).await
    }

    pub async fn unblock_contact(&self, user_id: UiUserId) -> anyhow::Result<()> {
        self.context.core_user.unblock_contact(user_id.into()).await
    }

    pub async fn delete_account(
        &self,
        db_path: &str,
        confirmation_text: &str,
    ) -> anyhow::Result<()> {
        ensure!(
            confirmation_text == DELETE_ACCOUNT_CONFIRMATION_TEXT,
            "unexpected confirmation text"
        );
        self.context.core_user.delete_account(Some(db_path)).await
    }

    pub async fn add_contact_from_group(
        &self,
        chat_id: ChatId,
        user_id: UiUserId,
    ) -> anyhow::Result<ChatId> {
        self.context
            .core_user
            .add_contact_from_group(chat_id, user_id.into())
            .await
    }

    pub async fn check_username_exists(
        &self,
        username: UiUsername,
    ) -> anyhow::Result<Option<UsernameHash>> {
        let username = Username::new(username.plaintext)?;
        self.context.core_user.check_username_exists(username).await
    }

    /// Returns the pair of safety codes of the logged-in user and the given user.
    ///
    /// The order of the codes is stable and is determined by their lexicographical order.
    #[frb(type_64bit_int)]
    pub async fn safety_codes(&self, other_user_id: UiUserId) -> anyhow::Result<[u64; 12]> {
        let mut first = self
            .context
            .core_user
            .safety_code(self.context.core_user.user_id())
            .await?;
        let mut second = self
            .context
            .core_user
            .safety_code(&other_user_id.into())
            .await?;
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        let mut code = [0; 12];
        let (prefix, suffix) = code.split_at_mut(6);
        prefix.copy_from_slice(&first.to_chunks());
        suffix.copy_from_slice(&second.to_chunks());
        Ok(code)
    }
}

impl Drop for UserCubitBase {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

/// Reusable context of this cubit in background tasks.
#[frb(ignore)]
#[derive(Debug, Clone)]
struct CubitContext {
    state_tx: watch::Sender<UiUser>,
    core_user: CoreUser,
    app_state: watch::Receiver<AppState>,
    navigation_state: watch::Receiver<NavigationState>,
    notification_service: NotificationService,
}

impl CubitContext {
    fn spawn_emit_stored_notifications(&self, cancel: CancellationToken) {
        let core_user = self.core_user.clone();
        let app_state = self.app_state.clone();
        spawn_from_sync(async move {
            if let Err(error) = Self::emit_stored_notifications(core_user, app_state, cancel).await
            {
                error!(%error, "Failed to emit stored notifications");
            }
        });
    }

    /// Emit persisted store notifications.
    ///
    /// Background push handlers (iOS NSE, Android WorkManager) persist store
    /// notifications to the database. We drain them on two triggers:
    /// - when the app goes into the foreground, and
    /// - when we got a signal from the push handler.
    async fn emit_stored_notifications(
        core_user: CoreUser,
        mut app_state: watch::Receiver<AppState>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        let pending = core_user.store_notifications_pending();
        loop {
            let should_drain = tokio::select! {
                // We got cancelled, let's abort
                _ = cancel.cancelled() => return Ok(()),
                // State change, we only want to drain when we go into foreground
                _ = app_state.changed() => {
                    matches!(*app_state.borrow_and_update(), AppState::Foreground)
                }
                // We got a signal from the push handler that there are pending
                // notifications, let's drain
                _ = pending.notified() => true,
            };
            if !should_drain {
                // Nothing to do, wait for the next trigger
                continue;
            }
            // Finally eat these yummy notifications! Nom nom nom
            match core_user.dequeue_notification().await {
                Ok(store_notification) if !store_notification.is_empty() => {
                    core_user.notify(store_notification);
                }
                Ok(_) => {}
                Err(error) => {
                    error!(%error, "Failed to dequeue stored notifications");
                }
            }
        }
    }
}

/// Places in the app where notifications in foreground are handled differently.
///
/// Derived from the [`NavigationState`].
#[derive(Debug)]
enum NotificationContext {
    Intro,
    Chat(ChatId),
    ChatList,
    Other,
}

impl CubitContext {
    /// Show OS notifications depending on the current navigation state and OS.
    async fn show_notifications(&self, mut notifications: Vec<NotificationContent>) {
        const IS_DESKTOP: bool = cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        ));
        let notification_context = match &*self.navigation_state.borrow() {
            NavigationState::Intro { .. } => NotificationContext::Intro,
            NavigationState::Home {
                home:
                    HomeNavigationState {
                        chat_id: Some(chat_id),
                        ..
                    },
            } => NotificationContext::Chat(*chat_id),
            NavigationState::Home {
                home:
                    HomeNavigationState {
                        chat_id: None,
                        developer_settings_screen,
                        user_profile_open,
                        ..
                    },
            } => {
                if !IS_DESKTOP && developer_settings_screen.is_none() && !user_profile_open {
                    NotificationContext::ChatList
                } else {
                    NotificationContext::Other
                }
            }
        };

        debug!(?notifications, ?notification_context, "send_notification");

        match notification_context {
            NotificationContext::Intro | NotificationContext::ChatList => {
                return; // suppress all notifications
            }
            NotificationContext::Chat(chat_id) => {
                // We don't want to show notifications when
                // - we are on mobile and the notification belongs to the currently open chat
                // - we are on desktop, the app is in the foreground, and the notification belongs to the currently open chat
                let app_state = *self.app_state.borrow();
                if !IS_DESKTOP || app_state == AppState::Foreground {
                    notifications.retain(|notification| notification.chat_id != chat_id);
                }
            }
            NotificationContext::Other => (),
        }

        for notification in notifications {
            self.notification_service
                .show_notification(notification)
                .await;
        }
    }
}

#[frb(mirror(InviteUsersError))]
enum _InviteUsersError {
    IncompatibleClient { reason: String },
}
