// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fmt::Display,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use aircommon::{codec::PersistenceCodec, crypto::ear::keys::DatabaseKek, identifiers::UserId};
use anyhow::{Context, Result, anyhow, bail};
use flate2::{Compression, bufread::GzDecoder, write::GzEncoder};
use openmls::group::GroupId;
use sqlx::{
    Database, Encode, Sqlite, SqlitePool, Type,
    encode::IsNull,
    error::BoxDynError,
    migrate,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use tracing::{error, info};

use crate::{
    clients::store::{ClientRecord, DatabaseDek},
    utils::{data_migrations, global_lock::GlobalLock},
};
pub(crate) const AIR_DB_NAME: &str = "air.db";

/// Open a connection to the DB that contains records for all clients on this
/// device.
pub(crate) async fn open_air_db(db_path: &str) -> sqlx::Result<SqlitePool> {
    let db_url = format!("sqlite://{db_path}/{AIR_DB_NAME}");
    let opts: SqliteConnectOptions = db_url.parse()?;
    let opts = opts
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts).await?;

    // Delete the old migration table if it exists
    const FIRST_MIGRATION: i64 = 20250115104336;
    if let Ok(Some(_)) = sqlx::query_scalar::<_, i64>(&format!(
        "SELECT 1 FROM _sqlx_migrations WHERE version = {FIRST_MIGRATION}"
    ))
    .fetch_optional(&pool)
    .await
    {
        // The database is based on old migration
        sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations")
            .execute(&pool)
            .await?;
    }

    migrate!("migrations/air").run(&pool).await?;

    Ok(pool)
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

/// Delete both the air.db and all client dbs from this device.
///
/// If the air.db exists, but cannot be opened, only the air.db is deleted.
///
/// WARNING: This will delete all APP-data from this device!
pub async fn delete_databases(client_db_path: &str) -> Result<()> {
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
    if let Ok(client_records) = ClientRecord::load_all(&air_db_connection).await {
        for client_record in client_records {
            let client_db_name = client_db_name(&client_record.user_id);
            let client_db_path = format!("{client_db_path}/{client_db_name}");
            info!(path =% client_db_path, "removing client DB");
            if let Err(error) = fs::remove_file(&client_db_path) {
                error!(%error, %client_db_path, "Failed to delete client DB")
            }
        }
    }
    Ok(())
}

pub async fn delete_client_database(db_path: &str, user_id: &UserId) -> Result<()> {
    // Delete the client DB
    // We also need to delete the WAL & SHM files
    let client_db_name = client_db_name(user_id);
    // Find all files that start with the client DB name and delete them
    for entry in fs::read_dir(db_path)? {
        if let Some(e) = entry?.file_name().to_str()
            && e.starts_with(&client_db_name)
        {
            let client_db_path = format!("{db_path}/{e}");
            info!(path =% client_db_path, "removing client DB related file");
            fs::remove_file(client_db_path)?;
        }
    }

    // Delete the client record from the air DB
    let full_air_db_path = format!("{db_path}/{AIR_DB_NAME}");
    if !Path::new(&full_air_db_path).exists() {
        bail!("air.db does not exist")
    }
    let air_db = open_air_db(db_path).await?;
    ClientRecord::delete(&air_db, user_id).await?;

    Ok(())
}

fn client_db_name(user_id: &UserId) -> String {
    format!("{}@{}.db", user_id.uuid(), user_id.domain())
}

pub async fn export_client_database(db_path: &str, user_id: &UserId) -> Result<Vec<u8>> {
    let client_db_name = client_db_name(user_id);

    // Commit the WAL to the database file
    let db_url = format!("sqlite://{db_path}/{client_db_name}");
    let opts: SqliteConnectOptions = db_url.parse()?;
    let opts = opts
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::default().connect_with(opts).await?;
    sqlx::query("VACUUM").execute(&pool).await?;
    pool.close().await;

    // Create a tar archive of the database
    let mut data = Vec::new();
    let enc = GzEncoder::new(&mut data, Compression::default());
    let mut tar = tar::Builder::new(enc);

    let client_db_path = format!("{db_path}/{client_db_name}");
    let content = fs::read(client_db_path)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(content.len().try_into().context("usize overflow")?);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, &client_db_name, content.as_slice())?;

    // Export the corresponding client record metadata so we can restore the DEK.
    let air_db = open_air_db(db_path).await?;
    let client_record = ClientRecord::load(&air_db, user_id)
        .await?
        .ok_or_else(|| anyhow!("client record not found"))?;
    let record_bytes = PersistenceCodec::to_vec(&client_record)?;
    let record_file_name = format!("{client_db_name}.record");
    let mut header = tar::Header::new_gnu();
    header.set_size(record_bytes.len().try_into().context("usize overflow")?);
    header.set_mode(0o600);
    header.set_cksum();
    tar.append_data(&mut header, record_file_name, record_bytes.as_slice())?;

    tar.finish()?;
    drop(tar);

    info!(?user_id, bytes = data.len(), "exported client DB");

    Ok(data)
}

