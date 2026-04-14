// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::Fqdn;
use aircoreclient::clients::CoreUser;

use crate::api::user_cubit::UserCubitBase;

pub async fn check_invitation_code(
    domain: String,
    invitation_code: String,
) -> anyhow::Result<bool> {
    let domain: Fqdn = domain.parse()?;
    CoreUser::check_invitation_code(domain, invitation_code).await
}

pub async fn get_invitation_code(user_cubit: &UserCubitBase) -> anyhow::Result<String> {
    user_cubit.core_user().get_invitation_code().await
}
