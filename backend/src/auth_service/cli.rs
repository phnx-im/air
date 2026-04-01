// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::UserId;

use crate::auth_service::{AuthService, invitation_code_record::InvitationCodeRecord};

impl AuthService {
    pub async fn invitation_code_stats(&self) -> sqlx::Result<InvitationCodeStats> {
        let stats = InvitationCodeRecord::stats(&self.db_pool).await?;
        Ok(stats)
    }

    pub async fn invitation_codes_list(
        &self,
        user_id: &UserId,
        limit: usize,
        include_redeemed: bool,
    ) -> sqlx::Result<impl Iterator<Item = (String, bool)>> {
        let codes =
            InvitationCodeRecord::load_all(&self.db_pool, user_id, include_redeemed, limit).await?;
        Ok(codes.into_iter().map(|code| (code.code, code.redeemed)))
    }

    pub async fn invitation_codes_insert(&self, code: &str) -> sqlx::Result<()> {
        InvitationCodeRecord::insert(&self.db_pool, code).await?;
        Ok(())
    }
}

pub struct InvitationCodeStats {
    pub count: usize,
    pub redeemed: usize,
}
