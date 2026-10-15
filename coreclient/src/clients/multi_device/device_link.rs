// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The self-group changes a device link wants to make, kept in the database so
//! they survive the linking flow.
//!
//! The linking flow records each step of a link. The outbound service turns the
//! record into self-group commits. A link left unfinished past its deadline
//! counts as abandoned, so a crash on the existing device does not leave a
//! half-linked device behind.

use aircommon::{
    codec::{BlobDecoded, BlobEncoded},
    crypto::signatures::keys::QsClientSigningKey,
    identifiers::QsClientId,
};
use anyhow::bail;
use chrono::{DateTime, Utc};
use sqlx::{query, query_as};
use uuid::Uuid;

use crate::db::access::{ReadConnection, WriteConnection};

pub(crate) use super::payloads::SelfGroupJoinRequest;

/// How far a device link has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeviceLinkState {
    /// The new device's queue exists, but the new device has not asked to
    /// join the self group yet.
    Provisioned,
    /// The outbound service adds the new device to the self group.
    Adding,
    /// The add failed for good.
    Failed,
    /// The link will not complete. The outbound service takes the new device
    /// out of the self group if it got in, and deletes its queue.
    Abandoned,
}

impl DeviceLinkState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Provisioned => "provisioned",
            Self::Adding => "adding",
            Self::Failed => "failed",
            Self::Abandoned => "abandoned",
        }
    }

    fn parse(state: &str) -> anyhow::Result<Self> {
        Ok(match state {
            "provisioned" => Self::Provisioned,
            "adding" => Self::Adding,
            "failed" => Self::Failed,
            "abandoned" => Self::Abandoned,
            other => bail!("unknown device link state {other}"),
        })
    }
}

/// Why the DS refused the add for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum DeviceLinkFailure {
    DeviceLimitReached { max_devices: u32 },
    Rejected,
}

/// The queue the existing device created for the new device.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct ProvisionedQueue {
    pub(crate) qs_client_id: QsClientId,
    pub(crate) qs_client_signing_key: QsClientSigningKey,
}

pub(crate) struct DeviceLink {
    pub(crate) link_id: Uuid,
    pub(crate) state: DeviceLinkState,
    pub(crate) queue: ProvisionedQueue,
    pub(crate) client_id: Option<Uuid>,
    pub(crate) join_request: Option<SelfGroupJoinRequest>,
    pub(crate) failure: Option<DeviceLinkFailure>,
}

struct SqlDeviceLink {
    link_id: Uuid,
    state: String,
    queue: BlobDecoded<ProvisionedQueue>,
    client_id: Option<Uuid>,
    join_request: Option<BlobDecoded<SelfGroupJoinRequest>>,
    failure: Option<BlobDecoded<DeviceLinkFailure>>,
}

impl TryFrom<SqlDeviceLink> for DeviceLink {
    type Error = anyhow::Error;

    fn try_from(row: SqlDeviceLink) -> anyhow::Result<Self> {
        Ok(Self {
            link_id: row.link_id,
            state: DeviceLinkState::parse(&row.state)?,
            queue: row.queue.into_inner(),
            client_id: row.client_id,
            join_request: row.join_request.map(BlobDecoded::into_inner),
            failure: row.failure.map(BlobDecoded::into_inner),
        })
    }
}

