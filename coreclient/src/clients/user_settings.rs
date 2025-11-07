// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) struct UserSettingRecord {}

mod persistence {
    use sqlx::SqliteExecutor;

    use super::UserSettingRecord;

    impl UserSettingRecord {
        pub(crate) async fn load(
            executor: impl SqliteExecutor<'_>,
            setting: &'static str,
        ) -> sqlx::Result<Option<Vec<u8>>> {
            sqlx::query_scalar!("SELECT value FROM user_setting WHERE setting = ?", setting)
                .fetch_optional(executor)
                .await
        }

        pub(crate) async fn store(
            executor: impl SqliteExecutor<'_>,
            setting: &str,
            value: Vec<u8>,
        ) -> sqlx::Result<()> {
            sqlx::query!(
                "INSERT OR REPLACE INTO user_setting (setting, value) VALUES (?, ?)",
                setting,
                value
            )
            .execute(executor)
            .await?;
            Ok(())
        }
    }
}
