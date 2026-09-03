// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    collections::HashMap,
    fmt::Display,
    fs,
    future::ready,
    io,
    path::{Path, PathBuf},
};

use anyhow::bail;
use openmls::group::GroupId;
use sqlx::{
    Connection, Database, Encode, Sqlite, SqlitePool, Type,
    encode::IsNull,
    error::BoxDynError,
    migrate::{Migrate, MigrateError},
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use strum::VariantArray;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    chats::messages::edit::purge_stale_deleted_messages,
    clients::{own_client_info::OwnClientInfo, store::ClientRecord},
    db::{
        access::{DbAccess, WriteConnection},
        notification::DbNotificationsSender,
    },
    utils::global_lock::GlobalLock,
};

pub(crate) const AIR_DB_NAME: &str = "air.db";

/// Open a connection to the DB that contains records for all clients on this
/// device.
pub(crate) async fn open_air_db(db_path: &str) -> sqlx::Result<DbAccess> {
    let db_url = format!("sqlite://{db_path}/{AIR_DB_NAME}");
    let opts: SqliteConnectOptions = db_url.parse()?;

    let write_pool = write_pool(opts.clone()).await?;

    // Delete the old migration table if it exists
    const FIRST_MIGRATION: i64 = 20250115104336;
    if let Ok(Some(_)) =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM _sqlx_migrations WHERE version = ?")
            .bind(FIRST_MIGRATION)
            .fetch_optional(&write_pool)
            .await
    {
        // The database is based on old migration
        sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
            .execute(&write_pool)
            .await?;
    }

    sqlx::migrate!("migrations/air").run(&write_pool).await?;
    let read_pool = read_pool(opts).await?;

    let air_db = DbAccess::with_split_pools(write_pool, read_pool, DbNotificationsSender::new());

    if let Err(error) = rename_legacy_client_dbs(db_path, &air_db).await {
        error!(%error, "Failed to rename legacy client DBs");
    }

    Ok(air_db)
}

/// Renames legacy client DB files (named after the user id) to the random DB UUID name from the
/// client record.
///
/// Legacy client DBs were created before the client record carried a DB UUID. The migration assigns
/// a random UUID to each record; the file is renamed here. Idempotent: does nothing when no legacy
/// file exists.
async fn rename_legacy_client_dbs(db_path: &str, air_db: &DbAccess) -> sqlx::Result<()> {
    for record in ClientRecord::load_all(air_db.read().await?).await? {
        let legacy_name = format!("{}@{}.db", record.user_id.uuid(), record.user_id.domain());
        let name = client_db_name(record.client_record_id);
        if !Path::new(db_path).join(&legacy_name).exists()
            || Path::new(db_path).join(&name).exists()
        {
            continue;
        }
        info!(from = legacy_name, to = name, "renaming legacy client DB");
        // Also move over the SQLite journal siblings, which exist when the DB
        // was not cleanly closed.
        for suffix in ["", "-wal", "-shm"] {
            let from = Path::new(db_path).join(format!("{legacy_name}{suffix}"));
            if !from.exists() {
                continue;
            }
            let to = Path::new(db_path).join(format!("{name}{suffix}"));
            if let Err(error) = fs::rename(&from, &to) {
                error!(%error, ?from, "Failed to rename legacy client DB file");
            }
        }
    }
    Ok(())
}

