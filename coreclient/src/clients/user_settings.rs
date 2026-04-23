// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::bail;

use crate::store::{StoreResult, UserSetting};

pub struct ReadReceiptsSetting(pub bool);

impl UserSetting for ReadReceiptsSetting {
    const KEY: &'static str = "read_receipts";

    fn encode(&self) -> StoreResult<Vec<u8>> {
        Ok(vec![self.0 as u8])
    }

    fn decode(bytes: Vec<u8>) -> StoreResult<Self> {
        match bytes.as_slice() {
            [byte] => Ok(Self(*byte != 0)),
            _ => bail!("invalid read_receipts bytes"),
        }
    }
}

pub(crate) struct UserSettingRecord {}

mod persistence {
    use sqlx::SqliteExecutor;

    use crate::db_access::{ReadConnection, WriteConnection};

    use super::UserSettingRecord;

    impl UserSettingRecord {
        pub(crate) async fn load(
            mut connection: impl ReadConnection,
            setting: &'static str,
        ) -> sqlx::Result<Option<Vec<u8>>> {
            sqlx::query_scalar!("SELECT value FROM user_setting WHERE setting = ?", setting)
                .fetch_optional(connection.as_mut())
                .await
        }

        pub(crate) async fn store(
            mut connection: impl WriteConnection,
            setting: &str,
            value: Vec<u8>,
        ) -> sqlx::Result<()> {
            sqlx::query!(
                "INSERT OR REPLACE INTO user_setting (setting, value) VALUES (?, ?)",
                setting,
                value
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }
    }
}
