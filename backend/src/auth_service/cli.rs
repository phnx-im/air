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
        user_id: Option<&UserId>,
        include_redeemed: bool,
    ) -> sqlx::Result<Vec<InvitationCodeRecord>> {
        let codes =
            InvitationCodeRecord::load_all(&self.db_pool, user_id, include_redeemed).await?;
        Ok(codes)
    }

    pub async fn invitation_codes_delete_all(&self, user_id: &UserId) -> sqlx::Result<u64> {
        let codes_deleted = InvitationCodeRecord::delete_all(&self.db_pool, user_id).await?;
        Ok(codes_deleted)
    }

    pub async fn invitation_codes_replenish(&self, user_id: &UserId) -> sqlx::Result<()> {
        InvitationCodeRecord::replenish(&self.db_pool, user_id).await?;
        Ok(())
    }
}

pub struct InvitationCodeStats {
    pub count: usize,
    pub redeemed: usize,
}
