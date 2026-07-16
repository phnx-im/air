// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    credentials::keys::ClientSigningKey,
    identifiers::{QsClientId, QsUserId, UserId},
};
use openmls::group::GroupId;

mod persistence;

/// The purpose of this struct is to be stored in the local DB for use as
/// reference for other tables.
#[derive(Debug, Clone)]
pub(crate) struct OwnClientInfo {
    pub(crate) qs_user_id: QsUserId,
    pub(crate) qs_client_id: QsClientId,
    pub(crate) user_id: UserId,
    pub(crate) self_group_id: Option<GroupId>,
    pub(crate) self_group_signing_key: Option<ClientSigningKey>,
}
