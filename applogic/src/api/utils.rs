// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Misc. functions

use super::types::UiUserId;

pub async fn delete_databases(db_path: String) -> anyhow::Result<()> {
    aircoreclient::delete_databases(&db_path).await
}

pub async fn delete_client_database(db_path: String, user_id: UiUserId) -> anyhow::Result<()> {
    aircoreclient::delete_client_database(&db_path, &user_id.into()).await
}

pub async fn export_client_database(db_path: String, user_id: UiUserId) -> anyhow::Result<Vec<u8>> {
    aircoreclient::export_client_database(&db_path, &user_id.into()).await
}

pub async fn import_client_database(db_path: String, tar_gz_bytes: Vec<u8>) -> anyhow::Result<()> {
    aircoreclient::import_client_database(&db_path, &tar_gz_bytes).await
}
