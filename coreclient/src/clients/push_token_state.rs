// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    messages::push_token::{PushToken, PushTokenOperator},
    time::TimeStamp,
};
use anyhow::{Result, bail};
use sqlx::{Row, SqlitePool};

const STATE_ID: i64 = 1;

#[derive(Debug, Clone)]
pub(crate) struct PushTokenState {
    operator: Option<i64>,
    token: Option<String>,
    pending_update: bool,
}

impl PushTokenState {
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

    fn is_same(&self, operator: Option<i64>, token: Option<&str>) -> bool {
        self.operator == operator && self.token.as_deref() == token
    }
}

pub(crate) async fn load_state(pool: &SqlitePool) -> sqlx::Result<Option<PushTokenState>> {
    let row =
        sqlx::query("SELECT operator, token, pending_update FROM push_token_state WHERE id = ?1")
            .bind(STATE_ID)
            .fetch_optional(pool)
            .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let operator = row.try_get("operator")?;
    let token = row.try_get("token")?;
    let pending_update: i64 = row.try_get("pending_update")?;
    Ok(Some(PushTokenState {
        operator,
        token,
        pending_update: pending_update != 0,
    }))
}

pub(crate) async fn load_pending(pool: &SqlitePool) -> sqlx::Result<Option<PushTokenState>> {
    let row = sqlx::query(
        "SELECT operator, token, pending_update FROM push_token_state
        WHERE id = ?1 AND pending_update = 1",
    )
    .bind(STATE_ID)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let operator = row.try_get("operator")?;
    let token = row.try_get("token")?;
    let pending_update: i64 = row.try_get("pending_update")?;
    Ok(Some(PushTokenState {
        operator,
        token,
        pending_update: pending_update != 0,
    }))
}

pub(crate) async fn mark_pending_if_changed(
    pool: &SqlitePool,
    push_token: Option<PushToken>,
) -> sqlx::Result<bool> {
    let existing = load_state(pool).await?;

    let (operator, token) = match push_token {
        Some(push_token) => (
            Some(operator_to_i64(push_token.operator())),
            Some(push_token.token().to_string()),
        ),
        None => (None, None),
    };

    if let Some(state) = existing {
        if state.is_same(operator, token.as_deref()) {
            return Ok(state.pending_update);
        }
    } else if operator.is_none() && token.is_none() {
        return Ok(false);
    }

    let updated_at = TimeStamp::now();
    sqlx::query(
        "INSERT INTO push_token_state (id, operator, token, updated_at, pending_update)
        VALUES (?1, ?2, ?3, ?4, 1)
        ON CONFLICT(id) DO UPDATE SET
            operator = excluded.operator,
            token = excluded.token,
            updated_at = excluded.updated_at,
            pending_update = 1",
    )
    .bind(STATE_ID)
    .bind(operator)
    .bind(token)
    .bind(updated_at)
    .execute(pool)
    .await?;

    Ok(true)
}

pub(crate) async fn clear_pending(pool: &SqlitePool) -> sqlx::Result<()> {
    sqlx::query("UPDATE push_token_state SET pending_update = 0 WHERE id = ?1")
        .bind(STATE_ID)
        .execute(pool)
        .await?;
    Ok(())
}

fn operator_to_i64(operator: &PushTokenOperator) -> i64 {
    match operator {
        PushTokenOperator::Apple => 0,
        PushTokenOperator::Google => 1,
    }
}

fn operator_from_i64(value: i64) -> Result<PushTokenOperator> {
    match value {
        0 => Ok(PushTokenOperator::Apple),
        1 => Ok(PushTokenOperator::Google),
        _ => bail!("Unknown push token operator: {value}"),
    }
}