pub async fn import_client_database(db_path: &str, tar_gz_bytes: &[u8]) -> Result<()> {
    let dec = GzDecoder::new(tar_gz_bytes);
    let mut tar = tar::Archive::new(dec);

    let mut imported_user_id: Option<UserId> = None;
    let mut imported_record: Option<(UserId, Vec<u8>)> = None;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        if let Some(user_id) = user_id_from_entry(&path) {
            let client_db_name = client_db_name(&user_id);
            let client_db_path = format!("{db_path}/{client_db_name}");
            info!(path =% client_db_path, "importing client DB");

            if Path::new(&client_db_path).exists() {
                bail!("client DB already exist: {client_db_path}");
            }

            std::io::copy(&mut entry, &mut File::create(&client_db_path)?)?;

            info!(?user_id, "imported client DB");
            imported_user_id = Some(user_id);
        } else if let Some(user_id) = user_id_from_record_entry(&path) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            imported_record = Some((user_id, bytes));
        }
    }

    let Some(user_id) = imported_user_id else {
        bail!("no client DB found in tar archive")
    };

    let Some((record_user_id, record_bytes)) = imported_record else {
        bail!("no client record metadata found in tar archive")
    };

    if record_user_id != user_id {
        bail!("client record metadata does not match imported DB");
    }

    let client_record: ClientRecord = PersistenceCodec::from_slice(&record_bytes)?;

    let air_db = open_air_db(db_path).await?;
    if ClientRecord::load(&air_db, &user_id).await?.is_some() {
        info!(?user_id, "client record already exists; skip adding it");
    } else {
        client_record.store(&air_db).await?;
        info!(?user_id, "added client record");
    }

    Ok(())
}

fn user_id_from_entry(path: &Path) -> Option<UserId> {
    let file_name = path.file_name()?.to_str()?;
    let (file_name, extension) = file_name.rsplit_once('.')?;
    if extension != "db" {
        return None;
    }
    let (user_id_str, domain) = file_name.split_once('@')?;
    let user_id = user_id_str.parse().ok()?;
    let domain = domain.parse().ok()?;
    let user_id = UserId::new(user_id, domain);
    Some(user_id)
}

fn user_id_from_record_entry(path: &Path) -> Option<UserId> {
    let file_name = path.file_name()?.to_str()?;
    let base = file_name.strip_suffix(".record")?;
    let faux_path = PathBuf::from(base);
    user_id_from_entry(&faux_path)
}

/// Open a client database with the provided DEK.
pub async fn open_client_db(
    user_id: &UserId,
    client_db_path: &str,
    kek: &DatabaseKek,
) -> anyhow::Result<SqlitePool> {
    // Fetch and decrypt DEK
    let air_db = open_air_db(client_db_path).await?;
    let record = ClientRecord::load(&air_db, user_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No client record found for user"))?;
    let dek = record.decrypt_dek(kek)?;

    open_client_db_with_dek(user_id, client_db_path, &dek).await
}

pub(crate) async fn open_client_db_with_dek(
    user_id: &UserId,
    client_db_path: &str,
    dek: &DatabaseDek,
) -> anyhow::Result<SqlitePool> {
    let client_db_name = client_db_name(user_id);
    let db_url = format!("sqlite://{client_db_path}/{client_db_name}");
    let opts: SqliteConnectOptions = db_url
        .parse()
        .map_err(|e| anyhow::anyhow!("Failed to parse database URL: {}", e))?;
    let opts = opts
        .pragma("key", format!("\"x'{}'\"", dek.to_hex_string()))
        .journal_mode(SqliteJournalMode::Wal)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::default()
        .connect_with(opts)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))?;

    data_migrations::migrate(&pool).await?;
    migrate!().run(&pool).await?;

    Ok(pool)
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
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'q>,
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
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
    use std::{fs, str::FromStr};
    use tempfile::tempdir;
    use uuid::Uuid;

    use crate::{clients::store::DatabaseDek, utils::persistence::open_client_db_with_dek};

    use aircommon::identifiers::{Fqdn, UserId};

    const MARKER: &str = "SECRET_MARKER_12345";

    /// Test that the encrypted database does not contain the plaintext marker
    #[tokio::test]
    async fn encrypted_db_marker() -> anyhow::Result<()> {
        let dir = tempdir()?;
        let plain_path = dir.path().join("plain.db");

        // Generate a DEK for testing
        let dek = DatabaseDek::random()?;

        // Encrypted DB via real code path
        let user_id = UserId::new(Uuid::new_v4(), Fqdn::from_str("example.test")?);
        let pool = open_client_db_with_dek(&user_id, dir.path().to_str().unwrap(), &dek).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS t (val TEXT)")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO t (val) VALUES (?1)")
            .bind(MARKER)
            .execute(&pool)
            .await?;
        // Force WAL checkpoint to ensure data is written to main db file
        sqlx::query("PRAGMA wal_checkpoint(FULL)")
            .execute(&pool)
            .await?;
        pool.close().await;

        // Discover the encrypted db file path that open_client_db created
        let mut enc_path: Option<std::path::PathBuf> = None;
        for entry in std::fs::read_dir(dir.path())? {
            let entry = entry?;
            if let Some(ext) = entry.path().extension()
                && ext == "db"
                && entry.file_name() != "plain.db"
            {
                enc_path = Some(entry.path());
                break;
            }
        }
        let enc_path = enc_path.expect("encrypted db file should exist");

        // Plaintext DB created manually
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", plain_path.display()))?
            .journal_mode(SqliteJournalMode::Wal)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::default().connect_with(opts).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS t (val TEXT)")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO t (val) VALUES (?1)")
            .bind(MARKER)
            .execute(&pool)
            .await?;
        // Force WAL checkpoint to ensure data is written db file
        sqlx::query("PRAGMA wal_checkpoint(FULL)")
            .execute(&pool)
            .await?;
        pool.close().await;

        // Read raw bytes
        let enc_bytes = fs::read(&enc_path)?;
        let plain_bytes = fs::read(&plain_path)?;

        // Plain should contain the marker in clear; encrypted should not
        assert!(
            !enc_bytes
                .windows(MARKER.len())
                .any(|w| w == MARKER.as_bytes()),
            "marker was found in encrypted db bytes"
        );
        assert!(
            plain_bytes
                .windows(MARKER.len())
                .any(|w| w == MARKER.as_bytes()),
            "marker not found in plaintext db bytes"
        );

        Ok(())
    }
}
