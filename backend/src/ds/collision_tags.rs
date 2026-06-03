// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::{
    common::v1::{
        GenerationCollisionDetail, StatusDetails, StatusDetailsCode, status_details::Detail,
    },
    delivery_service::v1::CollisionTags,
};
use prost::Message as _;
use sqlx::{PgPool, PgTransaction};
use tonic::{Code, Status};
use tracing::error;
use uuid::Uuid;

/// Check collision tags against the DB and insert them if they are new.
///
/// Returns `Ok(())` if no collision was detected, or `Err(Status)` carrying a
/// `GenerationCollisionDetail` that tells the client which sorted-position tag
/// collided.
pub(super) async fn check_and_insert(
    pool: &PgPool,
    group_id: Uuid,
    epoch: i64,
    tags: CollisionTags,
) -> Result<(), Status> {
    let check_collision = async |tag: Vec<u8>, detail: GenerationCollisionDetail| {
        let inserted = sqlx::query_scalar!(
            "INSERT INTO ds_collision_tag (group_id, epoch, tag)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING",
            group_id,
            epoch,
            tag
        )
        .execute(pool)
        .await
        .map_err(|error| {
            error!(%error, "Failed to check/insert collision tags");
            Status::internal("storage error")
        })?;

        if inserted.rows_affected() == 1 {
            Ok(())
        } else {
            Err(Status::with_details(
                Code::AlreadyExists,
                "generation collision",
                StatusDetails {
                    code: StatusDetailsCode::GenerationCollision.into(),
                    detail: Some(Detail::GenerationCollision(detail.into())),
                }
                .encode_to_vec()
                .into(),
            ))
        }
    };

    check_collision(tags.tag1, GenerationCollisionDetail::Tag1).await?;
    check_collision(tags.tag2, GenerationCollisionDetail::Tag2).await?;

    Ok(())
}

/// Delete all collision tags for the given group that belong to epochs older
/// than `current_epoch - max_past_epochs`.
///
/// Called after a successful commit to keep the table bounded.
pub(super) async fn delete_old(
    txn: &mut PgTransaction<'_>,
    group_id: Uuid,
    current_epoch: u64,
    max_past_epochs: u64,
) -> sqlx::Result<()> {
    let cutoff = (current_epoch as i64) - (max_past_epochs as i64);
    sqlx::query!(
        "DELETE FROM ds_collision_tag WHERE group_id = $1 AND epoch < $2",
        group_id,
        cutoff,
    )
    .execute(txn.as_mut())
    .await?;

    Ok(())
}