#[cfg(feature = "test_utils")]
pub(crate) async fn open_db_in_memory() -> sqlx::Result<SqlitePool> {
    use std::time::Duration;

    let opts = SqliteConnectOptions::new()
        .journal_mode(SqliteJournalMode::Wal)
        .in_memory(true);
    let pool = SqlitePoolOptions::new()
        // More than one connection in memory is not supported.
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        // We have only a single connection, so fail fast when there is a deadlock when acquiring a
        // connection.
        .acquire_timeout(Duration::from_secs(3))
        .connect_with(opts)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

async fn write_pool(opts: SqliteConnectOptions) -> sqlx::Result<SqlitePool> {
    let write_opts = opts
        .clone()
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);

    // we create a pool with a single connection that we use for writes (and reads inside of a write transaction)
    // to ensure that we have fair queueing of write operations (since it is implemented using a tokio::sync::Semaphore)
    SqlitePoolOptions::new()
        .idle_timeout(None)
        .max_lifetime(None)
        .min_connections(1)
        .max_connections(1)
        .after_release(|conn, _meta| {
            // Discard connections that are left in an open transaction.
            //
            // This can happen when a future holding a transaction is cancelled, causing the sqlx
            // worker to crash internally (it tries to send an error back via a rendezvous channel but
            // the receiver is gone). Discarding such connections prevents permanently-stuck
            // `transaction_depth > 0` errors on subsequent use.
            let return_to_pool = !conn.is_in_transaction();
            Box::pin(ready(Ok(return_to_pool)))
        })
        .connect_with(write_opts)
        .await
}

async fn read_pool(opts: SqliteConnectOptions) -> sqlx::Result<SqlitePool> {
    let read_opts = opts.read_only(true);
    SqlitePoolOptions::new()
        .idle_timeout(None)
        .max_lifetime(None)
        .after_release(|conn, _meta| {
            // Discard connections that are left in an open transaction.
            //
            // This can happen when a future holding a transaction is cancelled, causing the sqlx
            // worker to crash internally (it tries to send an error back via a rendezvous channel but
            // the receiver is gone). Discarding such connections prevents permanently-stuck
            // `transaction_depth > 0` errors on subsequent use.
            let return_to_pool = !conn.is_in_transaction();
            Box::pin(ready(Ok(return_to_pool)))
        })
        .connect_with(read_opts)
        .await
}

/// Delete both the air.db and all client dbs from this device.
///
/// If the air.db exists, but cannot be opened, only the air.db is deleted.
///
/// WARNING: This will delete all APP-data from this device!
pub async fn delete_databases(client_db_path: &str) -> anyhow::Result<()> {
    let full_air_db_path = format!("{client_db_path}/{AIR_DB_NAME}");
    if !Path::new(&full_air_db_path).exists() {
        bail!("{full_air_db_path} does not exist")
    }

    // First try to delete all client DBs
    if let Err(error) = delete_client_databases(client_db_path).await {
        error!(%error, "Failed to delete client DBs")
    }

    // Finally, delete the air.db
    info!(path =% full_air_db_path, "removing AIR DB");
    fs::remove_file(full_air_db_path)?;

    Ok(())
}

async fn delete_client_databases(client_db_path: &str) -> anyhow::Result<()> {
    let air_db_connection = open_air_db(client_db_path).await?;
    if let Ok(client_records) = ClientRecord::load_all(air_db_connection.read().await?).await {
        for client_record in client_records {
            let client_db_name = client_db_name(client_record.client_record_id);
            let client_db_path = format!("{client_db_path}/{client_db_name}");
            info!(path =% client_db_path, "removing client DB");
            if let Err(error) = fs::remove_file(&client_db_path) {
                error!(%error, %client_db_path, "Failed to delete client DB")
            }
        }
    }
    Ok(())
}

pub async fn delete_client_database(db_path: &str, client_record_id: Uuid) -> anyhow::Result<()> {
    let full_air_db_path: String = format!("{db_path}/{AIR_DB_NAME}");
    if !Path::new(&full_air_db_path).exists() {
        bail!("air.db does not exist")
    }

    // Delete the client DB
    let client_db_name = client_db_name(client_record_id);
    let client_db_path = Path::new(db_path).join(client_db_name);
    let mut client_db_removal_error = None;
    for path in [
        client_db_path.clone(),
        client_db_path.with_extension("db-shm"),
        client_db_path.with_extension("db-wal"),
    ] {
        info!(path = %path.display(), "removing client DB file");
        if let Err(error) = remove_file_if_exists(&path) {
            error!(%error, path = %path.display(), "failed to delete client DB file");
            client_db_removal_error.get_or_insert_with(|| anyhow::Error::new(error));
        }
    }

    // Delete the client record from the air DB
    let air_db = open_air_db(db_path).await?;
    ClientRecord::delete(air_db.write().await?, client_record_id).await?;

    client_db_removal_error.map_or(Ok(()), Err)
}

