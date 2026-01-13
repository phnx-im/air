// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    messages::push_token::{PushToken, PushTokenOperator},
    time::{Duration, TimeStamp},
};
use anyhow::{Result, bail};
use sqlx::{SqliteExecutor, SqlitePool, SqliteTransaction, query, query_as};

const STATE_ID: i64 = 1;
pub(crate) const PUSH_TOKEN_PENDING_MAX_FUTURE_SECS: i64 = 300;

#[derive(Debug, Clone)]
pub(crate) struct PushTokenState {
    operator: Option<i64>,
    token: Option<String>,
    pending_update: Option<TimeStamp>,
}

impl PushTokenState {
    /// Rebuilds a push token from stored fields, erroring on inconsistent state.
    pub(crate) fn to_push_token(&self) -> Result<Option<PushToken>> {
        let Some(token) = self.token.as_ref() else {
            return Ok(None);
        };
        let Some(operator) = self.operator else {
            bail!("Push token operator missing while token exists");
        };
        let operator = operator_from_i64(operator)?;
        Ok(Some(PushToken::new(operator, token.clone())))
    }

    /// Checks whether the stored operator/token match the provided values.
    fn is_same(&self, operator: Option<i64>, token: Option<&str>) -> bool {
        self.operator == operator && self.token.as_deref() == token
    }
}

/// Loads the current persisted push token state, if any.
pub(crate) async fn load_state(
    executor: impl SqliteExecutor<'_>,
) -> sqlx::Result<Option<PushTokenState>> {
    query_as!(
        PushTokenState,
        r#"SELECT
            operator AS "operator: _",
            token,
            pending_update AS "pending_update: _"
        FROM push_token_state
        WHERE id = ?1"#,
        STATE_ID,
    )
    .fetch_optional(executor)
    .await
}

/// Loads the state only when a pending update is due.
pub(crate) async fn load_pending(
    executor: impl SqliteExecutor<'_>,
    now: TimeStamp,
) -> sqlx::Result<Option<PushTokenState>> {
    query_as!(
        PushTokenState,
        r#"SELECT
            operator AS "operator: _",
            token,
            pending_update AS "pending_update: _"
        FROM push_token_state
        WHERE id = ?1
            AND pending_update IS NOT NULL
            AND pending_update <= ?2"#,
        STATE_ID,
        now,
    )
    .fetch_optional(executor)
    .await
}

