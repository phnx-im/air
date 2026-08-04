// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The user's linked devices: metadata synchronized across the self group.
//!
//! The self-group roster answers which devices are linked. This module carries
//! what the roster cannot: display name, platform and linked-at. The two are
//! joined on the client id of each leaf's `SelfGroupCredential`.

use aircommon::codec::PersistenceCodec;
use airprotos::client::self_group::{
    LinkedDevice, PLATFORM_ANDROID, PLATFORM_IOS, PLATFORM_LINUX, PLATFORM_MACOS, PLATFORM_UNKNOWN,
    PLATFORM_WINDOWS, SettingsUpdate,
};
use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use tracing::warn;
use uuid::Uuid;

use crate::{
    ChatId,
    clients::{
        CoreUser,
        own_client_info::OwnClientInfo,
        user_settings::{
            SyncedUserSetting, UserSetting, UserSettingRecord, persistence::set_synced_setting,
        },
    },
    db::{
        access::{ReadConnection, WriteConnection, WriteDbTransaction},
        notification::DbEntityId,
    },
    groups::self_group::SelfGroup,
    job::chat_operation::ChatOperation,
};

/// The synchronized set of the user's devices, sorted by client id.
///
/// The inner vector is private so every value is canonical: the encoding is
/// compared byte-wise by `complete_sent_setting` to decide whether an accepted
/// commit finished a pending change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedDevicesSetting(Vec<LinkedDevice>);

impl LinkedDevicesSetting {
    /// Normalizes `devices` into canonical form: sorted by client id, with
    /// duplicate ids reduced to the first occurrence.
    pub fn new(mut devices: Vec<LinkedDevice>) -> Self {
        devices.sort_by_key(|device| device.client_id);
        devices.dedup_by_key(|device| device.client_id);
        Self(devices)
    }

    pub fn devices(&self) -> &[LinkedDevice] {
        &self.0
    }

    pub fn into_devices(self) -> Vec<LinkedDevice> {
        self.0
    }
}

impl UserSetting for LinkedDevicesSetting {
    const KEY: &'static str = "linked_devices";

    fn encode(&self) -> anyhow::Result<Vec<u8>> {
        Ok(PersistenceCodec::to_vec(&self.0)?)
    }

    fn decode(bytes: Vec<u8>) -> anyhow::Result<Self> {
        let devices: Vec<LinkedDevice> = PersistenceCodec::from_slice(&bytes)?;
        Ok(Self::new(devices))
    }
}

impl SyncedUserSetting for LinkedDevicesSetting {
    fn apply_to_update(&self, update: &mut SettingsUpdate) {
        update.linked_devices = Some(self.0.clone());
    }

    fn from_update(update: &SettingsUpdate) -> Option<Self> {
        update.linked_devices.clone().map(Self::new)
    }

    fn clear_in_update(update: &mut SettingsUpdate) {
        update.linked_devices = None;
    }
}

/// The platform this build runs on, as a wire platform code.
pub fn current_platform() -> u8 {
    if cfg!(target_os = "android") {
        PLATFORM_ANDROID
    } else if cfg!(target_os = "ios") {
        PLATFORM_IOS
    } else if cfg!(target_os = "macos") {
        PLATFORM_MACOS
    } else if cfg!(target_os = "windows") {
        PLATFORM_WINDOWS
    } else if cfg!(target_os = "linux") {
        PLATFORM_LINUX
    } else {
        PLATFORM_UNKNOWN
    }
}

/// The default device name for a platform code, used when a device first
/// publishes itself. The user can rename it afterwards.
pub fn platform_label(platform: u8) -> &'static str {
    match platform {
        PLATFORM_ANDROID => "Android",
        PLATFORM_IOS => "iOS",
        PLATFORM_MACOS => "macOS",
        PLATFORM_WINDOWS => "Windows",
        PLATFORM_LINUX => "Linux",
        _ => "Device",
    }
}

/// Reads the stored device metadata, or an empty list if there is none.
pub(crate) async fn stored_devices(
    connection: impl ReadConnection,
) -> anyhow::Result<Vec<LinkedDevice>> {
    let Some(bytes) = UserSettingRecord::load(connection, LinkedDevicesSetting::KEY).await? else {
        return Ok(Vec::new());
    };
    match LinkedDevicesSetting::decode(bytes) {
        Ok(setting) => Ok(setting.into_devices()),
        Err(error) => {
            warn!(%error, "failed to decode the linked devices, treating them as empty");
            Ok(Vec::new())
        }
    }
}

