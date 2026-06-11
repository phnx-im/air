// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airprotos::common::v1::{
    GenerationCollisionDetail, StatusDetails, StatusDetailsCode, status_details::Detail,
};
use prost::Message;
use sqlx::{PgExecutor, PgPool};
use tonic::Code;
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum CollisionTagError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error("one or more tag(s) collided")]
    Collision { collisions: Vec<i64> },
    #[error("duplicate tag in input {0:x}")]
    DuplicateTag(i64),
}

impl From<CollisionTagError> for tonic::Status {
    fn from(error: CollisionTagError) -> Self {
        match error {
            CollisionTagError::Database(error) => {
                tracing::error!(%error, "failed to insert collision tag");
                Self::internal("database error")
            }
            CollisionTagError::Collision { collisions: tags } => Self::with_details(
                Code::AlreadyExists,
                "generation collision",
                StatusDetails {
                    code: StatusDetailsCode::GenerationCollision.into(),
                    detail: Some(Detail::GenerationCollision(GenerationCollisionDetail {
                        tags,
                    })),
                }
                .encode_to_vec()
                .into(),
            ),
            CollisionTagError::DuplicateTag(_) => Self::invalid_argument("duplicate tag in input"),
        }
    }
}

/// Check collision tags against the DB and insert them if they are new.
///
/// Returns `Ok(())` if no collision was detected, or `Err(Status)` carrying a
/// `GenerationCollisionDetail` that tells the client which sorted-position tag
/// collided.
pub(super) async fn check_and_insert(
    pool: &PgPool,
    group_id: Uuid,
    epoch: i64,
    mut tags: Vec<i64>,
) -> Result<(), CollisionTagError> {
    tags.sort_unstable();
    if let Some(w) = tags.windows(2).find(|w| w[0] == w[1]) {
        return Err(CollisionTagError::DuplicateTag(w[0]));
    }

    let mut tx = pool.begin().await?;
    let collisions: Vec<i64> = sqlx::query_scalar!(
        r#"
          WITH ins AS (
              INSERT INTO ds_collision_tag (group_id, epoch, tag)
              SELECT $1, $2, unnest($3::bigint[])
              ON CONFLICT (group_id, epoch, tag) DO NOTHING
              RETURNING tag
          )
          SELECT u.tag AS "tag!"
          FROM unnest($3::bigint[]) AS u(tag)
          EXCEPT
          SELECT tag FROM ins
          "#,
        group_id,
        epoch,
        &tags,
    )
    .fetch_all(&mut *tx)
    .await?;

    if collisions.is_empty() {
        tx.commit().await?;
        Ok(())
    } else {
        tx.rollback().await?;
        Err(CollisionTagError::Collision { collisions })
    }
}

/// Delete all collision tags for the given group that belong to epochs older
/// than `current_epoch - max_past_epochs`.
///
/// Called after a successful commit to keep the table bounded.
pub(super) async fn delete_old(
    connection: impl PgExecutor<'_>,
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
    .execute(connection)
    .await?;

    Ok(())
}