fn remove_file_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// The name of a client DB file
fn client_db_name(client_record_id: Uuid) -> String {
    format!("{client_record_id}.db")
}

pub async fn open_client_db(
    client_db_path: &str,
    client_record_id: Uuid,
) -> anyhow::Result<DbAccess> {
    let client_db_name = client_db_name(client_record_id);
    let db_url = format!("sqlite://{client_db_path}/{client_db_name}");
    info!(db_url, "opening client DB");
    let opts: SqliteConnectOptions = db_url.parse()?;

    let write_pool = write_pool(opts.clone()).await?;
    let read_pool = read_pool(opts).await?;
    let db = DbAccess::with_split_pools(write_pool, read_pool, DbNotificationsSender::new());

    run_client_migrations(&db).await?;

    Ok(db)
}

#[derive(Debug, Clone, Copy, strum::VariantArray)]
#[repr(i64)]
/// To add a new migration, add your variant here with the matching
/// sqlx migration version as value, then follow the compiler.
enum RustMigration {
    OwnClientIdBackfill = 20260817150000,
    StaleDeletedMessagesPurge = 20260817150100,
}

impl RustMigration {
    fn from_version(version: i64) -> Option<Self> {
        match version {
            20260817150000 => Some(Self::OwnClientIdBackfill),
            20260817150100 => Some(Self::StaleDeletedMessagesPurge),
            _ => None,
        }
    }

    /// Applies the code migration to the database.
    async fn apply(&self, write: impl WriteConnection) -> anyhow::Result<()> {
        match self {
            RustMigration::OwnClientIdBackfill => OwnClientInfo::backfill_client_id(write).await?,
            RustMigration::StaleDeletedMessagesPurge => purge_stale_deleted_messages(write).await?,
        }
        Ok(())
    }
}

/// Runs the client DB's SQL migrations, interleaved with one-time code
/// migrations that must not rerun on every open.
///
/// Each SQL migration can have a corresponding optional code migration. After it is applied, the
/// code migration is applied in the *same* transaction.
///
/// Note: This function is implemented along the lines of `sqlx::migrate::Migrator::run`, but
/// adjusted to sqlite.
async fn run_client_migrations(db: &DbAccess) -> anyhow::Result<()> {
    let migrator = sqlx::migrate!();
    let table = "_sqlx_migrations";

    // Check that every rust migration has a matching migration file.
    for m in RustMigration::VARIANTS {
        anyhow::ensure!(
            migrator.version_exists(*m as i64),
            "no migration file for paired code migration version {m:?}",
        );
    }

    let mut write = db.write().await?;
    write.as_mut().ensure_migrations_table(table).await?;
    if let Some(version) = write.as_mut().dirty_version(table).await? {
        return Err(MigrateError::Dirty(version).into());
    }
    let applied = write.as_mut().list_applied_migrations(table).await?;
    for m in &applied {
        anyhow::ensure!(
            migrator.version_exists(m.version),
            "unknown applied version {}",
            m.version,
        );
    }
    let applied: HashMap<i64, _> = applied.into_iter().map(|m| (m.version, m)).collect();

    for migration in migrator.iter() {
        if migration.migration_type.is_down_migration() {
            continue;
        }
        match applied.get(&migration.version) {
            Some(applied) if migration.checksum != applied.checksum => {
                return Err(MigrateError::VersionMismatch(migration.version).into());
            }
            Some(_) => {} // already applied
            None => match RustMigration::from_version(migration.version) {
                Some(code) => {
                    let mut txn = write.begin().await?;
                    txn.as_mut().apply(table, migration).await?;
                    code.apply(&mut txn).await?;
                    txn.commit().await?;
                }
                None => {
                    write.as_mut().apply(table, migration).await?;
                }
            },
        }
    }

    Ok(())
}

