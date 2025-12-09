// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircoreclient::clients::CoreUser;

pub async fn check_invitation_code(
    server_url: String,
    invitation_code: String,
) -> anyhow::Result<bool> {
    let server_url = server_url.parse()?;
    CoreUser::check_invitation_code(server_url, invitation_code).await
}
