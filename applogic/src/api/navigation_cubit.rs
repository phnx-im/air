// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::mem;

use aircoreclient::ChatId;
use flutter_rust_bridge::frb;
use tokio::sync::watch;

use crate::{
    StreamSink,
    notifications::NotificationService,
    util::{Cubit, CubitCore},
};

use super::{notifications::DartNotificationService, types::UiUserId};

/// State of the global App navigation
#[frb(dart_metadata = ("freezed"))]
#[derive(Debug, Clone, PartialEq, Eq, derive_more::From)]
pub enum NavigationState {
    /// Intro screen: welcome and registration screen
    Intro {
        #[frb(default = "[]")]
        screens: Vec<IntroScreenType>,
    },
    Home {
        #[frb(default = "HomeNavigationState()")]
        home: HomeNavigationState,
    },
}

/// Possible intro screens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub enum IntroScreenType {
    Intro,
    SignUp,
    DeveloperSettings(DeveloperSettingsScreenType),
}

/// Chats screen: main screen of the app
///
/// Note: this can be represented in a better way disallowing invalid states.
/// For now, following KISS we represent the navigation stack in a very simple
/// way by just storing true/false or an optional value representing if a
/// screen is opened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub struct HomeNavigationState {
    /// Indicates whether a chat is open independently of the state of the chat id.
    ///
    /// When this flag is true and a chat id is set, the chat is open. When it is
    /// false, no chat is open, even if the chat id is set.
    ///
    /// Allows to close a chat without setting the chat id to `None`.
    #[frb(default = false)]
    pub chat_open: bool,
    pub chat_id: Option<ChatId>,
    pub developer_settings_screen: Option<DeveloperSettingsScreenType>,
    /// User name of the member that details are currently open
    pub member_details: Option<UiUserId>,
    pub user_settings_screen: Option<UserSettingsScreenType>,
    #[frb(default = false)]
    pub chat_details_open: bool,
    #[frb(default = false)]
    pub add_members_open: bool,
    #[frb(default = false)]
    pub group_members_open: bool,
    #[frb(default = false)]
    pub create_group_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub enum DeveloperSettingsScreenType {
    Root,
    ChangeUser,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[frb(dart_metadata = ("freezed"))]
pub enum UserSettingsScreenType {
    Root,
    EditDisplayName,
    AddUserHandle,
    Help,
    DeleteAccount,
}

impl NavigationState {
    fn intro() -> Self {
        Self::Intro {
            screens: Vec::new(),
        }
    }

    fn home() -> NavigationState {
        Self::Home {
            home: HomeNavigationState::default(),
        }
    }
}

/// Provides the navigation state and navigation actions to the app
///
/// This is main entry point for navigation.
///
/// For the actual translation of the state to the actual screens, see
/// `AppRouter` in Dart.
pub struct NavigationCubitBase {
    core: CubitCore<NavigationState>,
    pub(crate) notification_service: NotificationService,
}

impl NavigationCubitBase {
    #[frb(sync)]
    pub fn new(notification_service: &DartNotificationService) -> Self {
        let core = CubitCore::with_initial_state(NavigationState::intro());
        Self {
            core,
            notification_service: NotificationService::new(notification_service.clone()),
        }
    }

    // Cubit interface

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    pub fn close(&mut self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> NavigationState {
        self.core.state()
    }

    pub async fn stream(&mut self, sink: StreamSink<NavigationState>) {
        self.core.stream(sink).await;
    }

    // Rust private methods

    #[frb(ignore)]
    pub(crate) fn subscribe(&self) -> watch::Receiver<NavigationState> {
        self.core.state_tx().subscribe()
    }

    // Cubit methods

    pub fn open_into(&self) {
        self.core.state_tx().send_modify(|state| {
            *state = NavigationState::intro();
        });
    }

    pub fn open_home(&self) {
        self.core.state_tx().send_modify(|state| {
            *state = NavigationState::home();
        });
    }

    pub async fn open_chat(&self, chat_id: ChatId) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => {
                *state = HomeNavigationState {
                    chat_open: true,
                    chat_id: Some(chat_id),
                    ..Default::default()
                }
                .into();
                true
            }
            NavigationState::Home { home } => {
                let was_open = mem::replace(&mut home.chat_open, true);
                let different_id = home.chat_id.replace(chat_id) != Some(chat_id);
                !was_open || different_id
            }
        });

        // Cancel the active notifications for the current chat
        let handles = self.notification_service.get_active_notifications().await;
        let identifiers = handles
            .into_iter()
            .filter_map(|handle| (handle.chat_id? == chat_id).then_some(handle.identifier))
            .collect();
        self.notification_service
            .cancel_notifications(identifiers)
            .await;
    }

    pub fn close_chat(&self) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => {
                let mut changed = false;
                if mem::replace(&mut home.chat_open, false) {
                    changed = true;
                }
                if mem::replace(&mut home.chat_details_open, false) {
                    changed = true;
                }
                if mem::replace(&mut home.add_members_open, false) {
                    changed = true;
                }
                if mem::replace(&mut home.group_members_open, false) {
                    changed = true;
                }
                if mem::replace(&mut home.create_group_open, false) {
                    changed = true;
                }
                if home.member_details.take().is_some() {
                    changed = true;
                }
                if home.chat_id.take().is_some() {
                    changed = true;
                }
                changed
            }
        });
    }

    pub fn open_member_details(&self, member: UiUserId) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => match home.member_details.as_mut() {
                Some(value) if *value != member => {
                    *value = member;
                    true
                }
                None => {
                    home.member_details.replace(member);
                    true
                }
                _ => false,
            },
        });
    }

    pub fn open_chat_details(&self) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => !mem::replace(&mut home.chat_details_open, true),
        });
    }

    pub fn open_add_members(&self) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => !mem::replace(&mut home.add_members_open, true),
        });
    }

    pub fn open_group_members(&self) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => !mem::replace(&mut home.group_members_open, true),
        });
    }

    pub fn open_create_group(&self) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => !mem::replace(&mut home.create_group_open, true),
        });
    }

    pub fn open_user_settings(&self, screen: UserSettingsScreenType) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { .. } => false,
            NavigationState::Home { home } => {
                home.user_settings_screen.replace(screen) != Some(screen)
            }
        });
    }

    pub fn open_developer_settings(&self, screen: DeveloperSettingsScreenType) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { screens } => match screens.last_mut() {
                Some(IntroScreenType::DeveloperSettings(DeveloperSettingsScreenType::Root)) => {
                    if screen != DeveloperSettingsScreenType::Root {
                        screens.push(IntroScreenType::DeveloperSettings(screen));
                        true
                    } else {
                        false
                    }
                }
                Some(IntroScreenType::DeveloperSettings(dev_screen)) => {
                    mem::replace(dev_screen, screen) == screen
                }
                _ => {
                    screens.push(IntroScreenType::DeveloperSettings(screen));
                    true
                }
            },
            NavigationState::Home { home } => {
                home.developer_settings_screen.replace(screen) != Some(screen)
            }
        });
    }

    pub fn open_intro_screen(&self, screen: IntroScreenType) {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { screens } => {
                if screens.last() != Some(&screen) {
                    screens.push(screen);
                    true
                } else {
                    false
                }
            }
            NavigationState::Home { .. } => false,
        });
    }

    #[frb(sync)]
    pub fn pop(&self) -> bool {
        self.core.state_tx().send_if_modified(|state| match state {
            NavigationState::Intro { screens } => screens.pop().is_some(),
            NavigationState::Home {
                home:
                    home @ HomeNavigationState {
                        developer_settings_screen: Some(DeveloperSettingsScreenType::Root),
                        ..
                    },
            } => {
                home.developer_settings_screen.take();
                true
            }
            NavigationState::Home {
                home:
                    home @ HomeNavigationState {
                        developer_settings_screen:
                            Some(
                                DeveloperSettingsScreenType::ChangeUser
                                | DeveloperSettingsScreenType::Logs,
                            ),
                        ..
                    },
            } => {
                home.developer_settings_screen
                    .replace(DeveloperSettingsScreenType::Root);
                true
            }
            NavigationState::Home {
                home:
                    home @ HomeNavigationState {
                        user_settings_screen: Some(UserSettingsScreenType::Root),
                        ..
                    },
            } => {
                home.user_settings_screen.take();
                true
            }
            NavigationState::Home {
                home:
                    home @ HomeNavigationState {
                        user_settings_screen:
                            Some(
                                UserSettingsScreenType::EditDisplayName
                                | UserSettingsScreenType::AddUserHandle
                                | UserSettingsScreenType::Help
                                | UserSettingsScreenType::DeleteAccount,
                            ),
                        ..
                    },
            } => {
                home.user_settings_screen
                    .replace(UserSettingsScreenType::Root);
                true
            }
            NavigationState::Home { home } if home.member_details.is_some() => {
                home.member_details.take();
                true
            }
            NavigationState::Home { home } if home.create_group_open => {
                home.create_group_open = false;
                true
            }
            NavigationState::Home { home } if home.chat_id.is_some() && home.add_members_open => {
                home.add_members_open = false;
                true
            }
            NavigationState::Home { home } if home.chat_id.is_some() && home.group_members_open => {
                home.group_members_open = false;
                true
            }
            NavigationState::Home { home } if home.chat_id.is_some() && home.chat_details_open => {
                home.chat_details_open = false;
                home.group_members_open = false;
                home.add_members_open = false;
                true
            }
            NavigationState::Home { home } if home.chat_id.is_some() => {
                home.chat_open = false;
                true
            }
            NavigationState::Home { .. } => false,
        })
    }
}
