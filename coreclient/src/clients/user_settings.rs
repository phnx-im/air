// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::self_group::SettingsUpdate;
use anyhow::{Context as _, bail};
use tracing::error;

use crate::{
    clients::{CoreUser, own_client_info::OwnClientInfo},
    db::{
        access::{WriteConnection, WriteDbTransaction},
        notification::DbEntityId,
    },
    job::pending_chat_operation::PendingChatOperation,
};

impl CoreUser {
    /// Loads a user setting
    ///
    /// If the setting is not found, or loading or decoding failed, `None` is returned.
    pub async fn user_setting<T: UserSetting>(&self) -> Option<T> {
        let connection = self
            .db()
            .read()
            .await
            .inspect_err(|error| {
                error!(%error, "Failed to acquire read connection while loading user settings; \
                    resetting to default");
            })
            .ok()?;

        match UserSettingRecord::load(connection, T::KEY).await {
            Ok(Some(bytes)) => match T::decode(bytes) {
                Ok(value) => Some(value),
                Err(error) => {
                    error!(%error, "Failed to decode user setting; resetting to default");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                error!(%error, "Failed to load user setting; resetting to default");
                None
            }
        }
    }

    pub async fn set_user_setting<T: UserSetting>(&self, value: &T) -> anyhow::Result<()> {
        UserSettingRecord::store(self.db().write().await?, T::KEY, T::encode(value)?).await?;
        Ok(())
    }

    /// Sets a user setting and synchronizes it across the user's linked devices
    /// through the self-group.
    ///
    /// The setting is applied locally right away (optimistic), then a self-group
    /// commit carrying the update is enqueued. If there is no self-group yet
    /// (single device that was never linked), the setting is only stored
    /// locally. If a self-group operation is already pending, the call fails and
    /// the setting is not stored.
    pub async fn set_synced_user_setting<T: SyncedUserSetting>(
        &self,
        value: &T,
    ) -> anyhow::Result<()> {
        let enqueued = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<bool> {
                let info = OwnClientInfo::load(&mut *txn).await?;

                let Some(self_group_id) = info.self_group_id else {
                    // Single device, never linked: store locally, nothing to
                    // sync to.
                    UserSettingRecord::store(&mut *txn, T::KEY, T::encode(value)?).await?;
                    return Ok(false);
                };

                // Capture the full settings state before the change so a failed
                // operation can be rolled back.
                let previous = SettingsUpdate::collect(&mut *txn).await?;

                // A settings update carries the full state of all synced
                // settings, not a diff.
                let mut update = previous.clone();
                value.apply_to_update(&mut update);

                // The new value matches the stored state. Nothing to store or
                // sync, and a no-op tap must not fail on a pending operation.
                if update == previous {
                    return Ok(false);
                }

                // Fail if a self-group operation is already pending. The tap
                // fails as a unit: do not store the setting in that case.
                if PendingChatOperation::load_by_group_id(&mut *txn, &self_group_id)
                    .await?
                    .is_some()
                {
                    bail!(
                        "a self-group operation is already pending; \
                        try changing the setting again shortly"
                    );
                }

                let signer = info
                    .self_group_signing_key
                    .context("self-group signer was not initialized")?;

                // Apply the setting locally (optimistic).
                UserSettingRecord::store(&mut *txn, T::KEY, T::encode(value)?).await?;

                PendingChatOperation::create_settings_update(
                    txn,
                    &signer,
                    &self_group_id,
                    update,
                    previous,
                )
                .await?;

                Ok(true)
            })
            .await?;

        if enqueued {
            self.outbound_service().notify_pending_chat_operations();
        }

        Ok(())
    }
}

pub trait UserSetting: Send + Sync {
    const KEY: &'static str;

    fn encode(&self) -> anyhow::Result<Vec<u8>>;
    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self>
    where
        Self: Sized;
}

/// A user setting that is synchronized across the user's devices through the
/// self-group. Adding a synced setting means adding a tag to `SettingsUpdate`
/// and implementing this trait.
pub trait SyncedUserSetting: UserSetting {
    /// Writes this setting's value into the update.
    fn apply_to_update(&self, update: &mut SettingsUpdate);
    /// Reads this setting's value from the update, if present.
    fn from_update(update: &SettingsUpdate) -> Option<Self>
    where
        Self: Sized;
}

/// Runs a per-setting function once for every synced user setting.
///
/// This is the single registry of synced settings: [`SettingsUpdate::collect`]
/// and [`roll_back_settings`] are both expanded from it, so adding a setting
/// here covers both directions.
macro_rules! for_each_synced_setting {
    ($f:ident($txn:expr $(, $args:expr)*)) => {
        $f::<ReadReceiptsSetting>(&mut *$txn $(, $args)*).await?;
    };
}