/// This device's own metadata entry, defaulted from its platform.
pub(crate) async fn own_device_entry(
    txn: &mut WriteDbTransaction<'_>,
    created_at: DateTime<Utc>,
) -> anyhow::Result<LinkedDevice> {
    let client_id = OwnClientInfo::load(&mut *txn).await?.client_id;
    let platform = current_platform();
    Ok(LinkedDevice {
        client_id,
        name: platform_label(platform).to_owned(),
        created_at: created_at.timestamp().max(0) as u64,
        platform,
    })
}

/// Stores `device` in the local device list if its client id has no entry yet,
/// without enqueueing anything for synchronization.
///
/// Used on both sides of linking, where the entry reaches the other devices on
/// the self-group add commit rather than on a settings commit of its own. An
/// existing entry is left alone, so a rename is never clobbered.
pub(crate) async fn merge_device_entry_locally(
    txn: &mut WriteDbTransaction<'_>,
    device: LinkedDevice,
) -> anyhow::Result<()> {
    let mut devices = stored_devices(&mut *txn).await?;
    if devices
        .iter()
        .any(|stored| stored.client_id == device.client_id)
    {
        return Ok(());
    }
    devices.push(device);

    let setting = LinkedDevicesSetting::new(devices);
    UserSettingRecord::store(&mut *txn, LinkedDevicesSetting::KEY, setting.encode()?).await?;
    txn.notifier().update(DbEntityId::UserSetting(
        LinkedDevicesSetting::KEY.to_owned(),
    ));
    Ok(())
}

/// Publishes an entry for this device if it does not have one yet.
///
/// Returns whether the change was enqueued for synchronization.
pub(crate) async fn ensure_own_device_entry(
    txn: &mut WriteDbTransaction<'_>,
    created_at: DateTime<Utc>,
) -> anyhow::Result<bool> {
    let entry = own_device_entry(&mut *txn, created_at).await?;
    let mut devices = stored_devices(&mut *txn).await?;
    if devices
        .iter()
        .any(|device| device.client_id == entry.client_id)
    {
        return Ok(false);
    }
    devices.push(entry);

    store_linked_devices(txn, devices).await
}

/// Records that a sibling device removed this device from the self group.
///
/// Terminal and durable: the flag outlives a restart, so a device that missed
/// the moment of removal still knows it is unlinked on its next launch.
pub(crate) async fn mark_account_unlinked(txn: &mut WriteDbTransaction<'_>) -> anyhow::Result<()> {
    sqlx::query!("INSERT OR IGNORE INTO account_unlinked (id) VALUES (0)")
        .execute(txn.as_mut())
        .await?;
    Ok(())
}

/// Renames the device with the given client id.
///
/// Any device may rename any entry. The map is shared state between all clients
/// of a user.
///
/// Returns whether the change was enqueued for synchronization.
pub(crate) async fn rename_device(
    txn: &mut WriteDbTransaction<'_>,
    client_id: Uuid,
    name: &str,
) -> anyhow::Result<bool> {
    let name = name.trim();
    if name.is_empty() {
        bail!("a device name cannot be empty");
    }

    let mut devices = stored_devices(&mut *txn).await?;
    let Some(device) = devices
        .iter_mut()
        .find(|device| device.client_id == client_id)
    else {
        bail!("no linked device with client id {client_id}");
    };
    if device.name == name {
        return Ok(false);
    }
    device.name = name.to_owned();

    store_linked_devices(txn, devices).await
}

/// Writes `devices` as the synchronized set, dropping entries that are not in
/// the roster.
async fn store_linked_devices(
    txn: &mut WriteDbTransaction<'_>,
    devices: Vec<LinkedDevice>,
) -> anyhow::Result<bool> {
    let roster = match SelfGroup::load(&mut *txn).await? {
        Some(self_group) => self_group.client_ids()?,
        None => Vec::new(),
    };
    let devices = if roster.is_empty() {
        devices
    } else {
        devices
            .into_iter()
            .filter(|device| roster.contains(&device.client_id))
            .collect()
    };
    set_synced_setting(txn, &LinkedDevicesSetting::new(devices)).await
}

