// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Synchronizing usernames with the user's other devices.
//!
//! The AS knows a username only by its hash and verifying key. Every device
//! therefore needs the signing key to listen to, refresh and delete a username.
//! It also needs the decryption keys of the published connection packages to
//! read incoming offers. Changes travel through the self-group as
//! [`UsernameEntry`] diffs.
//!
//! The outbox lists the changed usernames. A commit contains the state of a
//! username at staging time. Parking a username again before its commit lands
//! reuses the same row. The payload of a row is the verifying key of the ended
//! registration. The payload stays empty while the user holds the username.

use aircommon::{
    credentials::keys::UsernameVerifyingKey, crypto::ConnectionDecryptionKey, identifiers::Username,
};
use airprotos::client::self_group::{
    ConnectionPackageKey, UsernameClaimed, UsernameEntry, UsernameReleased,
};
use chrono::{DateTime, SubsecRound, Utc};
use tracing::{debug, warn};

use crate::{
    clients::{
        own_client_info::OwnClientInfo,
        self_group_outbox::{self, OutboxEntry, OutboxKind},
    },
    db::{
        access::{ReadConnection, WriteConnection, WriteDbTransaction},
        notification::DbEntityId,
    },
    usernames::{UsernameRecord, connection_packages::ConnectionPackageRecord},
};

/// A stored username with everything a claim contains.
struct StoredUsername {
    record: UsernameRecord,
    created_at: DateTime<Utc>,
    refreshed_at: DateTime<Utc>,
    packages: Vec<StoredPackageKey>,
}

struct StoredPackageKey {
    record: ConnectionPackageRecord,
    decryption_key: ConnectionDecryptionKey,
}

impl From<StoredUsername> for UsernameEntry {
    /// We truncate the timestamps to the milliseconds of the wire format. The
    /// entry then equals its own decoded echo in [`complete_sent_entries`].
    fn from(stored: StoredUsername) -> Self {
        let StoredUsername {
            record,
            created_at,
            refreshed_at,
            packages,
        } = stored;
        Self::Claimed(UsernameClaimed {
            username: record.username.plaintext().to_owned(),
            hash: Some(record.hash),
            signing_key: Some(record.signing_key),
            created_at: created_at.trunc_subsecs(3),
            refreshed_at: refreshed_at.trunc_subsecs(3),
            connection_packages: packages
                .into_iter()
                .map(ConnectionPackageKey::from)
                .collect(),
        })
    }
}

impl From<StoredPackageKey> for ConnectionPackageKey {
    fn from(key: StoredPackageKey) -> Self {
        let StoredPackageKey {
            record,
            decryption_key,
        } = key;
        Self {
            hash: Some(record.hash),
            decryption_key: Some(decryption_key),
            expires_at: record.expires_at.trunc_subsecs(3),
            is_last_resort: record.is_last_resort,
        }
    }
}

impl StoredUsername {
    /// The state an incoming claim asserts.
    ///
    /// `None` for a claim this client cannot make sense of.
    fn from_claim(claim: &UsernameClaimed) -> Option<Self> {
        let username = Username::new(claim.username.clone())
            .inspect_err(|error| warn!(%error, "Skipping a claim of an invalid username"))
            .ok()?;
        let (Some(hash), Some(signing_key)) = (claim.hash, &claim.signing_key) else {
            warn!("Skipping a username claim without its hash or signing key");
            return None;
        };
        let mut packages = Vec::with_capacity(claim.connection_packages.len());
        for key in &claim.connection_packages {
            let (Some(hash), Some(decryption_key)) = (key.hash, &key.decryption_key) else {
                warn!("Skipping a connection package key without its hash or decryption key");
                continue;
            };
            packages.push(StoredPackageKey {
                record: ConnectionPackageRecord {
                    hash,
                    expires_at: key.expires_at.into(),
                    is_last_resort: key.is_last_resort,
                },
                decryption_key: decryption_key.clone(),
            });
        }
        Some(Self {
            record: UsernameRecord::new(username, hash, signing_key.clone()),
            created_at: claim.created_at,
            refreshed_at: claim.refreshed_at,
            packages,
        })
    }

