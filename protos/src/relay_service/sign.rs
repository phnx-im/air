// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::crypto::signatures::keys::QsUserVerifyingKeyType;

use crate::signed::impl_signed_payload2;

impl_signed_payload2!(
    request = super::v1::LinkClientRequest,
    payload = super::v1::LinkClientRequestPayload,
    key_type = QsUserVerifyingKeyType,
    label = "LinkClientRequestPayload",
    seal = private::Seal,
);

mod private {
    #[derive(Default)]
    pub struct Seal;
}
