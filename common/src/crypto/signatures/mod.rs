// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use mls_assist::openmls::prelude::SignatureScheme;

pub const DEFAULT_SIGNATURE_SCHEME: SignatureScheme = SignatureScheme::ED25519;

pub mod keys;
pub mod private_keys;
pub mod signable;
pub mod sqlx_impls;