    async fn load(
        mut connection: impl ReadConnection,
        username: &Username,
    ) -> sqlx::Result<Option<Self>> {
        let Some((record, created_at, refreshed_at)) =
            UsernameRecord::load_with_timestamps(&mut connection, username).await?
        else {
            return Ok(None);
        };
        let packages = ConnectionPackageRecord::load_keys_for_username(&mut connection, username)
            .await?
            .into_iter()
            .map(|(record, decryption_key)| StoredPackageKey {
                record,
                decryption_key,
            })
            .collect();
        Ok(Some(Self {
            record,
            created_at,
            refreshed_at,
            packages,
        }))
    }
}

/// Parks a username for the next self-group commit after an add, a refresh or
/// new connection packages.
///
/// Does nothing on a device without a self group. Such a device has no
/// siblings.
///
/// Returns whether anything was parked. On `true` the caller wakes the outbound
/// service.
pub(crate) async fn park_claimed(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
) -> sqlx::Result<bool> {
    park(txn, username, None).await
}

/// Parks the end of the registration with `verifying_key`. See
/// [`park_claimed`].
pub(crate) async fn park_released(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
    verifying_key: &UsernameVerifyingKey,
) -> sqlx::Result<bool> {
    park(txn, username, Some(verifying_key)).await
}

async fn park(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
    released_verifying_key: Option<&UsernameVerifyingKey>,
) -> sqlx::Result<bool> {
    if OwnClientInfo::load_self_group_id(&mut *txn)
        .await?
        .is_none()
    {
        return Ok(false);
    }
    self_group_outbox::stage(
        &mut *txn,
        OutboxKind::Username,
        outbox_key(username),
        released_verifying_key.map_or(&[], UsernameVerifyingKey::as_slice),
        None,
    )
    .await?;
    Ok(true)
}

/// The entry a parked username resolves to right now.
///
/// `None` if there is nothing left to send. For example, a sibling released
/// the username after we parked it as claimed.
async fn current_entry(
    connection: impl ReadConnection,
    username: &Username,
    parked: &OutboxEntry,
) -> sqlx::Result<Option<UsernameEntry>> {
    if let Some(stored) = StoredUsername::load(connection, username).await? {
        return Ok(Some(UsernameEntry::from(stored)));
    }
    if parked.payload.is_empty() {
        return Ok(None);
    }
    Ok(Some(UsernameEntry::Released(UsernameReleased {
        username: username.plaintext().to_owned(),
        verifying_key: Some(UsernameVerifyingKey::from_bytes(parked.payload.clone())),
    })))
}

/// The entries for the next commit, sorted by username for a canonical
/// encoding. Drops the parked usernames with nothing left to send.
pub(crate) async fn entries_to_broadcast(
    txn: &mut WriteDbTransaction<'_>,
) -> sqlx::Result<Vec<UsernameEntry>> {
    let parked = self_group_outbox::load_kind(&mut *txn, OutboxKind::Username).await?;
    let mut entries = Vec::with_capacity(parked.len());
    for parked in parked {
        let current = match parked_username(&parked) {
            Some(username) => current_entry(&mut *txn, &username, &parked).await?,
            None => None,
        };
        match current {
            Some(entry) => entries.push(entry),
            None => self_group_outbox::remove(&mut *txn, OutboxKind::Username, &parked.key).await?,
        }
    }
    Ok(entries)
}

/// Drops the parked usernames of a sent commit. A username stays parked if it
/// changed again before the commit landed.
pub(crate) async fn complete_sent_entries(
    txn: &mut WriteDbTransaction<'_>,
    sent: &[UsernameEntry],
) -> sqlx::Result<()> {
    for entry in sent {
        let Some(username) = entry.username() else {
            continue;
        };
        let Ok(username) = Username::new(username.to_owned()) else {
            continue;
        };
        complete_if_current(txn, &username, entry).await?;
    }
    Ok(())
}