/// Clamps any far-future pending timestamp back to the allowed window.
pub(crate) async fn clamp_pending_future(
    executor: impl SqliteExecutor<'_>,
    now: TimeStamp,
) -> sqlx::Result<()> {
    let max_pending = max_pending_update(now);
    query!(
        "UPDATE push_token_state
        SET pending_update = ?1
        WHERE id = ?2 AND pending_update > ?1",
        max_pending,
        STATE_ID,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Updates state and sets a pending update when the token changes.
pub(crate) async fn mark_pending_if_changed(
    pool: &SqlitePool,
    push_token: Option<PushToken>,
) -> sqlx::Result<bool> {
    let mut txn = pool.begin_with("BEGIN IMMEDIATE").await?;
    let should_notify = mark_pending_if_changed_txn(&mut txn, push_token).await?;
    txn.commit().await?;
    Ok(should_notify)
}

/// Transactional helper to avoid racy read/modify/write updates.
async fn mark_pending_if_changed_txn(
    txn: &mut SqliteTransaction<'_>,
    push_token: Option<PushToken>,
) -> sqlx::Result<bool> {
    let existing = load_state(txn.as_mut()).await?;

    let (operator, token) = match push_token {
        Some(push_token) => (
            Some(operator_to_i64(push_token.operator())),
            Some(push_token.token().to_string()),
        ),
        None => (None, None),
    };

    if let Some(state) = existing {
        if state.is_same(operator, token.as_deref()) {
            return Ok(state.pending_update.is_some());
        }
    } else if operator.is_none() && token.is_none() {
        return Ok(false);
    }

    let now = TimeStamp::now();
    query!(
        "INSERT INTO push_token_state (id, operator, token, updated_at, pending_update)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(id) DO UPDATE SET
            operator = excluded.operator,
            token = excluded.token,
            updated_at = excluded.updated_at,
            pending_update = excluded.pending_update",
        STATE_ID,
        operator,
        token,
        now,
        now,
    )
    .execute(txn.as_mut())
    .await?;

    Ok(true)
}

/// Clears pending state after a successful update or a terminal failure.
pub(crate) async fn clear_pending(executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
    query!(
        "UPDATE push_token_state SET pending_update = NULL WHERE id = ?1",
        STATE_ID,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Schedules a retry, clamped to the max future window.
pub(crate) async fn schedule_retry(
    executor: impl SqliteExecutor<'_>,
    retry_at: TimeStamp,
) -> sqlx::Result<()> {
    let max_pending = max_pending_update(TimeStamp::now());
    let retry_at = if retry_at.as_ref() > max_pending.as_ref() {
        max_pending
    } else {
        retry_at
    };
    query!(
        "UPDATE push_token_state SET pending_update = ?1 WHERE id = ?2",
        retry_at,
        STATE_ID,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Computes the latest allowed pending timestamp relative to now.
fn max_pending_update(now: TimeStamp) -> TimeStamp {
    TimeStamp::from(*now.as_ref() + Duration::seconds(PUSH_TOKEN_PENDING_MAX_FUTURE_SECS))
}

/// Converts the operator enum into the persisted integer representation.
fn operator_to_i64(operator: &PushTokenOperator) -> i64 {
    match operator {
        PushTokenOperator::Apple => 0,
        PushTokenOperator::Google => 1,
    }
}

/// Parses the persisted integer representation into the operator enum.
fn operator_from_i64(value: i64) -> Result<PushTokenOperator> {
    match value {
        0 => Ok(PushTokenOperator::Apple),
        1 => Ok(PushTokenOperator::Google),
        _ => bail!("Unknown push token operator: {value}"),
    }
}

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;

    use super::*;

    #[sqlx::test]
    async fn mark_pending_and_clear_lifecycle(pool: SqlitePool) -> anyhow::Result<()> {
        // Empty DB + no token should be a no-op.
        let should_notify = mark_pending_if_changed(&pool, None).await?;
        assert!(!should_notify);
        assert!(load_state(&pool).await?.is_none());

        // First token write should set pending_update and persist state.
        let should_notify = mark_pending_if_changed(
            &pool,
            Some(PushToken::new(
                PushTokenOperator::Apple,
                "token-a".to_string(),
            )),
        )
        .await?;
        assert!(should_notify);
        let state = load_state(&pool).await?.expect("state should exist");
        assert!(state.pending_update.is_some());
        let push_token = state.to_push_token()?.expect("push token should exist");
        assert!(matches!(push_token.operator(), PushTokenOperator::Apple));
        assert_eq!(push_token.token(), "token-a");

        // Same token while pending should keep returning true.
        let should_notify = mark_pending_if_changed(
            &pool,
            Some(PushToken::new(
                PushTokenOperator::Apple,
                "token-a".to_string(),
            )),
        )
        .await?;
        assert!(should_notify);

        // Clearing pending should keep the state but drop the pending flag.
        clear_pending(&pool).await?;
        let state = load_state(&pool).await?.expect("state should exist");
        assert!(state.pending_update.is_none());

        // Same token after clearing should not trigger a pending update.
        let should_notify = mark_pending_if_changed(
            &pool,
            Some(PushToken::new(
                PushTokenOperator::Apple,
                "token-a".to_string(),
            )),
        )
        .await?;
        assert!(!should_notify);

        // Switching to None should schedule a pending update and clear fields.
        let should_notify = mark_pending_if_changed(&pool, None).await?;
        assert!(should_notify);
        let state = load_state(&pool).await?.expect("state should exist");
        assert!(state.operator.is_none());
        assert!(state.token.is_none());
        assert!(state.pending_update.is_some());

        Ok(())
    }

    #[sqlx::test]
    async fn pending_due_and_clamping(pool: SqlitePool) -> anyhow::Result<()> {
        mark_pending_if_changed(
            &pool,
            Some(PushToken::new(
                PushTokenOperator::Google,
                "token-b".to_string(),
            )),
        )
        .await?;

        let now = TimeStamp::now();

        // Pending updates should only be returned when due.
        let due_at = TimeStamp::from(*now.as_ref() + Duration::seconds(60));
        query!(
            "UPDATE push_token_state SET pending_update = ?1 WHERE id = ?2",
            due_at,
            STATE_ID,
        )
        .execute(&pool)
        .await?;
        assert!(load_pending(&pool, now).await?.is_none());
        assert!(load_pending(&pool, due_at).await?.is_some());

        // clamp_pending_future should cap far-future timestamps.
        let far_future = TimeStamp::from(
            *now.as_ref() + Duration::seconds(PUSH_TOKEN_PENDING_MAX_FUTURE_SECS + 1000),
        );
        query!(
            "UPDATE push_token_state SET pending_update = ?1 WHERE id = ?2",
            far_future,
            STATE_ID,
        )
        .execute(&pool)
        .await?;
        clamp_pending_future(&pool, now).await?;
        let state = load_state(&pool).await?.expect("state should exist");
        let clamped = state.pending_update.expect("pending_update should be set");
        let max_pending = max_pending_update(now);
        assert!(clamped.as_ref() <= max_pending.as_ref());

        // schedule_retry should also clamp to the max pending window.
        let far_retry = TimeStamp::from(
            *now.as_ref() + Duration::seconds(PUSH_TOKEN_PENDING_MAX_FUTURE_SECS + 1000),
        );
        schedule_retry(&pool, far_retry).await?;
        let state = load_state(&pool).await?.expect("state should exist");
        let scheduled = state.pending_update.expect("pending_update should be set");
        let upper_bound = max_pending_update(TimeStamp::now());
        assert!(scheduled.as_ref() <= upper_bound.as_ref());

        Ok(())
    }
}