impl CoreUser {
    /// This client's id, as carried by its self-group leaf credential.
    pub async fn own_client_id(&self) -> anyhow::Result<Uuid> {
        let mut read = self.db().read().await?;
        Ok(OwnClientInfo::load(&mut read).await?.client_id)
    }

    /// The synchronized device metadata, sorted by client id.
    ///
    /// Metadata only. Which devices are actually linked is decided by the
    /// self-group roster, and callers join the two.
    pub async fn linked_devices(&self) -> anyhow::Result<Vec<LinkedDevice>> {
        stored_devices(self.db().read().await?).await
    }

    /// The client ids of every self-group leaf, i.e. the devices that are
    /// actually linked. Empty when there is no self group.
    pub async fn self_group_client_ids(&self) -> anyhow::Result<Vec<Uuid>> {
        let mut read = self.db().read().await?;
        let Some(self_group) = SelfGroup::load(&mut read).await? else {
            return Ok(Vec::new());
        };
        self_group.client_ids()
    }

    /// The chat id of the self group ("Notes to self"), if there is one.
    pub async fn self_chat_id(&self) -> anyhow::Result<Option<ChatId>> {
        let mut read = self.db().read().await?;
        let Some(group_id) = OwnClientInfo::load_self_group_id(&mut read).await? else {
            return Ok(None);
        };
        Ok(ChatId::try_from(&group_id).ok())
    }

    /// Unlinks a device by removing its leaf from the self group.
    ///
    /// Not revocation: the device keeps the shared user credential and its QS
    /// queue, and only tears its own data down once it processes the removal.
    pub async fn unlink_device(&self, client_id: Uuid) -> anyhow::Result<()> {
        if client_id == self.own_client_id().await? {
            bail!("a device cannot unlink itself");
        }
        if !self.self_group_client_ids().await?.contains(&client_id) {
            bail!("client {client_id} is not in the self group");
        }
        let chat_id = self
            .self_chat_id()
            .await?
            .context("no self group to unlink from")?;

        self.execute_job(ChatOperation::remove_clients(chat_id, vec![client_id]))
            .await?;
        Ok(())
    }

    /// Whether a sibling device removed this device from the self group.
    ///
    /// Terminal: this device is no longer part of the user's set of devices.
    pub async fn is_account_unlinked(&self) -> anyhow::Result<bool> {
        let mut read = self.db().read().await?;
        let row = sqlx::query_scalar!("SELECT 1 FROM account_unlinked WHERE id = 0")
            .fetch_optional(read.as_mut())
            .await?;
        Ok(row.is_some())
    }

    /// Renames a device and synchronizes the change.
    pub async fn rename_device(&self, client_id: Uuid, name: String) -> anyhow::Result<()> {
        let enqueued = self
            .db()
            .with_write_transaction(async |txn| rename_device(txn, client_id, &name).await)
            .await?;
        if enqueued {
            self.outbound_service().notify_pending_chat_operations();
        }
        Ok(())
    }

    /// Publishes this device's entry if it has none, see [`ensure_own_device_entry`].
    pub(crate) async fn ensure_own_device_entry(
        &self,
        created_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let enqueued = self
            .db()
            .with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;
        if enqueued {
            self.outbound_service().notify_pending_chat_operations();
        }
        Ok(())
    }

