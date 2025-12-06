// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Application logic API exposed to the Flutter app
//!
//! Cubits are the main building blocks of the app. Each cubit represents a feature of the app. It
//! is responsible for defining the data (state) for the feature and for exposing it to the UI. It
//! also loads the state, listens to changes to it and updates it accordingly. At last, it exposes
//! APIs to interact with the feature.
//!
//! Also see <https://bloclibrary.dev/bloc-concepts/#cubit>
//!
//! Note: Each Cubit has a suffix `Base` because currently there is no way to enforce that the
//! corresponding Dart class implements the `StateStreamableSource` inteface. Therefore we have to
//! introduce a Dart wrapper for each cubit here. The wrappers have the same name as the cubit, but
//! without the `Base` suffix.

pub mod attachments_repository;
pub mod chat_details_cubit;
pub mod chat_list_cubit;
pub mod chats_repository;
pub mod highlight;
pub mod logging;
pub mod markdown;
pub mod member_details_cubit;
pub mod message_content;
pub mod message_cubit;
pub mod message_list_cubit;
pub mod navigation_cubit;
pub mod notifications;
pub mod types;
pub mod user;
pub mod user_cubit;
pub mod user_settings_cubit;
pub mod username_suggestions;
pub mod users_cubit;
pub mod utils;
