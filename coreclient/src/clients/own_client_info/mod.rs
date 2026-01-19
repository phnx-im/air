// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::{QsClientId, QsUserId, UserId};

mod persistence;

/// The purpose of this struct is to be stored in the local DB for use as
/// reference for other tables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OwnClientInfo {
    pub(crate) qs_user_id: QsUserId,
    pub(crate) qs_client_id: QsClientId,
    pub(crate) user_id: UserId,
}