pub(crate) fn open_lock_file(db_path: &str) -> std::io::Result<GlobalLock> {
    GlobalLock::new(PathBuf::from(db_path).join("lockfile"))
}

/// Helper struct that allows us to use GroupId as sqlite input.
pub(crate) struct GroupIdRefWrapper<'a>(&'a GroupId);

impl<'a> From<&'a GroupId> for GroupIdRefWrapper<'a> {
    fn from(group_id: &'a GroupId) -> Self {
        Self(group_id)
    }
}

impl Display for GroupIdRefWrapper<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from_utf8_lossy(self.0.as_slice()))
    }
}

impl Type<Sqlite> for GroupIdRefWrapper<'_> {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <Vec<u8> as Type<Sqlite>>::type_info()
    }
}

impl<'q> Encode<'q, Sqlite> for GroupIdRefWrapper<'q> {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode_by_ref(&self.0.as_slice(), buf)
    }
}

pub(crate) struct GroupIdWrapper(pub(crate) GroupId);

impl From<GroupIdWrapper> for GroupId {
    fn from(group_id: GroupIdWrapper) -> Self {
        group_id.0
    }
}

#[cfg(test)]
mod tests {
    use aircommon::identifiers::{QsClientId, QsUserId, UserId};
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    use super::*;
    use crate::clients::store::{ClientRecord, ClientRecordState};

    #[test]
    fn from_version_covers_all_variants() {
        for migration in RustMigration::VARIANTS {
            let version = *migration as i64;
            assert_eq!(
                RustMigration::from_version(version).map(|migration| migration as i64),
                Some(version),
                "from_version disagrees with the discriminant of {migration:?}",
            );
        }
    }

    async fn store_record(
        air_db: &DbAccess,
        user_id: &UserId,
        client_record_id: Uuid,
    ) -> anyhow::Result<()> {
        ClientRecord {
            client_record_id,
            user_id: user_id.clone(),
            client_record_state: ClientRecordState::Finished,
            created_at: Utc::now(),
            is_default: false,
        }
        .store(air_db.write().await?)
        .await?;
        Ok(())
    }

    fn legacy_db_name(user_id: &UserId) -> String {
        format!("{}@{}.db", user_id.uuid(), user_id.domain())
    }

    /// A legacy client DB file and its SQLite journal siblings are renamed to
    /// the DB UUID name from the client record. A second run is a no-op.
    #[tokio::test]
    async fn renames_legacy_client_db() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let air_db = open_air_db(db_path).await?;

        let user_id = UserId::random("localhost".parse()?);
        let client_record_id = Uuid::new_v4();
        store_record(&air_db, &user_id, client_record_id).await?;

        let legacy_name = legacy_db_name(&user_id);
        for suffix in ["", "-wal", "-shm"] {
            fs::write(
                dir.path().join(format!("{legacy_name}{suffix}")),
                format!("legacy{suffix}"),
            )?;
        }

        for _ in 0..2 {
            rename_legacy_client_dbs(db_path, &air_db).await?;

            for suffix in ["", "-wal", "-shm"] {
                assert!(!dir.path().join(format!("{legacy_name}{suffix}")).exists());
                let renamed = dir.path().join(format!("{client_record_id}.db{suffix}"));
                assert_eq!(fs::read_to_string(renamed)?, format!("legacy{suffix}"));
            }
        }

