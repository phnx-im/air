// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The rendezvous relay of the multi-device linking protocol.
//!
//! The relay routes opaque frames between a provisioning device and the
//! single existing device that answers its code. It parses only the
//! provisioning request and produces the session's rendezvous ID.
//!
//! State is in memory. Sessions are short lived and a lost session costs a user
//! one retry.

pub mod grpc;
pub(crate) mod sessions;

pub use sessions::{Rs, SessionId};
