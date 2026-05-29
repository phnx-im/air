// SPDX-FileCopyrightText: 2026 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::crypto::signatures::keys::QsUserVerifyingKeyType;

use crate::relay_service::v1::{LinkClientRequest, LinkClientRequestPayload};

impl_signed_payload!(
    LinkClientRequest,
    LinkClientRequestPayload,
    QsUserVerifyingKeyType,
    "LinkClientRequestPayload"
);