        Ok(())
    }

    /// A missing journal sibling of a legacy client DB is not an error.
    #[tokio::test]
    async fn renames_legacy_client_db_without_journal_siblings() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let air_db = open_air_db(db_path).await?;

        let user_id = UserId::random("localhost".parse()?);
        let client_record_id = Uuid::new_v4();
        store_record(&air_db, &user_id, client_record_id).await?;

        let legacy_name = legacy_db_name(&user_id);
        fs::write(dir.path().join(&legacy_name), "legacy")?;

        rename_legacy_client_dbs(db_path, &air_db).await?;

        assert!(!dir.path().join(&legacy_name).exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(format!("{client_record_id}.db")))?,
            "legacy"
        );
        for suffix in ["-wal", "-shm"] {
            assert!(
                !dir.path()
                    .join(format!("{client_record_id}.db{suffix}"))
                    .exists()
            );
        }

        Ok(())
    }

    /// A leftover legacy file must not clobber an existing client DB.
    #[tokio::test]
    async fn keeps_existing_client_db() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let air_db = open_air_db(db_path).await?;

        let user_id = UserId::random("localhost".parse()?);
        let client_record_id = Uuid::new_v4();
        store_record(&air_db, &user_id, client_record_id).await?;

        let legacy_name = legacy_db_name(&user_id);
        fs::write(dir.path().join(&legacy_name), "legacy")?;
        fs::write(dir.path().join(format!("{client_record_id}.db")), "current")?;

        rename_legacy_client_dbs(db_path, &air_db).await?;

        assert_eq!(fs::read_to_string(dir.path().join(&legacy_name))?, "legacy");
        assert_eq!(
            fs::read_to_string(dir.path().join(format!("{client_record_id}.db")))?,
            "current"
        );

        Ok(())
    }

    /// A record whose client DB was never created (nothing to rename) is
    /// skipped.
    #[tokio::test]
    async fn skips_record_without_legacy_db() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let air_db = open_air_db(db_path).await?;

        let user_id = UserId::random("localhost".parse()?);
        let client_record_id = Uuid::new_v4();
        store_record(&air_db, &user_id, client_record_id).await?;

        rename_legacy_client_dbs(db_path, &air_db).await?;

        assert!(!dir.path().join(format!("{client_record_id}.db")).exists());

        Ok(())
    }

    /// The rename pass runs when the air DB is opened.
    #[tokio::test]
    async fn renames_legacy_client_db_on_open() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let air_db = open_air_db(db_path).await?;

        let user_id = UserId::random("localhost".parse()?);
        let client_record_id = Uuid::new_v4();
        store_record(&air_db, &user_id, client_record_id).await?;

        let legacy_name = legacy_db_name(&user_id);
        fs::write(dir.path().join(&legacy_name), "legacy")?;
        drop(air_db);

        open_air_db(db_path).await?;

        assert!(!dir.path().join(&legacy_name).exists());
        assert_eq!(
            fs::read_to_string(dir.path().join(format!("{client_record_id}.db")))?,
            "legacy"
        );

        Ok(())
    }

    #[tokio::test]
    async fn closed_client_database_is_deleted_with_its_sidecars() -> anyhow::Result<()> {
        let directory = tempdir()?;
        let db_path = directory.path().to_str().unwrap();
        let user_id = UserId::new(Uuid::new_v4(), "example.com".parse()?);
        let client_record_id = Uuid::new_v4();

        let air_db = open_air_db(db_path).await?;
        ClientRecord {
            user_id: user_id.clone(),
            client_record_state: ClientRecordState::Finished,
            created_at: Utc::now(),
            is_default: true,
            client_record_id,
        }
        .store(air_db.write().await?)
        .await?;

        let client_db = open_client_db(db_path, client_record_id).await?;
        let other_handle = client_db.clone();
        client_db.close().await;
        assert!(other_handle.read().await.is_err());

        let client_db_path = directory.path().join(client_db_name(client_record_id));
        let shm_path = client_db_path.with_extension("db-shm");
        let wal_path = client_db_path.with_extension("db-wal");
        fs::write(&shm_path, b"stale shm")?;
        fs::write(&wal_path, b"stale wal")?;

        delete_client_database(db_path, client_record_id).await?;

        assert!(!client_db_path.exists());
        assert!(!shm_path.exists());
        assert!(!wal_path.exists());
        assert!(
            ClientRecord::load(air_db.read().await?, client_record_id)
                .await?
                .is_none()
        );

        Ok(())
    }

    #[tokio::test]
    async fn own_client_id_backfill_runs_once() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let client_record_id = Uuid::new_v4();
        let user_id = UserId::random("localhost".parse()?);
        let mut rng = rand::rng();

        // Fully migrate a fresh DB; this consumes the marker migration since there is nothing to
        // backfill yet.
        let db = open_client_db(db_path, client_record_id).await?;
        db.close().await;

        // Un-record the marker and insert an `own_client_info` row the way an old,
        // pre-backfill client would have left it on disk: with a nil client id. Only the marker
        // is un-recorded, so that later migrations are not replayed against a schema they have
        // already been applied to.
        let db = open_client_db(db_path, client_record_id).await?;
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
            .bind(RustMigration::OwnClientIdBackfill as i64)
            .execute(db.write().await?.as_mut())
            .await?;
        sqlx::query(
            "INSERT INTO own_client_info (qs_user_id, qs_client_id, user_uuid, user_domain)
            VALUES (?, ?, ?, ?)",
        )
        .bind(QsUserId::random())
        .bind(QsClientId::random(&mut rng))
        .bind(user_id.uuid())
        .bind(user_id.domain().to_string())
        .execute(db.write().await?.as_mut())
        .await?;
        db.close().await;

        // Reopening reruns the marker's code, backfilling the nil client id.
        let db = open_client_db(db_path, client_record_id).await?;
        let client_id: Uuid = sqlx::query_scalar("SELECT client_id FROM own_client_info")
            .fetch_one(db.read().await?.as_mut())
            .await?;
        assert_ne!(client_id, Uuid::nil());
        db.close().await;

        // Reset it to nil directly. The marker has re-applied, so a further open must leave it
        // alone rather than backfilling it again.
        let db = open_client_db(db_path, client_record_id).await?;
        sqlx::query("UPDATE own_client_info SET client_id = ?")
            .bind(Uuid::nil())
            .execute(db.write().await?.as_mut())
            .await?;
        db.close().await;

        let db = open_client_db(db_path, client_record_id).await?;
        let client_id: Uuid = sqlx::query_scalar("SELECT client_id FROM own_client_info")
            .fetch_one(db.read().await?.as_mut())
            .await?;
        assert_eq!(client_id, Uuid::nil());

        Ok(())
    }

    #[tokio::test]
    async fn modified_applied_migration_is_rejected() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let client_record_id = Uuid::new_v4();

        let db = open_client_db(db_path, client_record_id).await?;
        sqlx::query(
            "UPDATE _sqlx_migrations SET checksum = x'00'
            WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)",
        )
        .execute(db.write().await?.as_mut())
        .await?;
        db.close().await;

        let error = open_client_db(db_path, client_record_id).await.unwrap_err();
        assert!(matches!(
            error.downcast_ref(),
            Some(MigrateError::VersionMismatch(_))
        ));

        Ok(())
    }

    #[tokio::test]
    async fn dirty_migration_is_rejected() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let client_record_id = Uuid::new_v4();

        let db = open_client_db(db_path, client_record_id).await?;
        sqlx::query(
            "UPDATE _sqlx_migrations SET success = 0
            WHERE version = (SELECT MAX(version) FROM _sqlx_migrations)",
        )
        .execute(db.write().await?.as_mut())
        .await?;
        db.close().await;

        let error = open_client_db(db_path, client_record_id).await.unwrap_err();
        assert!(matches!(error.downcast_ref(), Some(MigrateError::Dirty(_))));

        Ok(())
    }

    #[tokio::test]
    async fn unknown_applied_migration_is_rejected() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let db_path = dir.path().to_str().unwrap();
        let client_record_id = Uuid::new_v4();

        let db = open_client_db(db_path, client_record_id).await?;
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
            VALUES (99999999999999, 'from the future', 1, x'00', 0)",
        )
        .execute(db.write().await?.as_mut())
        .await?;
        db.close().await;

        let error = open_client_db(db_path, client_record_id).await.unwrap_err();
        assert!(error.to_string().contains("unknown applied version"));

        Ok(())
    }
}
