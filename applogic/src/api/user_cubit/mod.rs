// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Logged-in user feature

use std::sync::Arc;

use aircommon::identifiers::{UserHandle, UserId};
use aircoreclient::Asset;
use aircoreclient::{ChatId, clients::CoreUser, store::Store};
use anyhow::ensure;
use flutter_rust_bridge::frb;
use qs::QueueContext;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use user_handle::{HandleBackgroundTasks, HandleContext};

use crate::api::{navigation_cubit::NavigationChat, types::UiContact};
use crate::{
    StreamSink,
    api::navigation_cubit::HomeNavigationState,
    notifications::NotificationService,
    util::{Cubit, CubitCore, spawn_from_sync},
};

use super::{
    navigation_cubit::{NavigationCubitBase, NavigationState},
    notifications::NotificationContent,
    types::{UiUserHandle, UiUserId},
    user::User,
};

mod qs;
mod user_handle;

const DELETE_ACCOUNT_CONFIRMATION_TEXT: &str = "delete";

/// State of the [`UserCubit`] which is the logged in user
///
/// Opaque, cheaply clonable, copy-on-write type
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
    user_handles: Vec<UserHandle>,
}

impl UiUser {
    fn new(inner: Arc<UiUserInner>) -> Self {
        Self { inner }
    }

    /// Loads state in the background
    fn spawn_load(state_tx: watch::Sender<UiUser>, core_user: CoreUser) {
        spawn_from_sync(async move {
            match core_user.user_handles().await {
                Ok(handles) => {
                    state_tx.send_modify(|state| {
                        let inner = Arc::make_mut(&mut state.inner);
                        inner.user_handles = handles;
                    });
                }
                Err(error) => {
                    error!(%error, "failed to load user handles");
                }
            }
        });
    }

    #[frb(getter, sync)]
    pub fn user_id(&self) -> UiUserId {
        self.inner.user_id.clone().into()
    }

    #[frb(getter, sync)]
    pub fn user_handles(&self) -> Vec<UiUserHandle> {
        self.inner
            .user_handles
            .iter()
            .cloned()
            .map(From::from)
            .collect()
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
/// special because it is a constuction entry point of other cubits.
#[frb(opaque)]
pub struct UserCubitBase {
    core: CubitCore<UiUser>,
    context: CubitContext,
    app_state_tx: watch::Sender<AppState>,
    background_listen_handle_tasks: HandleBackgroundTasks,
    cancel: CancellationToken,
}

impl UserCubitBase {
    #[frb(sync)]
    pub fn new(user: &User, navigation: &NavigationCubitBase) -> Self {
        let core_user = user.user.clone();
        let core = CubitCore::with_initial_state(UiUser::new(Arc::new(UiUserInner {
            user_id: user.user.user_id().clone(),
            user_handles: Vec::new(),
        })));

        UiUser::spawn_load(core.state_tx().clone(), core_user.clone());

        let navigation_state = navigation.subscribe();
        let notification_service = navigation.notification_service.clone();

        let (app_state_tx, app_state) = watch::channel(AppState::Foreground);

        let cancel = CancellationToken::new();

        let context = CubitContext {
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

        // start background tasks listening for incoming handle messages
        let background_listen_handle_tasks =
            HandleContext::spawn_loading(context.clone(), cancel.clone());

        Self {
            core,
            context,
            app_state_tx,
            background_listen_handle_tasks,
            cancel: cancel.clone(),
        }
    }

    #[frb(ignore)]
    pub(crate) fn core_user(&self) -> &CoreUser {
        &self.context.core_user
    }

    // Cubit inteface

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    pub fn close(&mut self) {
        self.core.close();
        self.cancel.cancel();
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> UiUser {
        self.core.state()
    }

    pub async fn stream(&mut self, sink: StreamSink<UiUser>) {
        self.core.stream(sink).await;
    }

    // Cubit methods

    /// Set the display name and/or profile picture of the user.
    pub async fn set_profile(
        &mut self,
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

    #[frb(positional)]
    pub async fn add_user_to_chat(&self, chat_id: ChatId, user_id: UiUserId) -> anyhow::Result<()> {
        self.context
            .core_user
            .invite_users(chat_id, &[user_id.into()])
            .await?;
        Ok(())
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
        let contacts = self.context.core_user.contacts().await?;
        Ok(contacts.into_iter().map(From::from).collect())
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

    pub async fn add_user_handle(&mut self, user_handle: UiUserHandle) -> anyhow::Result<bool> {
        let user_handle = UserHandle::new(user_handle.plaintext)?;
        let Some(record) = self
            .context
            .core_user
            .add_user_handle(user_handle.clone())
            .await?
        else {
            return Ok(false);
        };

        // add user handle to UI state
        self.core.state_tx().send_modify(|state| {
            let inner = Arc::make_mut(&mut state.inner);
            inner.user_handles.push(user_handle);
        });

        // start background listen stream for the handle
        HandleContext::new(self.context.clone(), record)
            .into_task(
                self.cancel.child_token(),
                &self.background_listen_handle_tasks,
            )
            .spawn();

        Ok(true)
    }

    pub async fn remove_user_handle(&mut self, user_handle: UiUserHandle) -> anyhow::Result<()> {
        let user_handle = UserHandle::new(user_handle.plaintext)?;
        self.context
            .core_user
            .remove_user_handle(&user_handle)
            .await?;

        // remove user handle from UI state
        self.core.state_tx().send_if_modified(|state| {
            let inner = Arc::make_mut(&mut state.inner);
            let Some(idx) = inner
                .user_handles
                .iter()
                .position(|handle| handle == &user_handle)
            else {
                error!("user handle is not found");
                return false;
            };
            inner.user_handles.remove(idx);
            true
        });

        // stop background listen stream for the handle
        self.background_listen_handle_tasks.remove(user_handle);

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

    /// Emit persisted store notifications when the app goes in the foreground.
    ///
    /// Store notification is stored in the database in the background process.
    async fn emit_stored_notifications(
        core_user: CoreUser,
        mut app_state: watch::Receiver<AppState>,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = app_state.changed() => {}
            };

            let state = *app_state.borrow_and_update();
            if let AppState::Foreground = state {
                match core_user.dequeue_notification().await {
                    Ok(store_notification) => {
                        if !store_notification.is_empty() {
                            core_user.notify(store_notification);
                        }
                    }
                    Err(error) => {
                        error!(%error, "Failed to dequeue stored notifications");
                    }
                }
            }
        }
    }
}

/// Places in the app where notifications in foreground are handled differently.
///
/// Dervived from the [`NavigationState`].
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
                        current_chat: NavigationChat::Open(chat_id),
                        ..
                    },
            } => NotificationContext::Chat(*chat_id),
            NavigationState::Home {
                home:
                    HomeNavigationState {
                        current_chat,
                        developer_settings_screen,
                        user_profile_open,
                        ..
                    },
            } => {
                if !IS_DESKTOP
                    && !current_chat.is_open()
                    && developer_settings_screen.is_none()
                    && !user_profile_open
                {
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
                    notifications.retain(|notification| notification.chat_id != Some(chat_id));
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