/// Drops the parked change of `username` if `entry` from a commit that just
/// landed holds its current state.
async fn complete_if_current(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
    entry: &UsernameEntry,
) -> sqlx::Result<()> {
    let key = outbox_key(username);
    let Some(parked) = self_group_outbox::load(&mut *txn, OutboxKind::Username, key).await? else {
        return Ok(());
    };
    let current = current_entry(&mut *txn, username, &parked).await?;
    if current.is_none() || current.as_ref() == Some(entry) {
        self_group_outbox::remove(&mut *txn, OutboxKind::Username, key).await?;
    }
    Ok(())
}

/// Applies the entries of a sibling's accepted usernames update.
pub(crate) async fn apply_usernames_update(
    txn: &mut WriteDbTransaction<'_>,
    entries: &[UsernameEntry],
) -> sqlx::Result<()> {
    for entry in entries {
        match entry {
            UsernameEntry::Claimed(claim) => {
                let Some(stored) = StoredUsername::from_claim(claim) else {
                    continue;
                };
                apply_claimed(txn, &stored, entry).await?;
            }
            UsernameEntry::Released(release) => {
                let Ok(username) = Username::new(release.username.clone()) else {
                    warn!("Skipping a release of an invalid username");
                    continue;
                };
                let Some(verifying_key) = &release.verifying_key else {
                    warn!("Skipping a username release without its verifying key");
                    continue;
                };
                apply_released(txn, &username, verifying_key).await?;
            }
            UsernameEntry::Unknown => debug!("Skipping a username entry with an unknown state"),
        }
    }
    Ok(())
}

/// Stores a claimed username and the package keys we miss. Drops our parked
/// change of the username.
async fn apply_claimed(
    txn: &mut WriteDbTransaction<'_>,
    stored: &StoredUsername,
    entry: &UsernameEntry,
) -> sqlx::Result<()> {
    let StoredUsername {
        record, packages, ..
    } = stored;
    if has_parked_release(txn, &record.username, record.signing_key.verifying_key()).await? {
        debug!("Ignoring a claim of a username registration we released");
        return Ok(());
    }
    if !merge_registration(txn, stored).await? {
        debug!("Ignoring a claim of an older registration of a username");
        return Ok(());
    }
    let mut stored_new_keys = false;
    for key in packages {
        stored_new_keys |= key
            .record
            .store_for_username(&mut *txn, &record.username, &key.decryption_key)
            .await?;
    }
    if stored_new_keys {
        txn.notifier()
            .update(DbEntityId::Username(record.username.clone()));
    }
    complete_if_current(txn, &record.username, entry).await
}

/// Merges the registration of a claim into the stored username.
async fn merge_registration(
    txn: &mut WriteDbTransaction<'_>,
    stored: &StoredUsername,
) -> sqlx::Result<bool> {
    let StoredUsername {
        record,
        created_at,
        refreshed_at,
        ..
    } = stored;
    let existing = UsernameRecord::load_with_timestamps(&mut *txn, &record.username).await?;
    match existing {
        Some((existing, ..))
            if existing.signing_key.verifying_key() == record.signing_key.verifying_key() =>
        {
            UsernameRecord::advance_refreshed_at(&mut *txn, &record.username, *refreshed_at)
                .await?;
        }
        Some((_, existing_created_at, _)) if *created_at < existing_created_at.trunc_subsecs(3) => {
            return Ok(false);
        }
        existing => {
            if existing.is_some() {
                UsernameRecord::delete(&mut *txn, &record.username).await?;
            }
            record
                .store_with_timestamps(&mut *txn, *created_at, *refreshed_at)
                .await?;
        }
    }
    Ok(true)
}

/// Returns `true` if a release is in the outbox.
async fn has_parked_release(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
    verifying_key: &UsernameVerifyingKey,
) -> sqlx::Result<bool> {
    let parked =
        self_group_outbox::load(&mut *txn, OutboxKind::Username, outbox_key(username)).await?;
    Ok(parked.is_some_and(|parked| parked.payload == verifying_key.as_slice()))
}

/// Deletes a released username, unless we hold it under another key.
async fn apply_released(
    txn: &mut WriteDbTransaction<'_>,
    username: &Username,
    verifying_key: &UsernameVerifyingKey,
) -> sqlx::Result<()> {
    let existing = UsernameRecord::load(&mut *txn, username).await?;
    if let Some(existing) = existing {
        if existing.signing_key.verifying_key() != verifying_key {
            debug!("Keeping a username that a sibling released under an older key");
            return Ok(());
        }
        UsernameRecord::delete(&mut *txn, username).await?;
    }
    self_group_outbox::remove(&mut *txn, OutboxKind::Username, outbox_key(username)).await
}

