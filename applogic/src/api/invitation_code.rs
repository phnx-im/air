// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::Fqdn;
use aircoreclient::clients::{CoreUser, InvitationCode};

use crate::api::types::UiUserId;

pub async fn check_invitation_code(
    domain: String,
    invitation_code: String,
) -> anyhow::Result<bool> {
    let domain: Fqdn = domain.parse()?;
    CoreUser::check_invitation_code(domain, invitation_code).await
}

pub async fn replenish_invitation_codes(user_id: UiUserId) -> anyhow::Result<Vec<InvitationCode>> {
    CoreUser::replenish_invitation_codes(user_id.into()).await
}
