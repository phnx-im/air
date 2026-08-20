// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shows DsGroupState blob sizes on the DS database.
//!
//! The DS stores each group as a single encrypted blob that it deserializes,
//! mutates, re-encrypts and rewrites on every group operation, so blob size
//! is the direct driver of per-operation server cost. Retained ratchet trees
//! (one per add-epoch, 90-day expiry) dominate it; watch the biggest blob
//! while a stress run is going to see the growth rate.

use std::time::Duration;

use chrono::{DateTime, Utc};
use clap::Parser;
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(about = "Show DsGroupState blob sizes on the DS database")]
struct Args {
    /// DS database to inspect.
    #[arg(
        long,
        env = "DS_DATABASE_URL",
        default_value = "postgres://postgres:password@localhost:5432/air_db_ds"
    )]
    db_url: String,

    /// How many of the largest groups to list.
    #[arg(long, default_value_t = 10)]
    limit: i64,

    /// Re-report every this many seconds instead of exiting.
    #[arg(long, short)]
    watch: Option<u64>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let pool = PgPool::connect(&args.db_url).await?;

    loop {
        report(&pool, args.limit).await?;
        let Some(interval) = args.watch else {
            return Ok(());
        };
        tokio::time::sleep(Duration::from_secs(interval)).await;
        println!();
    }
}

async fn report(pool: &PgPool, limit: i64) -> anyhow::Result<()> {
    println!(
        "=== largest group blobs ({}) ===",
        Utc::now().format("%H:%M:%S")
    );
    let rows = sqlx::query(
        "SELECT group_id, pg_column_size(ciphertext) AS size, last_used
         FROM encrypted_group
         ORDER BY pg_column_size(ciphertext) DESC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let group_id: Uuid = row.try_get("group_id")?;
        let size: i32 = row.try_get("size")?;
        let last_used: DateTime<Utc> = row.try_get("last_used")?;
        println!(
            "  {group_id}  {:>10}  last used {}",
            pretty(size as u64),
            last_used.format("%Y-%m-%d %H:%M:%S"),
        );
    }

    let totals = sqlx::query(
        "SELECT count(*) AS groups,
                coalesce(sum(pg_column_size(ciphertext)), 0)::int8 AS live,
                coalesce(avg(pg_column_size(ciphertext)), 0)::int8 AS avg,
                pg_total_relation_size('encrypted_group') AS table_total
         FROM encrypted_group",
    )
    .fetch_one(pool)
    .await?;
    let groups: i64 = totals.try_get("groups")?;
    let live: i64 = totals.try_get("live")?;
    let avg: i64 = totals.try_get("avg")?;
    let table_total: i64 = totals.try_get("table_total")?;
    println!(
        "=== totals ===\n  {groups} groups, {} live blobs (avg {}), table {} incl. bloat",
        pretty(live as u64),
        pretty(avg as u64),
        pretty(table_total as u64),
    );

    println!("=== size distribution ===");
    let rows = sqlx::query(
        "SELECT CASE
                    WHEN pg_column_size(ciphertext) < 10240 THEN '<  10 kB'
                    WHEN pg_column_size(ciphertext) < 102400 THEN '<  100 kB'
                    WHEN pg_column_size(ciphertext) < 1048576 THEN '<  1 MB'
                    WHEN pg_column_size(ciphertext) < 10485760 THEN '<  10 MB'
                    ELSE '>= 10 MB'
                END AS bucket,
                count(*) AS groups,
                max(pg_column_size(ciphertext))::int8 AS largest
         FROM encrypted_group
         GROUP BY 1
         ORDER BY min(pg_column_size(ciphertext))",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let bucket: String = row.try_get("bucket")?;
        let groups: i64 = row.try_get("groups")?;
        let largest: i64 = row.try_get("largest")?;
        println!(
            "  {bucket:<10} {groups:>7} groups   largest {}",
            pretty(largest as u64)
        );
    }
    Ok(())
}

fn pretty(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MiB", b as f64 / (1u64 << 20) as f64),
        b if b >= 1 << 10 => format!("{:.1} KiB", b as f64 / (1u64 << 10) as f64),
        b => format!("{b} B"),
    }
}