    /// Stores this device's entry locally and returns it, without enqueueing a
    /// synchronization.
    ///
    /// Used by a device being linked: the old device publishes the returned entry
    /// on the self-group add commit.
    pub(crate) async fn store_own_device_entry(
        &self,
        created_at: DateTime<Utc>,
    ) -> anyhow::Result<LinkedDevice> {
        self.db()
            .with_write_transaction(async |txn| {
                let entry = own_device_entry(&mut *txn, created_at).await?;
                merge_device_entry_locally(&mut *txn, entry.clone()).await?;
                Ok(entry)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::{QsClientId, QsUserId, UserId};
    use airprotos::client::self_group::{LinkedDevice, PLATFORM_IOS, PLATFORM_LINUX};
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use super::*;
    use crate::{
        clients::{own_client_info::OwnClientInfo, user_settings::UserSettingRecord},
        db::access::DbAccess,
    };

    /// An in-memory database with the `own_client_info` row the settings code
    /// needs, and no self group. Returns this device's client id.
    ///
    /// No self group is the Stage 0 shape: writes are stored locally and nothing
    /// is enqueued for synchronization.
    async fn test_db(pool: SqlitePool) -> anyhow::Result<(DbAccess, Uuid)> {
        let db = DbAccess::for_tests(pool);
        let client_id = Uuid::new_v4();
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            OwnClientInfo {
                qs_user_id: QsUserId::random(),
                qs_client_id: QsClientId::random(&mut rand::rng()),
                user_id: UserId::random("example.com".parse()?),
                client_id,
                self_group_id: None,
                self_group_signing_key: None,
            }
            .store(&mut *txn)
            .await?;
            Ok(())
        })
        .await?;
        Ok((db, client_id))
    }

    async fn devices_in(db: &DbAccess) -> anyhow::Result<Vec<LinkedDevice>> {
        stored_devices(db.read().await?).await
    }

    fn device(n: u128, name: &str, platform: u8) -> LinkedDevice {
        LinkedDevice {
            client_id: Uuid::from_u128(n),
            name: name.to_owned(),
            created_at: n as u64,
            platform,
        }
    }

    /// `new` sorts by client id, so two settings built from the same devices in
    /// different orders encode identically. `complete_sent_setting` compares
    /// encoded bytes, so this is a correctness requirement, not tidiness.
    #[test]
    fn construction_is_canonical() {
        let a = LinkedDevicesSetting::new(vec![
            device(2, "b", PLATFORM_IOS),
            device(1, "a", PLATFORM_LINUX),
        ]);
        let b = LinkedDevicesSetting::new(vec![
            device(1, "a", PLATFORM_LINUX),
            device(2, "b", PLATFORM_IOS),
        ]);
        assert_eq!(a.encode().unwrap(), b.encode().unwrap());
        assert_eq!(a.devices()[0].client_id, Uuid::from_u128(1));
    }

    #[test]
    fn duplicate_client_ids_are_dropped() {
        let setting = LinkedDevicesSetting::new(vec![
            device(1, "first", PLATFORM_LINUX),
            device(1, "second", PLATFORM_IOS),
        ]);
        assert_eq!(setting.devices().len(), 1);
        assert_eq!(setting.devices()[0].name, "first");
    }

    #[test]
    fn encode_decode_round_trip() {
        let setting = LinkedDevicesSetting::new(vec![
            device(1, "Laptop", PLATFORM_LINUX),
            device(2, "iPhone", PLATFORM_IOS),
        ]);
        let bytes = setting.encode().unwrap();
        let decoded = LinkedDevicesSetting::decode(bytes).unwrap();
        assert_eq!(decoded.devices(), setting.devices());
    }

    #[test]
    fn update_round_trip_and_clear() {
        let setting = LinkedDevicesSetting::new(vec![device(1, "Laptop", PLATFORM_LINUX)]);
        let mut update = SettingsUpdate::default();
        setting.apply_to_update(&mut update);
        assert_eq!(
            LinkedDevicesSetting::from_update(&update)
                .unwrap()
                .devices(),
            setting.devices()
        );

        LinkedDevicesSetting::clear_in_update(&mut update);
        assert!(LinkedDevicesSetting::from_update(&update).is_none());
        assert!(update.linked_devices.is_none());
    }

    /// `from_update` normalizes, so a peer that sent an unsorted list does not
    /// produce a setting whose re-encoding differs from what we would send.
    #[test]
    fn from_update_normalizes_order() {
        let update = SettingsUpdate {
            send_read_receipts: None,
            linked_devices: Some(vec![
                device(2, "b", PLATFORM_IOS),
                device(1, "a", PLATFORM_LINUX),
            ]),
        };
        let setting = LinkedDevicesSetting::from_update(&update).unwrap();
        assert_eq!(setting.devices()[0].client_id, Uuid::from_u128(1));
    }

    #[test]
    fn current_platform_is_known() {
        assert_ne!(
            current_platform(),
            airprotos::client::self_group::PLATFORM_UNKNOWN
        );
        assert!(!platform_label(current_platform()).is_empty());
    }

    /// Without a self group nothing is enqueued, so the caller must not notify.
    #[sqlx::test]
    async fn ensure_own_device_entry_publishes_this_device(pool: SqlitePool) -> anyhow::Result<()> {
        let (db, client_id) = test_db(pool).await?;

        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();
        let enqueued = db
            .with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;
        assert!(!enqueued, "no self group means nothing to synchronize");

        let stored = devices_in(&db).await?;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].client_id, client_id);
        assert_eq!(stored[0].name, platform_label(current_platform()));
        assert_eq!(stored[0].platform, current_platform());
        assert!(
            stored[0].created_at > 0,
            "linked_at must be a real timestamp"
        );

