// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::identifiers::QsClientId;

use crate::{
    errors::QueueError,
    qs::{
        Qs,
        client_record::{OwnerState, QsClientRecord},
        user_record::UserRecord,
    },
};

/// What is left of a client record after its queue was acked.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AckOutcome {
    /// The client record still exists.
    ClientKept,
    /// The client record does not exist anymore.
    ClientDeleted,
}

impl Qs {
    /// Deletes the messages below `up_to_sequence_number` from the client's
    /// queue.
    ///
    /// If the client's user is deleted and the queue is empty afterwards, the client's record is
    /// deleted, and the user record too once no active client record is left.
    pub(crate) async fn ack_queue(
        &self,
        client_id: QsClientId,
        up_to_sequence_number: u64,
    ) -> Result<AckOutcome, QueueError> {
        let is_empty = self.queues.ack(client_id, up_to_sequence_number).await?;
        if !is_empty {
            return Ok(AckOutcome::ClientKept);
        }
        self.cleanup_drained_client(client_id).await
    }

    async fn cleanup_drained_client(
        &self,
        client_id: QsClientId,
    ) -> Result<AckOutcome, QueueError> {
        let user_id = match QsClientRecord::load_owner_state(&self.db_pool, &client_id).await? {
            None => return Ok(AckOutcome::ClientDeleted),
            Some(OwnerState::Active) => return Ok(AckOutcome::ClientKept),
            Some(OwnerState::Deleted(user_id)) => user_id,
        };

        let mut txn = self.db_pool.begin().await?;
        // Serializes the cleanup with the other clients of the user draining
        // their queues at the same time.
        if !UserRecord::lock_deleted(txn.as_mut(), user_id).await? {
            return Ok(AckOutcome::ClientDeleted);
        }
        UserRecord::delete_drained(&mut txn, user_id).await?;
        let owner_state = QsClientRecord::load_owner_state(txn.as_mut(), &client_id).await?;
        txn.commit().await?;

        Ok(match owner_state {
            None => AckOutcome::ClientDeleted,
            Some(OwnerState::Active | OwnerState::Deleted(_)) => AckOutcome::ClientKept,
        })
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use tokio_util::sync::CancellationToken;

    use crate::{
        air_service::BackendService,
        qs::{
            client_record::persistence::tests::store_random_client_record,
            queue::tests::{enqueue_test_messages, queue_len},
            user_record::persistence::tests::{count_rows, store_random_user_record},
        },
    };

    use super::*;

    async fn qs(pool: &PgPool) -> anyhow::Result<Qs> {
        let qs = Qs::initialize(
            pool.clone(),
            "example.com".parse()?,
            Default::default(),
            0,
            CancellationToken::new(),
        )
        .await?;
        Ok(qs)
    }

    #[sqlx::test]
    async fn ack_queue_drains_deleted_user(pool: PgPool) -> anyhow::Result<()> {
        let qs = qs(&pool).await?;
        let user = store_random_user_record(&pool).await?;
        let user_id = user.user_id;
        let client_a = store_random_client_record(&pool, user_id).await?;
        let client_b = store_random_client_record(&pool, user_id).await?;
        let client_c = store_random_client_record(&pool, user_id).await?;
        for client in [&client_a, &client_b, &client_c] {
            enqueue_test_messages(&pool, client.client_id, 2).await?;
        }
        // A tombstoned client with a non-empty queue goes with the user.
        QsClientRecord::soft_delete(&pool, &client_c.client_id).await?;
        UserRecord::soft_delete(&mut *pool.acquire().await?, user_id).await?;
        assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 3);

        assert_eq!(
            qs.ack_queue(client_a.client_id, 2).await?,
            AckOutcome::ClientDeleted
        );
        assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 2);
        assert_eq!(count_rows(&pool, "qs_user_record", user_id).await?, 1);

        assert_eq!(
            qs.ack_queue(client_b.client_id, 2).await?,
            AckOutcome::ClientDeleted
        );

        assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 0);
        assert_eq!(count_rows(&pool, "qs_user_record", user_id).await?, 0);
        assert_eq!(queue_len(&pool, client_c.client_id).await?, 0);
        assert_eq!(
            qs.ack_queue(client_b.client_id, 2).await?,
            AckOutcome::ClientDeleted
        );

        Ok(())
    }

    #[sqlx::test]
    async fn ack_queue_keeps_pending_sibling(pool: PgPool) -> anyhow::Result<()> {
        let qs = qs(&pool).await?;
        let user = store_random_user_record(&pool).await?;
        let user_id = user.user_id;
        let client = store_random_client_record(&pool, user_id).await?;
        let pending_client = store_random_client_record(&pool, user_id).await?;
        for client in [&client, &pending_client] {
            enqueue_test_messages(&pool, client.client_id, 2).await?;
        }
        UserRecord::soft_delete(&mut *pool.acquire().await?, user_id).await?;

        assert_eq!(
            qs.ack_queue(client.client_id, 1).await?,
            AckOutcome::ClientKept
        );
        assert_eq!(count_rows(&pool, "qs_client_record", user_id).await?, 2);

        assert_eq!(
            qs.ack_queue(client.client_id, 2).await?,
            AckOutcome::ClientDeleted
        );
        assert_eq!(QsClientRecord::load(&pool, &client.client_id).await?, None);
        assert_eq!(
            QsClientRecord::load(&pool, &pending_client.client_id).await?,
            Some(QsClientRecord {
                encrypted_push_token: None,
                ..pending_client
            })
        );
        assert_eq!(count_rows(&pool, "qs_user_record", user_id).await?, 1);

        Ok(())
    }

    #[sqlx::test]
    async fn ack_queue_keeps_active_user(pool: PgPool) -> anyhow::Result<()> {
        let qs = qs(&pool).await?;
        let user = store_random_user_record(&pool).await?;
        let client = store_random_client_record(&pool, user.user_id).await?;
        enqueue_test_messages(&pool, client.client_id, 2).await?;

        assert_eq!(
            qs.ack_queue(client.client_id, 2).await?,
            AckOutcome::ClientKept
        );

        assert_eq!(queue_len(&pool, client.client_id).await?, 0);
        assert_eq!(
            QsClientRecord::load(&pool, &client.client_id).await?,
            Some(client)
        );
        assert_eq!(UserRecord::load(&pool, &user.user_id).await?, Some(user));

        Ok(())
    }
}
