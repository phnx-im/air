// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Code migration splitting attachments into content, remote location,
//! transfer and message references.

use aircommon::{codec::PersistenceCodec, identifiers::RemoteAttachmentId};
use chrono::Utc;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::{
    AttachmentId, MessageId, PartIndex,
    chats::messages::persistence::VersionedMessage,
    clients::attachment::{persistence::AttachmentStatus, process::AttachmentPart},
    db::access::WriteConnection,
};

/// Moves the transfer state and the remote location out of the attachment
/// rows and references each attachment from its message part.
///
/// Code migration paired with the migration creating the new tables. Rows no
/// part points to are deleted. Uses unchecked queries: the source columns do
/// not exist in the final schema.
pub(crate) async fn split_attachments(mut connection: impl WriteConnection) -> anyhow::Result<()> {
    // Status: 0 unknown, 1 pending, 2 downloading, 3 ready, 4 download failed,
    // 5 uploading, 6 upload failed, 7 not found
    sqlx::query(
        "INSERT INTO attachment_transfer (attachment_id, direction, state, failure, started_at)
        SELECT
            attachment_id,
            CASE WHEN status IN (5, 6) THEN 2 ELSE 1 END,
            CASE status WHEN 1 THEN 1 WHEN 2 THEN 1 WHEN 5 THEN 2 ELSE 3 END,
            CASE status WHEN 1 THEN NULL WHEN 2 THEN NULL WHEN 5 THEN NULL WHEN 7 THEN 2 ELSE 1 END,
            CASE status WHEN 5 THEN created_at ELSE NULL END
        FROM attachment
        WHERE status != 3",
    )
    .execute(connection.as_mut())
    .await?;

    sqlx::query(
        "INSERT INTO attachment_remote (
            attachment_id,
            remote_attachment_id,
            size,
            enc_alg,
            enc_key,
            nonce,
            aad,
            hash_alg,
            hash
        )
        SELECT
            a.attachment_id,
            p.remote_attachment_id,
            p.size,
            p.enc_alg,
            p.enc_key,
            p.nonce,
            p.aad,
            p.hash_alg,
            p.hash
        FROM pending_attachment p
        JOIN attachment a ON a.remote_attachment_id = p.remote_attachment_id",
    )
    .execute(connection.as_mut())
    .await?;

    let message_ids: Vec<MessageId> =
        sqlx::query_scalar("SELECT DISTINCT message_id FROM attachment")
            .fetch_all(connection.as_mut())
            .await?;
    for message_id in message_ids {
        reference_attachments(&mut connection, message_id).await?;
    }

    let deleted = sqlx::query(
        "DELETE FROM attachment
        WHERE attachment_id NOT IN (SELECT attachment_id FROM message_attachment)",
    )
    .execute(connection.as_mut())
    .await?
    .rows_affected();
    if deleted > 0 {
        info!(deleted, "Deleted attachments no message part points to");
    }

    Ok(())
}

/// An attachment row as stored before the split.
struct LegacyAttachment {
    attachment_id: AttachmentId,
    remote_attachment_id: Option<RemoteAttachmentId>,
}

