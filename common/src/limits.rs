// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Limits on published package inventory.
//!
//! The server enforces them before it parses a package. The client asserts
//! its own pool sizes against them at compile time.

/// Key packages in one QS publish or stage request, T and APQ together.
pub const MAX_KEY_PACKAGES_PER_REQUEST: usize = 512;

/// Serialized bytes of one T key package, or of one APQ key package pair.
pub const MAX_KEY_PACKAGE_BYTES: usize = 16 * 1024;

/// Serialized bytes of all key packages in one request.
pub const MAX_KEY_PACKAGE_REQUEST_BYTES: usize = 1024 * 1024;
