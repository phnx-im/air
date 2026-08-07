// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::client::self_group::SettingsUpdate;
use anyhow::bail;
use tracing::{error, warn};

use crate::{
    clients::CoreUser,
    db::{
        access::{WriteConnection, WriteDbTransaction},
        notification::DbEntityId,
    },
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
    /// The setting is applied locally right away (optimistic) and recorded in
    /// the pending [`SettingChanges`]. The outbound service turns the pending
    /// changes into a self-group commit and keeps re-issuing it until it is
    /// accepted or every touched field has been overwritten by an incoming
    /// commit. If there is no self-group yet (single device that was never
    /// linked), the setting is only stored locally.
    pub async fn set_synced_user_setting<T: SyncedUserSetting>(
        &self,
        value: &T,
    ) -> anyhow::Result<()> {
        let enqueued = self
            .db()
            .with_write_transaction(async |txn| persistence::set_synced_setting(txn, value).await)
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
    /// Removes this setting's field from the update.
    fn clear_in_update(update: &mut SettingsUpdate);
}

/// The user's not-yet-synchronized setting changes.
///
/// This is the durable intent behind settings sync, separate from the commit
/// that carries it: a `PendingChatOperation` is one send attempt, while this
/// records which settings are still ours to assert. The outbound service
/// stages a commit from the current stored settings state whenever pending
/// changes exist and no self-group operation is in flight.
///
/// A field leaves the pending changes when one of our commits carrying its
/// currently intended value is accepted ([`Self::complete_sent`]), or when a
/// sibling's accepted commit covers the field ([`Self::remove_covered`]): DS
/// commit order decides the winner and we give the field up regardless of the
/// incoming value. A terminal send failure rolls all touched fields back and
/// clears the pending changes ([`Self::roll_back_and_clear`]).
#[derive(Debug, Default, PartialEq)]
pub(crate) struct SettingChanges {
    /// The intended values of the touched settings. A present field is still
    /// ours to assert.
    changes: SettingsUpdate,
    /// The values stored before the settings were first touched, for rollback
    /// on terminal failure. A field absent here but present in `changes` had
    /// no stored value.
    previous: SettingsUpdate,
}

impl SettingChanges {
    fn is_empty(&self) -> bool {
        self.changes == SettingsUpdate::default()
    }

    /// Records a local setting change: stores the value (optimistic) and folds
    /// it into the pending changes.
    ///
    /// The first touch of a field records the value stored before it, so a
    /// terminal send failure can roll back to it. A later touch only folds the
    /// new value in. Returns whether anything is left to synchronize: a tap
    /// that re-asserts the stored value of an untouched setting is a no-op.
    pub(crate) async fn record<T: SyncedUserSetting>(
        txn: &mut WriteDbTransaction<'_>,
        value: &T,
    ) -> anyhow::Result<bool> {
        let encoded = T::encode(value)?;
        let current = UserSettingRecord::load(&mut *txn, T::KEY).await?;
        let mut pending = Self::load(&mut *txn).await?.unwrap_or_default();
        let already_touched = T::from_update(&pending.changes).is_some();

        if !already_touched && current.as_deref() == Some(encoded.as_slice()) {
            return Ok(false);
        }

        if !already_touched && let Some(bytes) = current {
            // A stored value we cannot decode is treated as unset, matching the
            // read path in `user_setting`. A rollback then deletes the row, so
            // the setting degrades to its default instead of the toggle failing
            // for good.
            match T::decode(bytes) {
                Ok(value) => value.apply_to_update(&mut pending.previous),
                Err(error) => {
                    warn!(
                        %error,
                        setting = T::KEY,
                        "Failed to decode the stored user setting, treating the previous value as unset"
                    );
                }
            }
        }
        value.apply_to_update(&mut pending.changes);

        UserSettingRecord::store(&mut *txn, T::KEY, encoded).await?;
        pending.store(&mut *txn).await?;
        Ok(true)
    }

    async fn store_or_delete(self, txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
        if self.is_empty() {
            Self::delete(&mut *txn).await?;
        } else {
            self.store(&mut *txn).await?;
        }
        Ok(())
    }

    /// Removes every field covered by a sibling's accepted snapshot from the
    /// pending changes. Coverage is value-independent: the sibling's commit is
    /// earlier in DS order, so covered fields are no longer ours to change.
    pub(crate) async fn remove_covered(
        txn: &mut WriteDbTransaction<'_>,
        incoming: &SettingsUpdate,
    ) -> anyhow::Result<()> {
        let Some(mut pending) = Self::load(&mut *txn).await? else {
            return Ok(());
        };
        remove_covered_settings(&mut pending.changes, &mut pending.previous, incoming).await?;
        pending.store_or_delete(txn).await
    }

    /// Completes the pending changes after one of our own commits was
    /// accepted. Only fields whose sent value equals the currently intended
    /// value are removed: a field the user re-toggled while the commit was in
    /// flight stays pending and is re-issued with the newer value.
    pub(crate) async fn complete_sent(
        txn: &mut WriteDbTransaction<'_>,
        sent: &SettingsUpdate,
    ) -> anyhow::Result<()> {
        let Some(mut pending) = Self::load(&mut *txn).await? else {
            return Ok(());
        };
        complete_sent_settings(&mut pending.changes, &mut pending.previous, sent).await?;
        pending.store_or_delete(txn).await
    }

    /// Rolls the touched settings back to their pre-change values and clears
    /// the pending changes. Used when a send fails terminally.
    pub(crate) async fn roll_back_and_clear(
        txn: &mut WriteDbTransaction<'_>,
    ) -> anyhow::Result<()> {
        let Some(pending) = Self::load(&mut *txn).await? else {
            return Ok(());
        };
        roll_back_settings(txn, &pending.changes, &pending.previous).await?;
        Self::delete(txn).await?;
        Ok(())
    }
}

/// Runs a per-setting function once for every synced user setting.
///
/// This is the single registry of synced settings. Every per-setting operation
/// is expanded from it, so adding a setting here covers all of them:
/// [`SettingsUpdate::collect`], [`roll_back_settings`], [`apply_settings_update`],
/// [`merge_settings_update`], [`remove_covered_settings`], and
/// [`complete_sent_settings`].
///
/// The macro expands `$f::<T>($($args),*).await?` for each setting, so every
/// per-setting helper is an `async fn` returning `anyhow::Result<()>`. The
/// merge, remove, and complete helpers need neither a transaction nor async,
/// but wearing that shape lets the single-arm macro stay the sole registry
/// rather than growing a second arm that would duplicate the settings list.
/// Call sites pass their own arguments, including a `&mut *txn` reborrow where
/// a transaction is needed.
macro_rules! for_each_synced_setting {
    ($f:ident($($args:expr),* $(,)?)) => {
        $f::<ReadReceiptsSetting>($($args),*).await?;
        $f::<crate::clients::linked_devices::LinkedDevicesSetting>($($args),*).await?;
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
        for_each_synced_setting!(collect_setting(&mut *txn, &mut update));
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
    for_each_synced_setting!(roll_back_setting(&mut *txn, update, previous));
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

/// Applies an incoming settings update to the local database.
///
/// A settings update is a snapshot, not a diff. For each synced setting an
/// absent field means the sender has no value for it, so the local value is
/// left unchanged. A present field is stored only when it differs from the
/// current value, and a settings-changed notification is emitted for each
/// setting that actually changed so the UI can refresh.
pub(crate) async fn apply_settings_update(
    txn: &mut WriteDbTransaction<'_>,
    update: &SettingsUpdate,
) -> anyhow::Result<()> {
    for_each_synced_setting!(apply_setting(&mut *txn, update));
    Ok(())
}

async fn apply_setting<T: SyncedUserSetting>(
    txn: &mut WriteDbTransaction<'_>,
    update: &SettingsUpdate,
) -> anyhow::Result<()> {
    // Absent field: the sender has no value for this setting. Leave the local
    // value unchanged.
    let Some(value) = T::from_update(update) else {
        return Ok(());
    };

    let new_bytes = value.encode()?;
    let current_bytes = UserSettingRecord::load(&mut *txn, T::KEY).await?;

    // The stored value already matches. Do nothing to avoid notification churn.
    if current_bytes.as_deref() == Some(new_bytes.as_slice()) {
        return Ok(());
    }

    UserSettingRecord::store(&mut *txn, T::KEY, new_bytes).await?;
    txn.notifier()
        .update(DbEntityId::UserSetting(T::KEY.to_string()));

    Ok(())
}

/// Folds `other` into `acc` field by field.
///
/// For each synced setting present in `other`, the value overwrites `acc`. A
/// setting absent from `other` leaves `acc` untouched. Both arguments are
/// snapshots, so folding a sequence of updates yields the last-wins union.
pub(crate) async fn merge_settings_update(
    acc: &mut SettingsUpdate,
    other: &SettingsUpdate,
) -> anyhow::Result<()> {
    for_each_synced_setting!(merge_setting(acc, other));
    Ok(())
}

async fn merge_setting<T: SyncedUserSetting>(
    acc: &mut SettingsUpdate,
    other: &SettingsUpdate,
) -> anyhow::Result<()> {
    if let Some(value) = T::from_update(other) {
        value.apply_to_update(acc);
    }
    Ok(())
}

/// Removes every field present in `incoming` from `changes` and `previous`,
/// regardless of the incoming value. See [`SettingChanges::remove_covered`].
pub(crate) async fn remove_covered_settings(
    changes: &mut SettingsUpdate,
    previous: &mut SettingsUpdate,
    incoming: &SettingsUpdate,
) -> anyhow::Result<()> {
    for_each_synced_setting!(remove_covered_setting(changes, previous, incoming));
    Ok(())
}

async fn remove_covered_setting<T: SyncedUserSetting>(
    changes: &mut SettingsUpdate,
    previous: &mut SettingsUpdate,
    incoming: &SettingsUpdate,
) -> anyhow::Result<()> {
    if T::from_update(incoming).is_some() {
        T::clear_in_update(changes);
        T::clear_in_update(previous);
    }
    Ok(())
}

/// Removes from `changes` and `previous` every field that `sent` asserts with
/// the value `changes` still intends. See [`SettingChanges::complete_sent`].
pub(crate) async fn complete_sent_settings(
    changes: &mut SettingsUpdate,
    previous: &mut SettingsUpdate,
    sent: &SettingsUpdate,
) -> anyhow::Result<()> {
    for_each_synced_setting!(complete_sent_setting(changes, previous, sent));
    Ok(())
}

async fn complete_sent_setting<T: SyncedUserSetting>(
    changes: &mut SettingsUpdate,
    previous: &mut SettingsUpdate,
    sent: &SettingsUpdate,
) -> anyhow::Result<()> {
    let (Some(sent_value), Some(intended)) = (T::from_update(sent), T::from_update(changes)) else {
        return Ok(());
    };
    if sent_value.encode()? == intended.encode()? {
        T::clear_in_update(changes);
        T::clear_in_update(previous);
    }
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

    fn clear_in_update(update: &mut SettingsUpdate) {
        update.send_read_receipts = None;
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

pub(crate) mod persistence {
    use aircommon::codec::{BlobDecoded, BlobEncoded};
    use airprotos::client::self_group::SettingsUpdate;

    use crate::{
        clients::{own_client_info::OwnClientInfo, user_settings::SyncedUserSetting},
        db::access::{ReadConnection, WriteConnection, WriteDbTransaction},
    };

    use super::{SettingChanges, UserSettingRecord};

    impl SettingChanges {
        pub(crate) async fn load(
            mut connection: impl ReadConnection,
        ) -> sqlx::Result<Option<Self>> {
            struct SqlSettingChanges {
                changes: BlobDecoded<SettingsUpdate>,
                previous: BlobDecoded<SettingsUpdate>,
            }

            let record = sqlx::query_as!(
                SqlSettingChanges,
                r#"SELECT
                    changes AS "changes: _",
                    previous AS "previous: _"
                FROM setting_changes WHERE id = 0"#
            )
            .fetch_optional(connection.as_mut())
            .await?;

            Ok(record.map(|record| Self {
                changes: record.changes.0,
                previous: record.previous.0,
            }))
        }

        pub(super) async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
            let changes = BlobEncoded(&self.changes);
            let previous = BlobEncoded(&self.previous);
            sqlx::query!(
                "INSERT INTO setting_changes (id, changes, previous) VALUES (0, ?, ?)
                ON CONFLICT (id) DO UPDATE SET
                    changes = excluded.changes,
                    previous = excluded.previous",
                changes as _,
                previous as _,
            )
            .execute(connection.as_mut())
            .await?;
            Ok(())
        }

        pub(super) async fn delete(mut connection: impl WriteConnection) -> sqlx::Result<()> {
            sqlx::query!("DELETE FROM setting_changes")
                .execute(connection.as_mut())
                .await?;
            Ok(())
        }
    }

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

    /// Records a synced setting change inside an existing transaction.
    ///
    /// Returns whether anything was enqueued for synchronization, i.e. whether the
    /// caller should notify the outbound service. A single-device client that was
    /// never linked has no self-group to sync through, so the value is only stored
    /// locally and `false` is returned.
    pub(crate) async fn set_synced_setting<T: SyncedUserSetting>(
        txn: &mut WriteDbTransaction<'_>,
        value: &T,
    ) -> anyhow::Result<bool> {
        let info = OwnClientInfo::load(&mut *txn).await?;
        if info.self_group_id.is_none() {
            // Single device, never linked: store locally, nothing to sync to.
            UserSettingRecord::store(&mut *txn, T::KEY, T::encode(value)?).await?;
            return Ok(false);
        }

        SettingChanges::record(txn, value).await
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
            linked_devices: None,
        }
    }

    async fn stored_read_receipts(pool: &DbAccess) -> anyhow::Result<Option<bool>> {
        let bytes = UserSettingRecord::load(pool.read().await?, ReadReceiptsSetting::KEY).await?;
        Ok(bytes.map(|b| ReadReceiptsSetting::decode(b).unwrap().0))
    }

    /// A notification for an update applied after subscribing but before the
    /// stream is first polled is buffered and delivered, not lost. This is the
    /// property the settings cubit relies on when it subscribes before its
    /// initial reads: a change landing during those reads reaches the listener
    /// task even though the task has not started polling yet.
    #[sqlx::test]
    async fn notification_buffered_between_subscribe_and_first_poll(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        use std::time::Duration;

        use tokio_stream::StreamExt;

        let pool = DbAccess::for_tests(pool);

        // Subscribe first, like the cubit's load path does.
        let mut notifications = std::pin::pin!(pool.notifier_tx.subscribe());

        // The update lands while nobody is polling the stream yet.
        pool.with_write_transaction(async |txn| {
            apply_settings_update(txn, &read_receipts_update(true)).await
        })
        .await?;

        // The first poll happens only now and still sees the notification.
        let notification = tokio::time::timeout(Duration::from_secs(5), notifications.next())
            .await
            .expect("notification should be buffered, not lost")
            .expect("notification stream should be open");
        assert!(
            notification.ops.keys().any(|entity_id| matches!(
                entity_id,
                DbEntityId::UserSetting(key) if key == ReadReceiptsSetting::KEY
            )),
            "expected a UserSetting notification for {}",
            ReadReceiptsSetting::KEY
        );

        Ok(())
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

    /// Applying an update with a present field stores that value.
    #[sqlx::test]
    async fn apply_stores_present_field(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| {
            apply_settings_update(txn, &read_receipts_update(true)).await
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, Some(true));
        Ok(())
    }

    /// A field absent from the update leaves the stored value untouched: an
    /// absent field means the sender has no value for it, not "clear it".
    #[sqlx::test]
    async fn apply_leaves_absent_field_untouched(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![1]).await?;

        // An update that carries no value for the setting.
        pool.with_write_transaction(async |txn| {
            apply_settings_update(txn, &SettingsUpdate::default()).await
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, Some(true));
        Ok(())
    }

    /// Applying an update whose value equals the stored value is a no-op: the
    /// stored value stays as it was.
    #[sqlx::test]
    async fn apply_equal_value_is_noop(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![1]).await?;

        pool.with_write_transaction(async |txn| {
            apply_settings_update(txn, &read_receipts_update(true)).await
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, Some(true));
        Ok(())
    }

    /// Recording a change stores the value optimistically and keeps the value
    /// stored before the first touch, so a rollback restores it and clears the
    /// pending changes.
    #[sqlx::test]
    async fn record_and_roll_back(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![0]).await?;

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?);
            Ok(())
        })
        .await?;
        assert_eq!(stored_read_receipts(&pool).await?, Some(true));

        pool.with_write_transaction(async |txn| SettingChanges::roll_back_and_clear(txn).await)
            .await?;
        assert_eq!(stored_read_receipts(&pool).await?, Some(false));
        assert!(
            SettingChanges::load(pool.read().await?).await?.is_none(),
            "rollback must clear the pending changes"
        );

        Ok(())
    }

    /// Rolling back a first touch that had no stored value deletes the row.
    #[sqlx::test]
    async fn roll_back_deletes_when_first_touch_had_no_value(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?);
            SettingChanges::roll_back_and_clear(txn).await
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, None);
        Ok(())
    }

    /// A stored value that does not decode does not block the change. It counts
    /// as no previous value, so a rollback deletes the row and the setting falls
    /// back to its default.
    #[sqlx::test]
    async fn record_treats_undecodable_previous_as_unset(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        // Two bytes never decode to a `ReadReceiptsSetting`.
        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![7, 7]).await?;

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?);

            let pending = SettingChanges::load(&mut *txn)
                .await?
                .expect("the change must be pending");
            assert_eq!(pending.changes, read_receipts_update(true));
            assert_eq!(
                pending.previous.send_read_receipts, None,
                "an undecodable previous value must be recorded as unset"
            );
            Ok(())
        })
        .await?;

        assert_eq!(stored_read_receipts(&pool).await?, Some(true));
        Ok(())
    }

    /// A tap that re-asserts the stored value of an untouched setting records
    /// nothing.
    #[sqlx::test]
    async fn record_noop_tap_records_nothing(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        UserSettingRecord::store(pool.write().await?, ReadReceiptsSetting::KEY, vec![1]).await?;

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            assert!(!SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?);
            Ok(())
        })
        .await?;

        assert!(SettingChanges::load(pool.read().await?).await?.is_none());
        Ok(())
    }

    /// A sibling's accepted snapshot removes covered fields from the pending
    /// changes regardless of the incoming value: DS order decided the winner.
    #[sqlx::test]
    async fn remove_covered_drops_field_for_any_incoming_value(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        for incoming in [true, false] {
            pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
                SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?;
                SettingChanges::remove_covered(txn, &read_receipts_update(incoming)).await?;
                assert!(
                    SettingChanges::load(&mut *txn).await?.is_none(),
                    "covered field must be dropped, incoming = {incoming}"
                );
                Ok(())
            })
            .await?;
            // Reset the stored value so the next `record` is not a no-op tap.
            UserSettingRecord::delete(pool.write().await?, ReadReceiptsSetting::KEY).await?;
        }
        Ok(())
    }

    /// An empty incoming snapshot, as an unknown-only update from a newer
    /// client decodes to, covers nothing: the pending changes survive.
    #[sqlx::test]
    async fn remove_covered_keeps_uncovered_fields(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?;
            SettingChanges::remove_covered(txn, &SettingsUpdate::default()).await?;
            let pending = SettingChanges::load(&mut *txn)
                .await?
                .expect("uncovered field must stay pending");
            assert_eq!(pending.changes, read_receipts_update(true));
            Ok(())
        })
        .await
    }

    /// Our own accepted commit completes a field it asserted with the
    /// still-intended value.
    #[sqlx::test]
    async fn complete_sent_finishes_matching_field(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?;
            SettingChanges::complete_sent(txn, &read_receipts_update(true)).await?;
            assert!(SettingChanges::load(&mut *txn).await?.is_none());
            Ok(())
        })
        .await
    }

    /// A field re-toggled while our commit was in flight stays pending: the
    /// sent value no longer matches the intended one, so the newer value must
    /// be re-issued.
    #[sqlx::test]
    async fn complete_sent_keeps_retoggled_field(pool: SqlitePool) -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(pool);

        pool.with_write_transaction(async |txn| -> anyhow::Result<()> {
            // Toggle to true; a commit carrying true goes out.
            SettingChanges::record(txn, &ReadReceiptsSetting(true)).await?;
            // Re-toggle to false while the commit is in flight.
            SettingChanges::record(txn, &ReadReceiptsSetting(false)).await?;
            // The commit carrying true is accepted.
            SettingChanges::complete_sent(txn, &read_receipts_update(true)).await?;

            let pending = SettingChanges::load(&mut *txn)
                .await?
                .expect("re-toggled field must stay pending");
            assert_eq!(pending.changes, read_receipts_update(false));
            Ok(())
        })
        .await
    }

    /// Folding a sequence of snapshots yields the last-wins union per field.
    #[tokio::test]
    async fn merge_folds_last_wins() -> anyhow::Result<()> {
        let mut acc = SettingsUpdate::default();
        merge_settings_update(&mut acc, &SettingsUpdate::default()).await?;
        assert_eq!(acc, SettingsUpdate::default(), "empty fold changes nothing");

        merge_settings_update(&mut acc, &read_receipts_update(true)).await?;
        assert_eq!(acc, read_receipts_update(true));

        // A later snapshot that covers the field overwrites it.
        merge_settings_update(&mut acc, &read_receipts_update(false)).await?;
        assert_eq!(acc, read_receipts_update(false));

        // A later empty snapshot leaves the accumulated value in place.
        merge_settings_update(&mut acc, &SettingsUpdate::default()).await?;
        assert_eq!(acc, read_receipts_update(false));
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
