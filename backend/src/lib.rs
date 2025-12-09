// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Implements the local and the federation part of the protocol logic on the server side

#![warn(clippy::large_futures)]

pub mod air_service;
pub mod auth_service;
pub mod ds;
pub(crate) mod errors;
pub mod messages;
pub(crate) mod pg_listen;
pub mod qs;
pub mod rate_limiter;
pub mod settings;
pub(crate) mod util;
pub(crate) mod version;

pub use mls_assist::messages::{AssistedGroupInfo, AssistedMessageOut};