/// Pairs the attachment parts of one message with its attachment rows.
async fn reference_attachments(
    mut connection: impl WriteConnection,
    message_id: MessageId,
) -> anyhow::Result<()> {
    let content: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT content FROM message WHERE message_id = ?")
            .bind(message_id)
            .fetch_optional(connection.as_mut())
            .await?;
    let Some(content) = content else {
        return Ok(());
    };
    // Decoding in the query would fail the whole migration on one bad row.
    let decoded = PersistenceCodec::from_slice::<VersionedMessage>(&content)
        .map_err(anyhow::Error::from)
        .and_then(|message| message.to_mimi_content());
    let mimi_content = match decoded {
        Ok(mimi_content) => mimi_content,
        Err(error) => {
            warn!(%error, %message_id, "Skipping attachments of an undecodable message");
            return Ok(());
        }
    };
    let parts = match AttachmentPart::collect(&mimi_content) {
        Ok(parts) => parts,
        Err(error) => {
            warn!(%error, %message_id, "Skipping attachments of an unwalkable message");
            return Ok(());
        }
    };

    let rows: Vec<(AttachmentId, Option<RemoteAttachmentId>)> = sqlx::query_as(
        "SELECT attachment_id, remote_attachment_id FROM attachment WHERE message_id = ?",
    )
    .bind(message_id)
    .fetch_all(connection.as_mut())
    .await?;
    let rows: Vec<LegacyAttachment> = rows
        .into_iter()
        .map(|(attachment_id, remote_attachment_id)| LegacyAttachment {
            attachment_id,
            remote_attachment_id,
        })
        .collect();

    for part in &parts {
        let part_remote_id = part.url().map(|url| url.remote_attachment_id());
        let Some(row) = find_row(&rows, message_id, part.index, part_remote_id) else {
            continue;
        };
        let Some(record) = part.record(
            row.attachment_id,
            message_id,
            AttachmentStatus::Ready,
            Utc::now(),
        ) else {
            continue;
        };

        sqlx::query(
            "INSERT INTO message_attachment (message_id, part_index, attachment_id, role, filename)
            VALUES (?, ?, ?, ?, ?)",
        )
        .bind(message_id)
        .bind(record.part_index)
        .bind(record.attachment_id)
        .bind(record.role)
        .bind(&record.filename)
        .execute(connection.as_mut())
        .await?;

        let size = record.size.and_then(|size| i64::try_from(size).ok());
        let (width, height) = record.dimensions.unzip();
        sqlx::query(
            "UPDATE attachment SET
                size = COALESCE(?, size),
                content_hash = COALESCE(?, content_hash),
                width = COALESCE(?, width),
                height = COALESCE(?, height),
                blurhash = COALESCE(?, blurhash)
            WHERE attachment_id = ?",
        )
        .bind(size)
        .bind(&record.content_hash)
        .bind(width)
        .bind(height)
        .bind(&record.blurhash)
        .bind(record.attachment_id)
        .execute(connection.as_mut())
        .await?;

        // A provisional part of an upload has no key yet
        if let Some(remote_attachment_id) = row.remote_attachment_id
            && let Some(remote) = part.remote()
            && !remote.enc_key.is_empty()
        {
            let size = i64::try_from(remote.size).ok();
            let enc_alg: u16 = remote.enc_alg.into();
            let hash_alg: u8 = remote.hash_alg.into();
            sqlx::query(
                "INSERT OR IGNORE INTO attachment_remote (
                    attachment_id,
                    remote_attachment_id,
                    size,
                    enc_alg,
                    enc_key,
                    nonce,
                    aad,
                    hash_alg,
                    hash
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(record.attachment_id)
            .bind(remote_attachment_id)
            .bind(size)
            .bind(enc_alg)
            .bind(&remote.enc_key)
            .bind(&remote.nonce)
            .bind(&remote.aad)
            .bind(hash_alg)
            .bind(&remote.hash)
            .execute(connection.as_mut())
            .await?;
        }
    }

    Ok(())
}

/// Finds the row of the part at `index` with the rules used before the split.
///
/// Matches the derived id first, unless the row is of another remote
/// attachment, then falls back to the remote id of the part.
fn find_row(
    rows: &[LegacyAttachment],
    message_id: MessageId,
    index: PartIndex,
    remote_attachment_id: Option<RemoteAttachmentId>,
) -> Option<&LegacyAttachment> {
    let derived = derived_attachment_id(message_id, index);
    if let Some(row) = rows.iter().find(|row| row.attachment_id == derived) {
        match (row.remote_attachment_id, remote_attachment_id) {
            (Some(stored), Some(part)) if stored != part => {}
            _ => return Some(row),
        }
    }
    let remote_attachment_id = remote_attachment_id?;
    rows.iter().find(|row| {
        row.remote_attachment_id == Some(remote_attachment_id)
            || row.attachment_id.uuid == remote_attachment_id.uuid()
    })
}

/// Id of the attachment at the given part, as derived before the split.
fn derived_attachment_id(message_id: MessageId, PartIndex(index): PartIndex) -> AttachmentId {
    let digest = Sha256::new()
        .chain_update(b"air attachment id")
        .chain_update(message_id.uuid.as_bytes())
        .chain_update(index.to_be_bytes())
        .finalize();
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    AttachmentId::from_raw(uuid::Builder::from_custom_bytes(bytes).into_uuid())
}

#[cfg(test)]
mod tests {
    use aircommon::{identifiers::UserId, time::TimeStamp};
    use chrono::{DateTime, SubsecRound};
    use mimi_content::{
        Disposition, MimiContent,
        content_container::{EncryptionAlgorithm, HashAlgorithm, NestedPart, PartSemantics},
    };
    use openmls::group::GroupId;
    use sqlx::{Row, SqlitePool};
    use uuid::{Uuid, uuid};

    use crate::{
        AttachmentUrl, ChatId, ChatMessage,
        chats::messages::ContentMessage,
        chats::persistence::tests::test_chat,
        db::access::DbAccess,
        utils::persistence::{open_db_in_memory_before, run_client_migrations},
    };

    use super::*;

    const ATTACHMENT_SPLIT_MIGRATION: i64 = 20261117120000;

    #[test]
    fn derived_attachment_ids_are_stable() {
        let message_id = MessageId::new(uuid!("b6a42a7a-62fa-4c10-acfb-6124d80aae09"));
        assert_eq!(
            derived_attachment_id(message_id, PartIndex(1)),
            derived_attachment_id(message_id, PartIndex(1))
        );
        assert_ne!(
            derived_attachment_id(message_id, PartIndex(0)),
            derived_attachment_id(message_id, PartIndex(1))
        );
    }

    fn external(disposition: Disposition, remote_id: RemoteAttachmentId, key: &[u8]) -> NestedPart {
        NestedPart::ExternalPart {
            disposition,
            language: String::new(),
            content_type: "image/webp".to_owned(),
            url: AttachmentUrl::new(remote_id, Some((4, 3))).to_string(),
            expires: 0,
            size: 42,
            enc_alg: EncryptionAlgorithm::Aes256Gcm,
            key: key.to_vec(),
            nonce: vec![2; 12],
            aad: Vec::new(),
            hash_alg: HashAlgorithm::Sha256,
            content_hash: vec![3; 32],
            description: String::new(),
            filename: "image.webp".to_owned(),
        }
    }

    fn multi(disposition: Disposition, parts: Vec<NestedPart>) -> NestedPart {
        NestedPart::MultiPart {
            disposition,
            language: String::new(),
            part_semantics: PartSemantics::ProcessAll,
            parts,
        }
    }

    async fn store_message(
        pool: &SqlitePool,
        chat_id: ChatId,
        nested_part: NestedPart,
    ) -> anyhow::Result<MessageId> {
        let message = ChatMessage::new_for_test(
            chat_id,
            MessageId::random(),
            TimeStamp::now(),
            ContentMessage::new(
                UserId::random("localhost".parse()?),
                false,
                MimiContent {
                    salt: Uuid::new_v4().as_bytes().to_vec(),
                    nested_part,
                    ..Default::default()
                },
                &GroupId::from_slice(&[0]),
            ),
        );
        message
            .store(DbAccess::for_tests(pool.clone()).write().await?)
            .await?;
        Ok(message.id())
    }

    struct Legacy {
        attachment_id: AttachmentId,
        remote_attachment_id: Option<RemoteAttachmentId>,
        message_id: MessageId,
        status: u32,
        created_at: DateTime<Utc>,
    }

    async fn store_legacy(pool: &SqlitePool, chat_id: ChatId, row: &Legacy) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO attachment (
                attachment_id,
                remote_attachment_id,
                chat_id,
                message_id,
                content_type,
                status,
                created_at
            ) VALUES (?, ?, ?, ?, 'image/webp', ?, ?)",
        )
        .bind(row.attachment_id)
        .bind(row.remote_attachment_id)
        .bind(chat_id)
        .bind(row.message_id)
        .bind(row.status)
        .bind(row.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn store_pending(
        pool: &SqlitePool,
        remote_attachment_id: RemoteAttachmentId,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO pending_attachment (
                remote_attachment_id, size, enc_alg, enc_key, nonce, aad, hash_alg, hash
            ) VALUES (?, 42, 1, x'01', x'02', x'', 1, x'03')",
        )
        .bind(remote_attachment_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn store_child_rows(
        pool: &SqlitePool,
        attachment_id: AttachmentId,
    ) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO attachment_content (attachment_id, content) VALUES (?, x'00')")
            .bind(attachment_id)
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO attachment_thumbnail (attachment_id, state, created_at)
            VALUES (?, 3, '2026-01-01T00:00:00Z')",
        )
        .bind(attachment_id)
        .execute(pool)
        .await?;
        Ok(())
    }

    async fn count(pool: &SqlitePool, table: &str) -> anyhow::Result<i64> {
        Ok(
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
                .fetch_one(pool)
                .await?,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn split_keeps_content_and_references_every_part() -> anyhow::Result<()> {
        let pool = open_db_in_memory_before(ATTACHMENT_SPLIT_MIGRATION).await?;
        let chat = test_chat();
        chat.store(DbAccess::for_tests(pool.clone()).write().await?)
            .await?;
        let created_at = Utc::now().round_subsecs(6);

        // One part per status, in an attachment message
        let statuses = [3, 1, 2, 4, 5, 6, 7, 0];
        let remote_ids: Vec<_> = statuses
            .iter()
            .map(|_| RemoteAttachmentId::new(Uuid::new_v4()))
            .collect();
        let parts = statuses
            .iter()
            .zip(&remote_ids)
            .map(|(&status, &remote_id)| {
                // Uploads have provisional parts without a key
                let key: &[u8] = match status {
                    5 | 6 => &[],
                    _ => &[1],
                };
                external(Disposition::Attachment, remote_id, key)
            })
            .collect();
        let message_id =
            store_message(&pool, chat.id(), multi(Disposition::Attachment, parts)).await?;
        let mut expected = Vec::new();
        for (index, (&status, &remote_id)) in statuses.iter().zip(&remote_ids).enumerate() {
            let index = PartIndex(u32::try_from(index)? + 1);
            let attachment_id = derived_attachment_id(message_id, index);
            let remote_attachment_id = match status {
                5 | 6 => None,
                _ => Some(remote_id),
            };
            let row = Legacy {
                attachment_id,
                remote_attachment_id,
                message_id,
                status,
                created_at,
            };
            store_legacy(&pool, chat.id(), &row).await?;
            match status {
                1 | 2 | 4 | 0 => store_pending(&pool, remote_id).await?,
                3 | 5 | 6 => store_child_rows(&pool, attachment_id).await?,
                _ => {}
            }
            expected.push((attachment_id, index, status));
        }

        // A link preview image at part 3
        let preview_remote_id = RemoteAttachmentId::new(Uuid::new_v4());
        let preview_message_id = store_message(
            &pool,
            chat.id(),
            multi(
                Disposition::Render,
                vec![
                    NestedPart::SinglePart {
                        disposition: Disposition::Render,
                        language: String::new(),
                        content_type: "text/markdown".to_owned(),
                        content: b"https://air.ms".to_vec(),
                    },
                    NestedPart::SinglePart {
                        disposition: Disposition::Preview,
                        language: String::new(),
                        content_type: "application/octet-stream".to_owned(),
                        content: Vec::new(),
                    },
                    external(Disposition::Preview, preview_remote_id, &[1]),
                ],
            ),
        )
        .await?;
        let preview_attachment_id = derived_attachment_id(preview_message_id, PartIndex(3));
        store_legacy(
            &pool,
            chat.id(),
            &Legacy {
                attachment_id: preview_attachment_id,
                remote_attachment_id: Some(preview_remote_id),
                message_id: preview_message_id,
                status: 1,
                created_at,
            },
        )
        .await?;
        store_pending(&pool, preview_remote_id).await?;

        // A row from before derived ids, paired by its remote id
        let legacy_remote_id = RemoteAttachmentId::new(Uuid::new_v4());
        let legacy_message_id = store_message(
            &pool,
            chat.id(),
            multi(
                Disposition::Attachment,
                vec![external(Disposition::Attachment, legacy_remote_id, &[1])],
            ),
        )
        .await?;
        let legacy_attachment_id = AttachmentId::random();
        store_legacy(
            &pool,
            chat.id(),
            &Legacy {
                attachment_id: legacy_attachment_id,
                remote_attachment_id: Some(legacy_remote_id),
                message_id: legacy_message_id,
                status: 3,
                created_at,
            },
        )
        .await?;
        store_child_rows(&pool, legacy_attachment_id).await?;

        // A leftover no part points to
        let leftover_id = AttachmentId::random();
        store_legacy(
            &pool,
            chat.id(),
            &Legacy {
                attachment_id: leftover_id,
                remote_attachment_id: None,
                message_id,
                status: 3,
                created_at,
            },
        )
        .await?;
        store_child_rows(&pool, leftover_id).await?;

        let content_before = count(&pool, "attachment_content").await?;
        let thumbnails_before = count(&pool, "attachment_thumbnail").await?;

        run_client_migrations(&DbAccess::for_tests(pool.clone())).await?;

        assert_eq!(
            count(&pool, "attachment_content").await?,
            content_before - 1
        );
        assert_eq!(
            count(&pool, "attachment_thumbnail").await?,
            thumbnails_before - 1
        );
        let leftover: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM attachment WHERE attachment_id = ?")
                .bind(leftover_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(leftover, 0);

        // References
        let references = sqlx::query(
            "SELECT message_id, part_index, attachment_id, role FROM message_attachment",
        )
        .fetch_all(&pool)
        .await?;
        assert_eq!(references.len(), statuses.len() + 2);
        let reference = |attachment_id: AttachmentId| {
            references
                .iter()
                .find(|row| row.get::<AttachmentId, _>("attachment_id") == attachment_id)
                .map(|row| {
                    (
                        row.get::<MessageId, _>("message_id"),
                        row.get::<u32, _>("part_index"),
                        row.get::<u32, _>("role"),
                    )
                })
        };
        for &(attachment_id, PartIndex(index), _) in &expected {
            assert_eq!(reference(attachment_id), Some((message_id, index, 1)));
        }
        assert_eq!(
            reference(preview_attachment_id),
            Some((preview_message_id, 3, 2))
        );
        assert_eq!(
            reference(legacy_attachment_id),
            Some((legacy_message_id, 1, 1))
        );

        // Transfers
        let transfer = async |attachment_id: AttachmentId| -> anyhow::Result<_> {
            let row = sqlx::query(
                "SELECT direction, state, failure, started_at
                FROM attachment_transfer WHERE attachment_id = ?",
            )
            .bind(attachment_id)
            .fetch_optional(&pool)
            .await?;
            Ok(row.map(|row| {
                (
                    row.get::<u32, _>("direction"),
                    row.get::<u32, _>("state"),
                    row.get::<Option<u32>, _>("failure"),
                    row.get::<Option<DateTime<Utc>>, _>("started_at"),
                )
            }))
        };
        for &(attachment_id, _, status) in &expected {
            let expected_transfer = match status {
                3 => None,
                1 | 2 => Some((1, 1, None, None)),
                4 | 0 => Some((1, 3, Some(1), None)),
                7 => Some((1, 3, Some(2), None)),
                5 => Some((2, 2, None, Some(created_at))),
                6 => Some((2, 3, Some(1), None)),
                _ => unreachable!(),
            };
            assert_eq!(
                transfer(attachment_id).await?,
                expected_transfer,
                "status {status}"
            );
        }
        assert_eq!(
            transfer(preview_attachment_id).await?,
            Some((1, 1, None, None))
        );
        assert_eq!(transfer(legacy_attachment_id).await?, None);

        // Remote locations: from the pending rows and from the downloaded parts
        let remote_ids_after: Vec<RemoteAttachmentId> =
            sqlx::query_scalar("SELECT remote_attachment_id FROM attachment_remote")
                .fetch_all(&pool)
                .await?;
        // All but the two uploads
        assert_eq!(remote_ids_after.len(), statuses.len() - 2 + 2);
        assert!(remote_ids_after.contains(&remote_ids[0]));
        assert!(remote_ids_after.contains(&legacy_remote_id));

        // Metadata from the parts
        let (size, width, height): (Option<i64>, Option<u32>, Option<u32>) =
            sqlx::query_as("SELECT size, width, height FROM attachment WHERE attachment_id = ?")
                .bind(legacy_attachment_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!((size, width, height), (Some(42), Some(4), Some(3)));

        // Schema
        let pending_tables: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'pending_attachment'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(pending_tables, 0);
        let removed_columns: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('attachment')
            WHERE name IN ('remote_attachment_id', 'chat_id', 'message_id', 'status')",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(removed_columns, 0);
        let violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await?;
        assert!(violations.is_empty());
        let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await?;
        assert_eq!(foreign_keys, 1);

        Ok(())
    }
}
