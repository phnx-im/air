// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::{HashMap, hash_map},
    sync::Arc,
};

use aircommon::identifiers::UserId;
use aircoreclient::{
    DisplayName, UserProfile,
    clients::CoreUser,
    db::notification::{DbEntityId, DbNotification, DbOperation},
};
use flutter_rust_bridge::frb;
use tokio::sync::{mpsc, watch};
use tokio_stream::StreamExt;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::error;

use crate::{
    StreamSink,
    util::{Cubit, CubitCore, spawn_from_sync},
};

use super::{
    types::{ImageData, UiContactVerification, UiUserId, UiUserProfile},
    user_cubit::UserCubitBase,
};

/// Clone-on-write state of the [`UsersCubitBase`].
#[derive(Debug, Clone)]
#[frb(opaque)]
pub struct UsersState {
    inner: Arc<UsersStateInner>,
}

impl UsersState {
    /// Returns the profile of the given user.
    ///
    /// If the user is not specified, the profile of the logged-in user is returned.
    ///
    /// If the profile is not yet loaded, the default profile is returned and loading is spawned in
    /// the background.
    #[frb(sync)]
    pub fn profile(&self, user_id: Option<UiUserId>) -> UiUserProfile {
        let user_id: UserId = user_id
            .map(From::from)
            .unwrap_or_else(|| self.inner.own_user_id.clone());
        let profile = self
            .inner
            .users
            .get(&user_id)
            .map(|user| user.profile.clone());
        match profile {
            Some(profile) => profile,
            None => {
                // spawn loading profile and return default profile
                self.spawn_load_profile(user_id.clone());
                let default_profile = UserProfile::from_user_id(&user_id);
                UiUserProfile::from_profile(default_profile)
            }
        }
    }

    /// Returns the display name of the given user.
    ///
    /// If the user is not specified, the display name of the logged-in user is returned.
    ///
    /// If the profile is not yet loaded, the default display name is returned and loading of the
    /// profile is spawned in the background.
    #[frb(sync)]
    pub fn display_name(&self, user_id: Option<UiUserId>) -> String {
        let user_id = user_id
            .map(From::from)
            .unwrap_or_else(|| self.inner.own_user_id.clone());
        let display_name = self
            .inner
            .users
            .get(&user_id)
            .map(|user| user.profile.display_name.clone());
        match display_name {
            Some(display_name) => display_name,
            None => {
                // spawn loading profile and return default display name
                self.spawn_load_profile(user_id.clone());
                DisplayName::from_user_id(&user_id).into_string()
            }
        }
    }

    /// Returns the profile picture of the given user if any is set.
    ///
    /// If the user is not specified, the profile picture of the logged-in user is returned.
    ///
    /// If the profile is not yet loaded, `null` is returned and loading of the profile is spawned
    /// in the background.
    #[frb(sync)]
    pub fn profile_picture(&self, user_id: Option<UiUserId>) -> Option<ImageData> {
        let user_id = user_id
            .map(From::from)
            .unwrap_or_else(|| self.inner.own_user_id.clone());
        let profile_picture = self
            .inner
            .users
            .get(&user_id)
            .map(|user| user.profile.profile_picture.clone());
        match profile_picture {
            Some(profile_picture) => profile_picture,
            None => {
                // spawn loading profile
                self.spawn_load_profile(user_id.clone());
                None
            }
        }
    }

    /// Returns whether the given user is marked as verified.
    ///
    /// If the profile is not yet loaded, `Unverified` is returned and loading of the profile is
    /// spawned in the background.
    #[frb(sync)]
    pub fn verification(&self, user_id: UiUserId) -> UiContactVerification {
        let user_id = user_id.into();
        let verification = self.inner.users.get(&user_id).map(|user| user.verification);
        match verification {
            Some(verification) => verification,
            None => {
                self.spawn_load_profile(user_id);
                UiContactVerification::Unverified
            }
        }
    }

    #[frb(ignore)]
    fn spawn_load_profile(&self, user_id: UserId) {
        let tx = self.inner.load_profile_tx.clone();
        spawn_from_sync(async move {
            let _ = tx.send(user_id).await.inspect_err(|error| {
                error!(%error, "Failed to send load profile request");
            });
        });
    }
}

#[derive(Debug, Clone)]
#[frb(ignore)]
pub(crate) struct UsersStateInner {
    load_profile_tx: mpsc::Sender<UserId>,
    own_user_id: UserId,
    users: HashMap<UserId, LoadedUser>,
}

impl UsersStateInner {
    fn new(own_user_id: UserId, load_profile_tx: mpsc::Sender<UserId>) -> Self {
        Self {
            load_profile_tx,
            own_user_id,
            users: HashMap::new(),
        }
    }

    /// Returns `true` if the user was changed, otherwise `false`.
    ///
    /// A user is not changed if the profile and the verification are the same as the current
    /// ones.
    fn set_user(&mut self, user_id: UserId, user: LoadedUser) -> bool {
        match self.users.entry(user_id) {
            hash_map::Entry::Occupied(mut entry) => {
                if entry.get() != &user {
                    entry.insert(user);
                    true
                } else {
                    false
                }
            }
            hash_map::Entry::Vacant(entry) => {
                entry.insert(user);
                true
            }
        }
    }
}