/// Every stored username as entries for a provisioning package.
///
/// A device joining the self group through a Welcome cannot decrypt older
/// commits. The usernames from before the join travel in the linking payload
/// instead.
pub(crate) async fn usernames_snapshot(
    mut connection: impl ReadConnection,
) -> sqlx::Result<Vec<UsernameEntry>> {
    let usernames = UsernameRecord::load_all_usernames(&mut connection).await?;
    let mut entries = Vec::with_capacity(usernames.len());
    for username in usernames {
        if let Some(stored) = StoredUsername::load(&mut connection, &username).await? {
            entries.push(UsernameEntry::from(stored));
        }
    }
    Ok(entries)
}

fn outbox_key(username: &Username) -> &[u8] {
    username.plaintext().as_bytes()
}

/// The username a parked row is for. `None` for a row this client cannot read.
fn parked_username(parked: &OutboxEntry) -> Option<Username> {
    let username = str::from_utf8(&parked.key).ok()?;
    Username::new(username.to_owned())
        .inspect_err(|error| warn!(%error, "Dropping a parked change of an invalid username"))
        .ok()
}

#[cfg(test)]
mod tests {
    use aircommon::{
        codec::PersistenceCodec, credentials::keys::UsernameSigningKey,
        identifiers::QualifiedGroupId, messages::connection_package::ConnectionPackageHash,
    };
    use airprotos::{
        auth_service::v1::SignedConnectionPackage, client::self_group::UsernamesUpdate,
    };
    use openmls::group::GroupId;
    use sqlx::SqlitePool;
    use uuid::Uuid;

    use crate::{
        db::{access::DbAccess, notification::DbOperation},
        utils::persistence::open_db_in_memory_before,
    };

    use super::*;

    /// The migration that parks the existing usernames.
    const PARK_USERNAMES_MIGRATION: i64 = 20261013140000;

    fn self_group_id() -> GroupId {
        QualifiedGroupId::new(Uuid::new_v4(), "example.com".parse().unwrap()).into()
    }

