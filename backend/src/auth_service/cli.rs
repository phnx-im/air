// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{identifiers::USERNAME_VALIDITY_PERIOD, time::ExpirationData};
use chrono::{DateTime, Utc};

use crate::auth_service::{
    AuthService, invitation_code_record::InvitationCodeRecord, usernames::UsernameRecord,
};

impl AuthService {
    pub async fn invitation_code_stats(&self) -> sqlx::Result<InvitationCodeStats> {
        let stats = InvitationCodeRecord::stats(&self.db_pool).await?;
        Ok(stats)
    }

    pub async fn invitation_codes_list(
        &self,
        limit: usize,
        include_redeemed: bool,
    ) -> sqlx::Result<impl Iterator<Item = (String, bool)>> {
        let codes = InvitationCodeRecord::load_all(&self.db_pool, include_redeemed, limit).await?;
        Ok(codes.into_iter().map(|code| (code.code, code.redeemed)))
    }

    pub async fn invitation_codes_generate(&self, n: usize) -> sqlx::Result<()> {
        let mut connection = self.db_pool().acquire().await?;
        for _ in 0..n {
            let code = InvitationCodeRecord::generate(&mut connection).await?;
            println!("{code}");
        }
        Ok(())
    }

    pub async fn usernames_list(
        &self,
    ) -> sqlx::Result<impl Iterator<Item = ([u8; 32], ExpirationData)>> {
        let mut connection = self.db_pool().acquire().await?;
        let mut records = UsernameRecord::load_all(connection.as_mut()).await?;
        records.sort_by_key(|record| *record.expiration_data.not_after().as_ref());
        Ok(records
            .into_iter()
            .map(|record| (record.username_hash.into_bytes(), record.expiration_data)))
    }

    pub async fn username_refresh_expiring(&self, before: DateTime<Utc>) -> sqlx::Result<usize> {
        let mut txn = self
            .db_pool()
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await?;
        let records = UsernameRecord::load_all(txn.as_mut()).await?;
        let mut updated = 0;
        for record in records {
            if record.expiration_data.not_after().as_ref() < &before {
                UsernameRecord::update_expiration_data(
                    txn.as_mut(),
                    &record.username_hash,
                    ExpirationData::new(USERNAME_VALIDITY_PERIOD),
                )
                .await?;
                updated += 1;
            }
        }
        txn.commit().await?;
        Ok(updated)
    }
}

pub struct InvitationCodeStats {
    pub count: usize,
    pub redeemed: usize,
}