/// What the state keeps about a user.
#[derive(Debug, Clone, PartialEq)]
struct LoadedUser {
    profile: UiUserProfile,
    verification: UiContactVerification,
}

impl LoadedUser {
    async fn load(core_user: &CoreUser, user_id: &UserId) -> Self {
        let profile = UiUserProfile::from_profile(core_user.user_profile(user_id).await);
        let verification = core_user
            .contact_verification(user_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to load the contact verification"))
            .map_or(UiContactVerification::Unverified, From::from);
        Self {
            profile,
            verification,
        }
    }
}

/// Provides synchronous access to the profiles of users and their verification.
///
/// Caches already loaded profiles. Loads profiles on first access in the background. Automatically
/// reloads profiles when these are changed/removed.
pub struct UsersCubitBase {
    core: CubitCore<UsersState>,
    _cancel: DropGuard,
}

impl UsersCubitBase {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let store = user_cubit.core_user().clone();

        /// Commit all drafts (some might be uncommitted, because the app was closed while editing).
        spawn_from_sync({
            let store = store.clone();
            async move {
                if let Err(error) = store.commit_all_message_drafts().await {
                    error!(%error, "Failed to commit all message drafts");
                }
            }
        });

        let (load_profile_tx, load_profile_rx) = mpsc::channel(1024);
        let inner = UsersStateInner::new(store.user_id().clone(), load_profile_tx);
        let core = CubitCore::with_initial_state(UsersState {
            inner: Arc::new(inner),
        });

        let cancel = CancellationToken::new();
        ProfileLoadingTask::new(
            store,
            core.state_tx().clone(),
            load_profile_rx,
            cancel.clone(),
        )
        .spawn();

        Self {
            core,
            _cancel: cancel.drop_guard(),
        }
    }

    // Cubit interface

    pub fn close(&self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> UsersState {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<UsersState>) {
        self.core.stream(sink).await;
    }
}

/// Loads the profile and the verification of users in the background.
///
/// The profiles are loaded (or removed) when
///
/// 1. there is a store notification about a user profile change/removal, or
/// 2. profile loading is requested from the state.
struct ProfileLoadingTask {
    core_user: CoreUser,
    state_tx: watch::Sender<UsersState>,
    load_profile_rx: mpsc::Receiver<UserId>,
    cancel: CancellationToken,
}

impl ProfileLoadingTask {
    fn new(
        core_user: CoreUser,
        state_tx: watch::Sender<UsersState>,
        load_profile_rx: mpsc::Receiver<UserId>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            core_user,
            state_tx,
            load_profile_rx,
            cancel,
        }
    }

    fn spawn(self) {
        spawn_from_sync(self.process());
    }

    async fn process(mut self) -> Option<()> {
        let mut store_notifications = self.core_user.db_notifications();
        loop {
            // wait for the next store notification, explicit load profile request or cancellation
            let changed_profiles = tokio::select! {
                notification = store_notifications.next() => {
                    self.process_notification(notification?).await
                }
                user_id = self.load_profile_rx.recv() => {
                    let user_id = user_id?;
                    // Every rebuild asks again for a user until the first load
                    // lands. Notifications keep a loaded user up to date.
                    if self.state_tx.borrow().inner.users.contains_key(&user_id) {
                        continue;
                    }
                    let user = LoadedUser::load(&self.core_user, &user_id).await;
                    vec![(user_id, Some(user))]
                }
                _ = self.cancel.cancelled() => return None,
            };

            // update the state
            self.state_tx.send_if_modified(|state| {
                let inner = Arc::make_mut(&mut state.inner);
                let mut modified = false;
                for (user_id, user) in changed_profiles {
                    if let Some(user) = user {
                        if inner.set_user(user_id, user) {
                            modified = true;
                        }
                    } else {
                        inner.users.remove(&user_id);
                        modified = true;
                    }
                }
                modified
            });
        }
    }

    async fn process_notification(
        &self,
        notification: Arc<DbNotification>,
    ) -> Vec<(UserId, Option<LoadedUser>)> {
        let mut changed_profiles = Vec::new();
        for (entity_id, op) in notification.ops.iter() {
            let DbEntityId::User(user_id) = entity_id else {
                continue;
            };

            let is_loaded = self.state_tx.borrow().inner.users.contains_key(user_id);
            if !is_loaded {
                continue;
            }

            // We consider Add/Update to be of higher precedence than Remove
            if op.contains(DbOperation::Add) || op.contains(DbOperation::Update) {
                changed_profiles.push((
                    user_id.clone(),
                    Some(LoadedUser::load(&self.core_user, user_id).await),
                ));
            } else if op.contains(DbOperation::Remove) {
                changed_profiles.push((user_id.clone(), None));
            }
        }
        changed_profiles
    }
}