        Ok(())
    }

    /// Re-publishing on every launch would churn a self-group commit each time
    /// the app starts.
    #[sqlx::test]
    async fn ensure_own_device_entry_is_idempotent(pool: SqlitePool) -> anyhow::Result<()> {
        let (db, _client_id) = test_db(pool).await?;
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();

        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;
        let after_first = devices_in(&db).await?;
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();

        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;
        assert_eq!(devices_in(&db).await?, after_first);

        Ok(())
    }

    #[sqlx::test]
    async fn ensure_own_device_entry_preserves_a_rename(pool: SqlitePool) -> anyhow::Result<()> {
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();
        let (db, client_id) = test_db(pool).await?;
        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;

        db.with_write_transaction(async |txn| rename_device(txn, client_id, "Work laptop").await)
            .await?;
        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;

        let stored = devices_in(&db).await?;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "Work laptop");

        Ok(())
    }

    #[sqlx::test]
    async fn rename_trims_and_stores(pool: SqlitePool) -> anyhow::Result<()> {
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();
        let (db, client_id) = test_db(pool).await?;
        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;

        db.with_write_transaction(async |txn| rename_device(txn, client_id, "  Desk  ").await)
            .await?;

        assert_eq!(devices_in(&db).await?[0].name, "Desk");
        Ok(())
    }

    /// A redundant tap must not cost a commit.
    #[sqlx::test]
    async fn rename_to_the_same_name_is_a_no_op(pool: SqlitePool) -> anyhow::Result<()> {
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();
        let (db, client_id) = test_db(pool).await?;
        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;
        let before = devices_in(&db).await?;
        let name = before[0].name.clone();

        let enqueued = db
            .with_write_transaction(async |txn| rename_device(txn, client_id, &name).await)
            .await?;

        assert!(!enqueued);
        assert_eq!(devices_in(&db).await?, before);
        Ok(())
    }

    #[sqlx::test]
    async fn rename_unknown_device_is_an_error(pool: SqlitePool) -> anyhow::Result<()> {
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();
        let (db, _client_id) = test_db(pool).await?;
        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;

        let error = db
            .with_write_transaction(async |txn| {
                rename_device(txn, Uuid::from_u128(0xdead), "Nope").await
            })
            .await
            .expect_err("renaming an unknown client id must fail");
        assert!(
            error.to_string().contains("no linked device"),
            "unexpected error: {error}"
        );

        Ok(())
    }

    #[sqlx::test]
    async fn rename_rejects_an_empty_name(pool: SqlitePool) -> anyhow::Result<()> {
        let created_at = DateTime::parse_from_rfc3339("2026-08-03T20:41:52.000Z")
            .unwrap()
            .to_utc();
        let (db, client_id) = test_db(pool).await?;
        db.with_write_transaction(async |txn| ensure_own_device_entry(txn, created_at).await)
            .await?;

        let error = db
            .with_write_transaction(async |txn| rename_device(txn, client_id, "   ").await)
            .await
            .expect_err("an all-whitespace name must be rejected");
        assert!(
            error.to_string().contains("empty"),
            "unexpected error: {error}"
        );

        Ok(())
    }

    /// Matches the read path in `CoreUser::user_setting`. The alternative is a
    /// device list that fails permanently on one bad row.
    #[sqlx::test]
    async fn undecodable_stored_value_reads_as_empty(pool: SqlitePool) -> anyhow::Result<()> {
        let (db, _client_id) = test_db(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            UserSettingRecord::store(&mut *txn, LinkedDevicesSetting::KEY, vec![0xff, 0xff])
                .await?;
            Ok(())
        })
        .await?;

        assert!(devices_in(&db).await?.is_empty());
        Ok(())
    }
}
