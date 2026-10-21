// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Contacts: connecting via username or targeted message, connection packages,
//! contact requests, blocking and safety codes.

mod blocking;
mod connection_packages;
mod username;
pub(crate) mod requests;
mod safety_codes;
mod targeted;