    async fn linked_device(pool: SqlitePool) -> anyhow::Result<DbAccess> {
        let db = DbAccess::for_tests(pool);
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            OwnClientInfo::store_for_test(txn, Some(self_group_id())).await?;
            Ok(())
        })
        .await?;
        Ok(db)
    }

    /// Stores a username with `packages` connection package keys and returns
    /// its record.
    async fn store_username(
        txn: &mut WriteDbTransaction<'_>,
        name: &str,
        packages: usize,
    ) -> anyhow::Result<UsernameRecord> {
        let username = Username::new(name.to_owned())?;
        let hash = username.calculate_hash()?;
        let record = UsernameRecord::new(username, hash, UsernameSigningKey::generate()?);
        record.store(&mut *txn).await?;
        for _ in 0..packages {
            let (key, _package, metadata) =
                SignedConnectionPackage::generate(record.hash, &record.signing_key, false)?;
            ConnectionPackageRecord::from(metadata)
                .store_for_username(&mut *txn, &record.username, &key)
                .await?;
        }
        Ok(record)
    }

    async fn package_hashes(
        txn: &mut WriteDbTransaction<'_>,
        username: &Username,
    ) -> anyhow::Result<Vec<ConnectionPackageHash>> {
        Ok(
            ConnectionPackageRecord::load_keys_for_username(&mut *txn, username)
                .await?
                .into_iter()
                .map(|(record, _)| record.hash)
                .collect(),
        )
    }

    async fn refreshed_at(txn: &mut WriteDbTransaction<'_>, username: &Username) -> i64 {
        let (_, _, refreshed_at) = UsernameRecord::load_with_timestamps(&mut *txn, username)
            .await
            .unwrap()
            .unwrap();
        refreshed_at.timestamp()
    }

    async fn is_parked(txn: &mut WriteDbTransaction<'_>, username: &Username) -> bool {
        self_group_outbox::load(&mut *txn, OutboxKind::Username, outbox_key(username))
            .await
            .unwrap()
            .is_some()
    }

    #[sqlx::test]
    async fn a_device_without_a_self_group_parks_nothing(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            OwnClientInfo::store_for_test(&mut *txn, None).await?;
            let record = store_username(txn, "ellie-03", 0).await?;
            assert!(!park_claimed(txn, &record.username).await?);
            assert!(
                !park_released(txn, &record.username, record.signing_key.verifying_key()).await?
            );
            assert!(entries_to_broadcast(txn).await?.is_empty());
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn the_snapshot_brings_keys_and_timestamps_to_a_new_device(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 2).await?;
            let refreshed_long_ago = Utc::now() - chrono::Duration::days(100);
            UsernameRecord::update_refreshed_at(&mut *txn, &record.hash, refreshed_long_ago)
                .await?;
            let hashes = package_hashes(txn, &record.username).await?;
            let snapshot = usernames_snapshot(&mut *txn).await?;

            // Deleting the username simulates the new device.
            UsernameRecord::delete(&mut *txn, &record.username).await?;
            assert!(package_hashes(txn, &record.username).await?.is_empty());
            apply_usernames_update(txn, &snapshot).await?;

            let (stored, _, refreshed_at) =
                UsernameRecord::load_with_timestamps(&mut *txn, &record.username)
                    .await?
                    .expect("the new device should hold the username");
            assert_eq!(stored.hash, record.hash);
            assert_eq!(
                stored.signing_key.verifying_key(),
                record.signing_key.verifying_key()
            );
            assert_eq!(
                refreshed_at,
                refreshed_long_ago.trunc_subsecs(3),
                "the new device must not think the username was just refreshed"
            );
            assert_eq!(package_hashes(txn, &record.username).await?, hashes);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn a_commit_completes_only_an_unchanged_username(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            assert!(park_claimed(txn, &record.username).await?);
            let sent = entries_to_broadcast(txn).await?;
            assert!(matches!(sent.as_slice(), [UsernameEntry::Claimed(_)]));

            // A refresh before the commit lands keeps the username parked.
            UsernameRecord::update_refreshed_at(
                &mut *txn,
                &record.hash,
                Utc::now() + chrono::Duration::days(1),
            )
            .await?;
            complete_sent_entries(txn, &sent).await?;
            assert!(is_parked(txn, &record.username).await);

            let resent = entries_to_broadcast(txn).await?;
            complete_sent_entries(txn, &resent).await?;
            assert!(!is_parked(txn, &record.username).await);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn the_decoded_echo_completes_the_username(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            park_claimed(txn, &record.username).await?;
            let sent = UsernamesUpdate {
                usernames: entries_to_broadcast(txn).await?,
            };

            let bytes = PersistenceCodec::to_vec(&sent)?;
            let echo: UsernamesUpdate = PersistenceCodec::from_slice(&bytes)?;
            complete_sent_entries(txn, &echo.usernames).await?;
            assert!(!is_parked(txn, &record.username).await);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn a_release_is_broadcast_with_its_key(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            UsernameRecord::delete(&mut *txn, &record.username).await?;
            park_released(txn, &record.username, record.signing_key.verifying_key()).await?;

            let entries = entries_to_broadcast(txn).await?;
            assert_eq!(
                entries,
                vec![UsernameEntry::Released(UsernameReleased {
                    username: "ellie-03".to_owned(),
                    verifying_key: Some(record.signing_key.verifying_key().clone()),
                })]
            );
            complete_sent_entries(txn, &entries).await?;
            assert!(!is_parked(txn, &record.username).await);
            Ok(())
        })
        .await
    }

    /// A sibling released a username we parked as claimed. We drop its row. A
    /// kept row would block the outbox.
    #[sqlx::test]
    async fn a_parked_username_that_is_gone_is_dropped(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 0).await?;
            park_claimed(txn, &record.username).await?;
            UsernameRecord::delete(&mut *txn, &record.username).await?;

            assert!(entries_to_broadcast(txn).await?.is_empty());
            assert!(!is_parked(txn, &record.username).await);
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn a_release_under_another_key_keeps_the_username(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 0).await?;
            let older_key = UsernameSigningKey::generate()?;

            let stale = UsernameEntry::Released(UsernameReleased {
                username: "ellie-03".to_owned(),
                verifying_key: Some(older_key.verifying_key().clone()),
            });
            apply_usernames_update(txn, &[stale]).await?;
            assert!(
                UsernameRecord::load(&mut *txn, &record.username)
                    .await?
                    .is_some()
            );

            let current = UsernameEntry::Released(UsernameReleased {
                username: "ellie-03".to_owned(),
                verifying_key: Some(record.signing_key.verifying_key().clone()),
            });
            apply_usernames_update(txn, &[current]).await?;
            assert!(
                UsernameRecord::load(&mut *txn, &record.username)
                    .await?
                    .is_none()
            );
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn entries_without_their_keys_are_skipped(pool: SqlitePool) -> anyhow::Result<()> {
        let db = DbAccess::for_tests(pool);
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            let snapshot = usernames_snapshot(&mut *txn).await?;
            let [UsernameEntry::Claimed(claim)] = snapshot.as_slice() else {
                anyhow::bail!("the snapshot should hold exactly one claim");
            };
            UsernameRecord::delete(&mut *txn, &record.username).await?;

            let without_signing_key = UsernameClaimed {
                signing_key: None,
                ..claim.clone()
            };
            apply_usernames_update(txn, &[UsernameEntry::Claimed(without_signing_key)]).await?;
            assert!(
                UsernameRecord::load(&mut *txn, &record.username)
                    .await?
                    .is_none()
            );

            let mut without_package_key = claim.clone();
            without_package_key.connection_packages[0].decryption_key = None;
            apply_usernames_update(txn, &[UsernameEntry::Claimed(without_package_key)]).await?;
            assert!(
                UsernameRecord::load(&mut *txn, &record.username)
                    .await?
                    .is_some()
            );
            assert!(package_hashes(txn, &record.username).await?.is_empty());

            let without_verifying_key = UsernameEntry::Released(UsernameReleased {
                username: "ellie-03".to_owned(),
                verifying_key: None,
            });
            apply_usernames_update(txn, &[without_verifying_key]).await?;
            assert!(
                UsernameRecord::load(&mut *txn, &record.username)
                    .await?
                    .is_some()
            );
            Ok(())
        })
        .await
    }

    /// A sibling's refresh saves this device its own refresh and a token. An
    /// older claim does not undo a newer refresh.
    #[sqlx::test]
    async fn a_claim_keeps_the_later_refresh_time(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 0).await?;
            let earlier = Utc::now() - chrono::Duration::days(100);
            let later = Utc::now();

            UsernameRecord::update_refreshed_at(&mut *txn, &record.hash, earlier).await?;
            let earlier_claim = usernames_snapshot(&mut *txn).await?;
            UsernameRecord::update_refreshed_at(&mut *txn, &record.hash, later).await?;
            let later_claim = usernames_snapshot(&mut *txn).await?;

            apply_usernames_update(txn, &earlier_claim).await?;
            assert_eq!(refreshed_at(txn, &record.username).await, later.timestamp());

            UsernameRecord::update_refreshed_at(&mut *txn, &record.hash, earlier).await?;
            apply_usernames_update(txn, &later_claim).await?;
            assert_eq!(refreshed_at(txn, &record.username).await, later.timestamp());
            Ok(())
        })
        .await
    }

    #[sqlx::test]
    async fn advancing_the_refresh_time_never_moves_it_back(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 0).await?;
            let earlier = Utc::now() - chrono::Duration::days(100);
            let later = Utc::now();

            UsernameRecord::update_refreshed_at(&mut *txn, &record.hash, earlier).await?;
            UsernameRecord::advance_refreshed_at(&mut *txn, &record.username, later).await?;
            assert_eq!(refreshed_at(txn, &record.username).await, later.timestamp());

            UsernameRecord::advance_refreshed_at(&mut *txn, &record.username, earlier).await?;
            assert_eq!(refreshed_at(txn, &record.username).await, later.timestamp());
            Ok(())
        })
        .await
    }

    /// An update restarts the listener of the username. Only new package keys
    /// need the restart.
    #[sqlx::test]
    async fn only_a_claim_with_new_package_keys_reports_an_update(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        let (record, claim) = db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let record = store_username(txn, "ellie-03", 1).await?;
                let claim = usernames_snapshot(&mut *txn).await?;
                Ok((record, claim))
            })
            .await?;
        let entity_id = DbEntityId::Username(record.username.clone());
        let mut notifications = db.notifier_tx.subscribe_iter();
        let mut username_ops = || {
            notifications
                .by_ref()
                .filter_map(|notification| notification.ops.get(&entity_id).copied())
                .collect::<Vec<_>>()
        };

        db.with_write_transaction(async |txn| apply_usernames_update(txn, &claim).await)
            .await?;
        assert!(username_ops().is_empty());

        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            for hash in package_hashes(txn, &record.username).await? {
                ConnectionPackageRecord::delete(&mut *txn, &hash).await?;
            }
            apply_usernames_update(txn, &claim).await?;
            Ok(())
        })
        .await?;
        assert_eq!(username_ops(), vec![DbOperation::Update]);
        Ok(())
    }

    #[sqlx::test]
    async fn a_claim_under_another_key_replaces_the_registration(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let old = store_username(txn, "ellie-03", 1).await?;
            let old_hashes = package_hashes(txn, &old.username).await?;

            // The newer registration, as a sibling holds it.
            let newer = store_username(txn, "joel-03", 2).await?;
            let mut snapshot = usernames_snapshot(&mut *txn).await?;
            snapshot.retain(|entry| entry.username() == Some("joel-03"));
            let [UsernameEntry::Claimed(claim)] = snapshot.as_mut_slice() else {
                panic!("expected one claim, got {snapshot:?}");
            };
            claim.username = "ellie-03".to_owned();
            let newer_hashes = package_hashes(txn, &newer.username).await?;
            UsernameRecord::delete(&mut *txn, &newer.username).await?;

            park_claimed(txn, &old.username).await?;
            apply_usernames_update(txn, &snapshot).await?;

            let stored = UsernameRecord::load(&mut *txn, &old.username)
                .await?
                .unwrap();
            assert_eq!(
                stored.signing_key.verifying_key(),
                newer.signing_key.verifying_key()
            );
            let hashes = package_hashes(txn, &old.username).await?;
            assert_eq!(hashes, newer_hashes);
            assert!(old_hashes.iter().all(|hash| !hashes.contains(hash)));
            assert!(
                !is_parked(txn, &old.username).await,
                "the sibling's newer word supersedes our parked change"
            );
            Ok(())
        })
        .await
    }

    /// We removed the username and registered it again under a new key. A
    /// sibling's claim of the old registration commits before ours.
    #[sqlx::test]
    async fn a_claim_of_an_older_registration_is_ignored(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let older = store_username(txn, "ellie-03", 1).await?;
            let mut stale = usernames_snapshot(&mut *txn).await?;
            let [UsernameEntry::Claimed(claim)] = stale.as_mut_slice() else {
                panic!("expected one claim, got {stale:?}");
            };
            claim.created_at = (Utc::now() - chrono::Duration::days(30)).trunc_subsecs(3);

            UsernameRecord::delete(&mut *txn, &older.username).await?;
            let newer = store_username(txn, "ellie-03", 1).await?;
            park_claimed(txn, &newer.username).await?;
            let newer_hashes = package_hashes(txn, &newer.username).await?;

            apply_usernames_update(txn, &stale).await?;

            let stored = UsernameRecord::load(&mut *txn, &newer.username)
                .await?
                .unwrap();
            assert_eq!(
                stored.signing_key.verifying_key(),
                newer.signing_key.verifying_key(),
                "the only copy of the new signing key must survive"
            );
            assert_eq!(package_hashes(txn, &newer.username).await?, newer_hashes);
            let entries = entries_to_broadcast(txn).await?;
            let [UsernameEntry::Claimed(sent)] = entries.as_slice() else {
                panic!("the new registration should stay parked, got {entries:?}");
            };
            assert_eq!(sent.signing_key.as_ref(), Some(&newer.signing_key));
            Ok(())
        })
        .await
    }

    /// We removed the username on the AS. A sibling's claim from before the
    /// removal commits before our release.
    #[sqlx::test]
    async fn a_claim_of_a_released_registration_is_ignored(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            let stale = usernames_snapshot(&mut *txn).await?;
            UsernameRecord::delete(&mut *txn, &record.username).await?;
            park_released(txn, &record.username, record.signing_key.verifying_key()).await?;

            apply_usernames_update(txn, &stale).await?;

            assert!(
                UsernameRecord::load(&mut *txn, &record.username)
                    .await?
                    .is_none(),
                "the claim must not bring the released username back"
            );
            assert_eq!(
                entries_to_broadcast(txn).await?,
                vec![UsernameEntry::Released(UsernameReleased {
                    username: "ellie-03".to_owned(),
                    verifying_key: Some(record.signing_key.verifying_key().clone()),
                })]
            );
            Ok(())
        })
        .await
    }

    /// We published a new package. A sibling's claim without its key commits
    /// before ours.
    #[sqlx::test]
    async fn a_claim_without_our_new_package_key_keeps_it_parked(
        pool: SqlitePool,
    ) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            let sibling_claim = usernames_snapshot(&mut *txn).await?;
            let (key, _package, metadata) =
                SignedConnectionPackage::generate(record.hash, &record.signing_key, false)?;
            ConnectionPackageRecord::from(metadata)
                .store_for_username(&mut *txn, &record.username, &key)
                .await?;
            park_claimed(txn, &record.username).await?;

            apply_usernames_update(txn, &sibling_claim).await?;

            let entries = entries_to_broadcast(txn).await?;
            let [UsernameEntry::Claimed(sent)] = entries.as_slice() else {
                panic!("the username should stay parked, got {entries:?}");
            };
            assert_eq!(
                sent.connection_packages.len(),
                2,
                "the sibling still needs the new key"
            );
            Ok(())
        })
        .await
    }

    /// After the upgrade every device parks the same usernames. The first
    /// commit completes them on the other devices.
    #[sqlx::test]
    async fn a_claim_of_our_parked_state_completes_it(pool: SqlitePool) -> anyhow::Result<()> {
        let db = linked_device(pool).await?;
        db.with_write_transaction(async |txn| -> anyhow::Result<()> {
            let record = store_username(txn, "ellie-03", 1).await?;
            park_claimed(txn, &record.username).await?;
            let sibling_claim = usernames_snapshot(&mut *txn).await?;

            apply_usernames_update(txn, &sibling_claim).await?;

            assert!(!is_parked(txn, &record.username).await);
            Ok(())
        })
        .await
    }

    /// Runs the migration on a database with a username. Returns the entries of
    /// the next commit.
    async fn migrate_with_a_username(
        self_group_id: Option<GroupId>,
    ) -> anyhow::Result<(UsernameRecord, Vec<UsernameEntry>)> {
        let pool = open_db_in_memory_before(PARK_USERNAMES_MIGRATION).await?;
        let db = DbAccess::for_tests(pool.clone());
        let record = db
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                OwnClientInfo::store_for_test(&mut *txn, self_group_id).await?;
                store_username(txn, "ellie-03", 1).await
            })
            .await?;

        sqlx::migrate!().run(&pool).await?;

        let entries = db
            .with_write_transaction(async |txn| entries_to_broadcast(txn).await)
            .await?;
        Ok((record, entries))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_migration_parks_the_stored_usernames() -> anyhow::Result<()> {
        let (record, entries) = migrate_with_a_username(Some(self_group_id())).await?;
        let [UsernameEntry::Claimed(claim)] = entries.as_slice() else {
            panic!("expected one claim, got {entries:?}");
        };
        assert_eq!(claim.username, record.username.plaintext());
        assert_eq!(claim.connection_packages.len(), 1);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_migration_parks_nothing_without_a_self_group() -> anyhow::Result<()> {
        let (_, entries) = migrate_with_a_username(None).await?;
        assert!(entries.is_empty());
        Ok(())
    }
}
