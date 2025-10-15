// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{ops::DerefMut, str::FromStr};

use aircommon::{
    crypto::{
        errors::DecryptionError,
        kdf::keys::RatchetSecret,
        ratchet::{QueueRatchet, RatchetPayload},
    },
    messages::{EncryptedQsQueueMessageCtype, QueueMessage, client_ds::QsQueueMessagePayload},
};
use sqlx::{
    Database, Decode, Encode, Sqlite, SqliteExecutor, Type, encode::IsNull, error::BoxDynError,
    query, query_scalar,
};
use tracing::error;

use super::*;

#[derive(Debug, Serialize, Deserialize, Copy, Clone)]
pub(crate) enum QueueType {
    Qs,
}

impl QueueType {
    fn as_str(&self) -> &'static str {
        match self {
            QueueType::Qs => "qs",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid queue type: {0}")]
pub(crate) struct QueueTypeParseError(String);

impl FromStr for QueueType {
    type Err = QueueTypeParseError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "qs" => Ok(Self::Qs),
            _ => Err(QueueTypeParseError(s.into())),
        }
    }
}

impl Type<Sqlite> for QueueType {
    fn type_info() -> <Sqlite as Database>::TypeInfo {
        <&str as Type<Sqlite>>::type_info()
    }
}

impl Encode<'_, Sqlite> for QueueType {
    fn encode_by_ref(
        &self,
        buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
    ) -> Result<IsNull, BoxDynError> {
        Encode::<Sqlite>::encode(self.as_str(), buf)
    }
}

impl Decode<'_, Sqlite> for QueueType {
    fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
        let s: &str = Decode::<Sqlite>::decode(value)?;
        Ok(s.parse()?)
    }
}

pub(crate) struct StorableQueueRatchet<Ciphertext, Payload: RatchetPayload<Ciphertext>> {
    queue_type: QueueType,
    queue_ratchet: QueueRatchet<Ciphertext, Payload>,
}

impl<Ciphertext, Payload: RatchetPayload<Ciphertext>> Deref
    for StorableQueueRatchet<Ciphertext, Payload>
{
    type Target = QueueRatchet<Ciphertext, Payload>;

    fn deref(&self) -> &Self::Target {
        &self.queue_ratchet
    }
}

impl<Ciphertext, Payload: RatchetPayload<Ciphertext>> DerefMut
    for StorableQueueRatchet<Ciphertext, Payload>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.queue_ratchet
    }
}

pub(crate) type StorableQsQueueRatchet =
    StorableQueueRatchet<EncryptedQsQueueMessageCtype, QsQueueMessagePayload>;

impl StorableQsQueueRatchet {
    pub(crate) async fn initialize(
        executor: impl SqliteExecutor<'_>,
        ratcht_secret: RatchetSecret,
    ) -> sqlx::Result<()> {
        Self {
            queue_type: QueueType::Qs,
            queue_ratchet: QueueRatchet::try_from(ratcht_secret).map_err(|error| {
                error!(%error, "Error initializing QS queue ratchet");
                // This is just a library error, so we hide it behind a sqlx
                // error.
                sqlx::Error::Decode(Box::new(error))
            })?,
        }
        .store(executor)
        .await?;
        Ok(())
    }

    /// Decrypt a `QueueMessage` received from the QS queue.
    ///
    /// This function is thread-safe via sqlite transactions.
    pub(crate) async fn decrypt_qs_queue_message(
        pool: &SqlitePool,
        qs_message_ciphertext: QueueMessage,
    ) -> Result<QsQueueMessagePayload, DecryptQsQueueMessageError> {
        // Makes sure reading the ratchet and updating it is atomic.
        let mut txn = pool.begin_with("BEGIN IMMEDIATE").await?;

        let mut qs_queue_ratchet = StorableQsQueueRatchet::load(txn.as_mut()).await?;

        let message_seq_nr = qs_message_ciphertext.sequence_number;
        let ratchet_seq_nr = qs_queue_ratchet.sequence_number();

        let payload = qs_queue_ratchet
            .decrypt(qs_message_ciphertext)
            .map_err(|error| DecryptQsQueueMessageError::Decrypt {
                error,
                ratchet_seq_nr,
                message_seq_nr,
            })?;
        qs_queue_ratchet.update(txn.as_mut(), QueueType::Qs).await?;

        txn.commit().await?;
        Ok(payload)
    }

    pub(crate) async fn load(executor: impl SqliteExecutor<'_>) -> sqlx::Result<Self> {
        StorableQueueRatchet::load_internal(executor, QueueType::Qs).await
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DecryptQsQueueMessageError {
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(
        "Failed to decrypt: \
            error = {error}, \
            message seq nr {message_seq_nr}, \
            ratchet seq nr {ratchet_seq_nr}"
    )]
    Decrypt {
        error: DecryptionError,
        ratchet_seq_nr: u64,
        message_seq_nr: u64,
    },
}

impl<Ciphertext, Payload> StorableQueueRatchet<Ciphertext, Payload>
where
    Ciphertext: Unpin + Send,
    Payload: RatchetPayload<Ciphertext> + Unpin + Send,
{
    async fn store(&self, executor: impl SqliteExecutor<'_>) -> sqlx::Result<()> {
        let sequence_number: i64 = self
            .queue_ratchet
            .sequence_number()
            .try_into()
            .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        query!(
            "INSERT INTO queue_ratchet
                (queue_type, queue_ratchet, sequence_number)
            VALUES (?, ?, ?)",
            self.queue_type,
            self.queue_ratchet,
            sequence_number,
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    async fn load_internal(
        executor: impl SqliteExecutor<'_>,
        queue_type: QueueType,
    ) -> sqlx::Result<Self> {
        let queue_ratchet = query_scalar!(
            r#"SELECT
                queue_ratchet AS "queue_ratchet: _"
            FROM queue_ratchet WHERE queue_type = ?"#,
            queue_type
        )
        .fetch_one(executor)
        .await?;
        Ok(Self {
            queue_type,
            queue_ratchet,
        })
    }

    async fn update(
        &self,
        executor: impl SqliteExecutor<'_>,
        queue_type: QueueType,
    ) -> sqlx::Result<()> {
        let sequence_number: i64 = self
            .queue_ratchet
            .sequence_number()
            .try_into()
            .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
        query!(
            "UPDATE queue_ratchet
            SET queue_ratchet = ?, sequence_number = ?
            WHERE queue_type = ?",
            self.queue_ratchet,
            sequence_number,
            queue_type
        )
        .execute(executor)
        .await?;
        Ok(())
    }
}
