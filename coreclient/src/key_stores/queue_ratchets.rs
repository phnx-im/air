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
    Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError, query, query_scalar,
};
use tracing::error;

use crate::db::access::{ReadConnection, WriteConnection, WriteDbTransaction};

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
        connection: impl WriteConnection,
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
        .store(connection)
        .await?;
        Ok(())
    }

    /// Decrypt a `QueueMessage` received from the QS queue.
    ///
    /// # Contract
    ///
    /// QS is expected to deliver, in strict sequence order, only messages whose `sequence_number`
    /// is at or above the client's locally persisted ratchet sequence number: the value the client
    /// passes as `sequence_number_start` when it opens the listen stream (see
    /// [`CoreUser::listen_queue`]). Under that contract, every received message satisfies
    /// `message_seq_nr == ratchet_seq_nr` on arrival, the ratchet decrypts it with the current key,
    /// and advances by one.
    ///
    /// The two non-happy-path branches below recover from a violation of that contract:
    ///
    /// * `message_seq_nr > ratchet_seq_nr`: a gap. The ratchet is forward-only, so we ratchet
    ///   forward through the gap to decrypt this message; any messages at the skipped sequence
    ///   numbers become permanently undecryptable. Lossy, but unavoidable given the forward-only
    ///   design.
    ///
    /// * `message_seq_nr < ratchet_seq_nr`: a replay of an already-consumed sequence. Returns
    ///   `Ok(None)` so the caller skips this message; the ratchet is not updated. The most common
    ///   trigger is a *client-side* violation: the listen start seq we sent to the server was stale
    ///   relative to our actual ratchet (e.g., read from a lagging read-pool snapshot while the
    ///   write-pool ratchet had already advanced).
    pub(crate) async fn decrypt_qs_queue_message(
        txn: &mut WriteDbTransaction<'_>,
        qs_message_ciphertext: QueueMessage,
    ) -> Result<Option<QsQueueMessagePayload>, DecryptQsQueueMessageError> {
        let mut qs_queue_ratchet = StorableQsQueueRatchet::load(&mut *txn).await?;

        let message_seq_nr = qs_message_ciphertext.sequence_number;
        let ratchet_seq_nr = qs_queue_ratchet.sequence_number();

        if message_seq_nr > ratchet_seq_nr {
            // In case the message sequence number is ahead of the ratchet, we need to ratchet
            // forward to catch up. This really shouldn't happen, so we log an error in case it
            // does.
            error!(
                "QS queue ratchet is behind message sequence number: \
                    ratchet_seq_nr = {}, \
                    message_seq_nr = {}",
                ratchet_seq_nr, message_seq_nr
            );
            while message_seq_nr > qs_queue_ratchet.sequence_number() {
                qs_queue_ratchet.ratchet_forward().map_err(|error| {
                    DecryptQsQueueMessageError::Decrypt {
                        error: error.into(),
                        ratchet_seq_nr,
                        message_seq_nr,
                    }
                })?;
            }
        } else if message_seq_nr < ratchet_seq_nr {
            // In case the message sequence number is behind the ratchet, this is most likely a
            // replay of already received message. We log an error and skip the message.
            error!(
                "QS queue ratchet is ahead of message sequence number: \
                    ratchet_seq_nr = {}, \
                    message_seq_nr = {}",
                ratchet_seq_nr, message_seq_nr
            );
            return Ok(None);
        }

        let payload = qs_queue_ratchet
            .decrypt(qs_message_ciphertext)
            .map_err(|error| DecryptQsQueueMessageError::Decrypt {
                error,
                ratchet_seq_nr,
                message_seq_nr,
            })?;
        qs_queue_ratchet.update(txn, QueueType::Qs).await?;

        Ok(Some(payload))
    }

    pub(crate) async fn load(connection: impl ReadConnection) -> sqlx::Result<Self> {
        StorableQueueRatchet::load_internal(connection, QueueType::Qs).await
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
    async fn store(&self, mut connection: impl WriteConnection) -> sqlx::Result<()> {
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
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }

    async fn load_internal(
        mut connection: impl ReadConnection,
        queue_type: QueueType,
    ) -> sqlx::Result<Self> {
        let queue_ratchet = query_scalar!(
            r#"SELECT
                queue_ratchet AS "queue_ratchet: _"
            FROM queue_ratchet WHERE queue_type = ?"#,
            queue_type
        )
        .fetch_one(connection.as_mut())
        .await?;
        Ok(Self {
            queue_type,
            queue_ratchet,
        })
    }

    async fn update(
        &self,
        mut connection: impl WriteConnection,
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
        .execute(connection.as_mut())
        .await?;
        Ok(())
    }
}
