// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::codec::PersistenceCodec;
use airprotos::auth_service::v1::OperationType;
use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::{
    clients::CoreUser,
    outbound_service::timed_tasks::{TimedTask, TimedTaskKind},
    privacy_pass,
};

#[derive(Debug, Clone)]
pub struct TimedTaskDebugInfo {
    pub name: String,
    pub scheduled_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UserDebugInfo {
    pub user_id: String,
    pub timed_tasks: Vec<TimedTaskDebugInfo>,
    pub add_username_token_count: u32,
    pub invitation_code_token_count: u32,
}

impl CoreUser {
    pub async fn user_debug_info(&self) -> anyhow::Result<UserDebugInfo> {
        let pool = self.pool();

        let uid = self.user_id();
        let user_id = format!("{}@{}", uid.uuid(), uid.domain());

        let rows = sqlx::query(
            "SELECT data, scheduled_at FROM operation
            WHERE kind = 'timed_task' ORDER BY scheduled_at ASC",
        )
        .fetch_all(pool)
        .await?;

        let mut timed_tasks = Vec::new();
        for row in rows {
            let data: Vec<u8> = row.get("data");
            let scheduled_at: DateTime<Utc> = row.get("scheduled_at");
            if let Ok(task) = PersistenceCodec::from_slice::<TimedTask>(&data) {
                timed_tasks.push(TimedTaskDebugInfo {
                    name: task.kind.display_name().to_string(),
                    scheduled_at,
                });
            }
        }

        let add_username_token_count =
            privacy_pass::persistence::token_count(pool, OperationType::AddUsername).await? as u32;
        let invitation_code_token_count =
            privacy_pass::persistence::token_count(pool, OperationType::GetInviteCode).await?
                as u32;

        Ok(UserDebugInfo {
            user_id,
            timed_tasks,
            add_username_token_count,
            invitation_code_token_count,
        })
    }
}

impl TimedTaskKind {
    fn display_name(&self) -> &'static str {
        match self {
            TimedTaskKind::KeyPackageUpload => "Key Package Upload",
            TimedTaskKind::UsernameRefresh => "Username Refresh",
            TimedTaskKind::SelfUpdate => "Self Update",
            TimedTaskKind::TokenReplenishment { operation_type } => match operation_type {
                OperationType::Unknown => "Unknown",
                OperationType::AddUsername => "Token Replenishment (Add Username)",
                OperationType::GetInviteCode => "Token Replenishment (Invite Code)",
            },
        }
    }
}