/// Constructor-style extension for [`SettingsUpdate`], which lives in the wire
/// format crate and cannot access the client database itself.
pub(crate) trait SettingsUpdateExt: Sized {
    /// Reads the current values of all synced settings into a snapshot.
    ///
    /// A settings update carries the full state of all synced settings, not a
    /// diff. Settings without a stored value are left absent. On the wire an
    /// absent field means "the sender has no value for this setting", so
    /// receivers leave the local value unchanged.
    async fn collect(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<Self>;
}

impl SettingsUpdateExt for SettingsUpdate {
    async fn collect(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<Self> {
        let mut update = SettingsUpdate::default();
        for_each_synced_setting!(collect_setting(txn, &mut update));
        Ok(update)
    }
}

async fn collect_setting<T: SyncedUserSetting>(
    txn: &mut WriteDbTransaction<'_>,
    update: &mut SettingsUpdate,
) -> anyhow::Result<()> {
    if let Some(bytes) = UserSettingRecord::load(&mut *txn, T::KEY).await? {
        T::decode(bytes)?.apply_to_update(update);
    }
    Ok(())
}

/// Rolls the touched settings in `update` back to their `previous` values.
///
/// For each setting present in `update`, the current stored value is restored
/// to the `previous` value only if it still equals the value the update tried
/// to set. If an incoming update has already overwritten it, the newer value is
/// left in place. A settings-changed notification is emitted for every setting
/// that was actually reverted, so the UI can refresh.
pub(crate) async fn roll_back_settings(
    txn: &mut WriteDbTransaction<'_>,
    update: &SettingsUpdate,
    previous: &SettingsUpdate,
) -> anyhow::Result<()> {
    for_each_synced_setting!(roll_back_setting(txn, update, previous));
    Ok(())
}

async fn roll_back_setting<T: SyncedUserSetting>(
    txn: &mut WriteDbTransaction<'_>,
    update: &SettingsUpdate,
    previous: &SettingsUpdate,
) -> anyhow::Result<()> {
    // Only act on settings this update actually touched.
    let Some(intended) = T::from_update(update) else {
        return Ok(());
    };

    let intended_bytes = intended.encode()?;
    let current_bytes = UserSettingRecord::load(&mut *txn, T::KEY).await?;

    // Only roll back if the stored value still matches what the operation tried
    // to set. An incoming update that already overwrote it must not be clobbered.
    if current_bytes.as_deref() != Some(intended_bytes.as_slice()) {
        return Ok(());
    }

    match T::from_update(previous) {
        Some(prev) => UserSettingRecord::store(&mut *txn, T::KEY, prev.encode()?).await?,
        None => UserSettingRecord::delete(&mut *txn, T::KEY).await?,
    }

    txn.notifier()
        .update(DbEntityId::UserSetting(T::KEY.to_string()));

    Ok(())
}

pub struct ReadReceiptsSetting(pub bool);

impl UserSetting for ReadReceiptsSetting {
    const KEY: &'static str = "read_receipts";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(vec![self.0 as u8])
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        match bytes.as_slice() {
            [byte] => Ok(Self(*byte != 0)),
            _ => bail!("invalid read_receipts bytes"),
        }
    }
}

impl SyncedUserSetting for ReadReceiptsSetting {
    fn apply_to_update(&self, update: &mut SettingsUpdate) {
        update.send_read_receipts = Some(self.0);
    }

    fn from_update(update: &SettingsUpdate) -> Option<Self> {
        update.send_read_receipts.map(Self)
    }
}

pub struct IsDeveloperSetting(pub bool);

impl UserSetting for IsDeveloperSetting {
    const KEY: &'static str = "is_developer";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(vec![self.0 as u8])
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        match bytes.as_slice() {
            [byte] => Ok(Self(*byte != 0)),
            _ => bail!("invalid is_developer bytes"),
        }
    }
}

pub(crate) struct UserSettingRecord {}

mod persistence {
    use crate::db::access::{ReadConnection, WriteConnection};

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

        pub(crate) async fn delete(
            mut connection: impl WriteConnection,
            setting: &str,
        ) -> sqlx::Result<()> {
            sqlx::query!("DELETE FROM user_setting WHERE setting = ?", setting)
                .execute(connection.as_mut())
                .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use crate::db::access::DbAccess;

    use super::*;

    fn read_receipts_update(value: bool) -> SettingsUpdate {
        SettingsUpdate {
            send_read_receipts: Some(value),
        }
    }

    async fn stored_read_receipts(pool: &DbAccess) -> anyhow::Result<Option<bool>> {
        let bytes = UserSettingRecord::load(pool.read().await?, ReadReceiptsSetting::KEY).await?;
        Ok(bytes.map(|b| ReadReceiptsSetting::decode(b).unwrap().0))
    }

    /// Rolls back to the previous value when the stored value still equals the
    /// value the update tried to set.
    #[sqlx::test]
    async fn roll_back_reverts_when_unchanged(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        // Previous value false, optimistically set to true.
        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![0]).await?;
        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![1]).await?;

        pool.with_write_transaction(async |txn| {
            roll_back_settings(
                txn,
                &read_receipts_update(true),
                &read_receipts_update(false),
            )
            .await
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, Some(false));
        Ok(())
    }

    /// Leaves the stored value alone when it no longer matches the update, i.e.
    /// an incoming update already overwrote it.
    #[sqlx::test]
    async fn roll_back_keeps_newer_value(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        // The current value differs from the update's intent (true).
        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![0]).await?;

        pool.with_write_transaction(async |txn| {
            roll_back_settings(
                txn,
                &read_receipts_update(true),
                &read_receipts_update(true),
            )
            .await
        })
        .await?;

        // Untouched: still the newer value.
        assert_eq!(stored_read_receipts(&pool).await?, Some(false));
        Ok(())
    }

    /// Deletes the row when the previous update carried no value for the setting.
    #[sqlx::test]
    async fn roll_back_deletes_when_previous_empty(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        // Optimistically set to true; there was no prior row.
        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![1]).await?;

        pool.with_write_transaction(async |txn| {
            roll_back_settings(txn, &read_receipts_update(true), &SettingsUpdate::default()).await
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, None);
        Ok(())
    }
}