impl DeviceLink {
    /// Records a link whose new device just got its queue.
    pub(crate) async fn create(
        mut connection: impl WriteConnection,
        queue: ProvisionedQueue,
        expires_at: DateTime<Utc>,
    ) -> sqlx::Result<Uuid> {
        let link_id = Uuid::new_v4();
        let state = DeviceLinkState::Provisioned.as_str();
        let queue = BlobEncoded(queue);
        query!(
            "INSERT INTO device_link (link_id, state, queue, expires_at)
            VALUES (?1, ?2, ?3, ?4)",
            link_id,
            state,
            queue,
            expires_at,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(link_id)
    }

    pub(crate) async fn load(
        mut connection: impl ReadConnection,
        link_id: Uuid,
    ) -> anyhow::Result<Option<Self>> {
        let row = query_as!(
            SqlDeviceLink,
            r#"SELECT
                link_id AS "link_id: _",
                state,
                queue AS "queue: _",
                client_id AS "client_id: _",
                join_request AS "join_request: _",
                failure AS "failure: _"
            FROM device_link
            WHERE link_id = ?1"#,
            link_id,
        )
        .fetch_optional(connection.as_mut())
        .await?;
        row.map(DeviceLink::try_from).transpose()
    }

    /// The links the outbound service has work for.
    pub(crate) async fn load_actionable(
        mut connection: impl ReadConnection,
    ) -> anyhow::Result<Vec<Self>> {
        let adding = DeviceLinkState::Adding.as_str();
        let abandoned = DeviceLinkState::Abandoned.as_str();
        let rows = query_as!(
            SqlDeviceLink,
            r#"SELECT
                link_id AS "link_id: _",
                state,
                queue AS "queue: _",
                client_id AS "client_id: _",
                join_request AS "join_request: _",
                failure AS "failure: _"
            FROM device_link
            WHERE state IN (?1, ?2)"#,
            adding,
            abandoned,
        )
        .fetch_all(connection.as_mut())
        .await?;
        rows.into_iter().map(DeviceLink::try_from).collect()
    }

    /// Hands the outbound service the new device's join request.
    ///
    /// Fails if the link was abandoned in the meantime.
    pub(crate) async fn request_add(
        mut connection: impl WriteConnection,
        link_id: Uuid,
        request: SelfGroupJoinRequest,
        expires_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let adding = DeviceLinkState::Adding.as_str();
        let provisioned = DeviceLinkState::Provisioned.as_str();
        let client_id = request.device.client_id;
        let request = BlobEncoded(request);
        let updated = query!(
            "UPDATE device_link
            SET state = ?1, client_id = ?2, join_request = ?3, expires_at = ?4
            WHERE link_id = ?5 AND state = ?6",
            adding,
            client_id,
            request,
            expires_at,
            link_id,
            provisioned,
        )
        .execute(connection.as_mut())
        .await?;
        if updated.rows_affected() == 0 {
            bail!("the device link was abandoned before the add was requested");
        }
        Ok(())
    }

    /// Records that the DS refused to add the device with `client_id`.
    pub(crate) async fn fail_add(
        mut connection: impl WriteConnection,
        client_id: Uuid,
        failure: DeviceLinkFailure,
    ) -> sqlx::Result<()> {
        let failed = DeviceLinkState::Failed.as_str();
        let adding = DeviceLinkState::Adding.as_str();
        let failure = BlobEncoded(failure);
        query!(
            "UPDATE device_link SET state = ?1, failure = ?2
            WHERE client_id = ?3 AND state = ?4",
            failed,
            failure,
            client_id,
            adding,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Gives up on a link, so the outbound service undoes it.
    pub(crate) async fn abandon(
        mut connection: impl WriteConnection,
        link_id: Uuid,
    ) -> sqlx::Result<()> {
        let abandoned = DeviceLinkState::Abandoned.as_str();
        query!(
            "UPDATE device_link SET state = ?1 WHERE link_id = ?2",
            abandoned,
            link_id,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Gives up on every unfinished link whose deadline passed.
    pub(crate) async fn abandon_expired(
        mut connection: impl WriteConnection,
        now: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        let abandoned = DeviceLinkState::Abandoned.as_str();
        query!(
            "UPDATE device_link SET state = ?1 WHERE state != ?1 AND expires_at <= ?2",
            abandoned,
            now,
        )
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    /// Finishes a link the new device confirmed.
    ///
    /// Fails if the link was abandoned in the meantime.
    pub(crate) async fn complete(
        mut connection: impl WriteConnection,
        link_id: Uuid,
    ) -> anyhow::Result<()> {
        let adding = DeviceLinkState::Adding.as_str();
        let deleted = query!(
            "DELETE FROM device_link WHERE link_id = ?1 AND state = ?2",
            link_id,
            adding,
        )
        .execute(connection.as_mut())
        .await?;
        if deleted.rows_affected() == 0 {
            bail!("the device link was abandoned before it completed");
        }
        Ok(())
    }

    /// Forgets a link that has been undone.
    pub(crate) async fn delete(
        mut connection: impl WriteConnection,
        link_id: Uuid,
    ) -> sqlx::Result<()> {
        query!("DELETE FROM device_link WHERE link_id = ?1", link_id)
            .execute(connection.as_mut())
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Context as _;
    use chrono::Duration;

    use crate::{db::access::DbAccess, utils::persistence::open_db_in_memory};

    use super::*;

    fn queue() -> anyhow::Result<ProvisionedQueue> {
        Ok(ProvisionedQueue {
            qs_client_id: Uuid::new_v4().into(),
            qs_client_signing_key: QsClientSigningKey::generate()?,
        })
    }

    async fn state(pool: &DbAccess, link_id: Uuid) -> anyhow::Result<DeviceLinkState> {
        Ok(DeviceLink::load(pool.read().await?, link_id)
            .await?
            .context("the link should be stored")?
            .state)
    }

    #[tokio::test]
    async fn only_expired_links_are_abandoned() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let now = Utc::now();
        let expired = DeviceLink::create(pool.write().await?, queue()?, now).await?;
        let live =
            DeviceLink::create(pool.write().await?, queue()?, now + Duration::minutes(10)).await?;

        DeviceLink::abandon_expired(pool.write().await?, now).await?;

        assert_eq!(state(&pool, expired).await?, DeviceLinkState::Abandoned);
        assert_eq!(state(&pool, live).await?, DeviceLinkState::Provisioned);
        let actionable = DeviceLink::load_actionable(pool.read().await?).await?;
        assert_eq!(
            actionable
                .iter()
                .map(|link| link.link_id)
                .collect::<Vec<_>>(),
            vec![expired]
        );
        Ok(())
    }

    #[tokio::test]
    async fn an_abandoned_link_does_not_complete() -> anyhow::Result<()> {
        let pool = DbAccess::for_tests(open_db_in_memory().await?);
        let link_id = DeviceLink::create(
            pool.write().await?,
            queue()?,
            Utc::now() + Duration::minutes(10),
        )
        .await?;

        DeviceLink::abandon(pool.write().await?, link_id).await?;

        assert!(
            DeviceLink::complete(pool.write().await?, link_id)
                .await
                .is_err()
        );
        assert_eq!(state(&pool, link_id).await?, DeviceLinkState::Abandoned);
        Ok(())
    }
}
