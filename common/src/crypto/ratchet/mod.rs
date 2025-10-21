// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use errors::{DecryptionError, EncryptionError};

use super::{errors::RandomnessError, *};

#[cfg(test)]
mod tests;

pub trait RatchetPayload<CT>:
    EarEncryptable<RatchetKey, CT> + EarDecryptable<RatchetKey, CT>
{
}

impl<CT, T> RatchetPayload<CT> for T where
    T: EarEncryptable<RatchetKey, CT> + EarDecryptable<RatchetKey, CT>
{
}

// WARNING: If this struct is changed its implementation of ToSql and FromSql in the sqlite module
// must be updated and a new `QueueRatchetVersion` introduced.
#[derive(Serialize, Deserialize, Clone, Debug, TlsSerialize, TlsDeserializeBytes, TlsSize)]
pub struct QueueRatchet<CT, Payload: RatchetPayload<CT>> {
    sequence_number: u64,
    secret: RatchetSecret,
    key: RatchetKey,
    _phantom: PhantomData<(CT, Payload)>,
}

#[cfg(feature = "test_utils")]
impl<CT, Payload: RatchetPayload<CT>> PartialEq for QueueRatchet<CT, Payload> {
    fn eq(&self, other: &Self) -> bool {
        self.sequence_number == other.sequence_number
            && self.secret == other.secret
            && self.key == other.key
            && self._phantom == other._phantom
    }
}

#[cfg(feature = "test_utils")]
impl<CT, Payload: RatchetPayload<CT>> Eq for QueueRatchet<CT, Payload> {}

impl<CT, Payload: RatchetPayload<CT>> TryFrom<RatchetSecret> for QueueRatchet<CT, Payload> {
    type Error = LibraryError;

    fn try_from(secret: RatchetSecret) -> Result<Self, Self::Error> {
        let key = RatchetKey::derive(&secret, &Vec::new()).map_err(|_| LibraryError)?;
        Ok(Self {
            sequence_number: 0,
            secret,
            key,
            _phantom: PhantomData,
        })
    }
}

// TODO: Implement the ratchet key.
impl<CT, Payload: RatchetPayload<CT>> QueueRatchet<CT, Payload> {
    /// Initialize a new ratchet key.
    pub fn random() -> Result<Self, RandomnessError> {
        let secret = RatchetSecret::random()?;
        secret
            .try_into()
            .map_err(|_| RandomnessError::InsufficientRandomness)
    }

    /// Advance the ratchet by one step. This should be called from the outside
    /// if messages are irrecoverably lost and the ratchet needs to be advanced
    /// to catch up.
    pub fn ratchet_forward(&mut self) -> Result<(), LibraryError> {
        let secret = RatchetSecret::derive(&self.secret, &Vec::new())?;

        let key = RatchetKey::derive(&secret, &Vec::new())?;

        self.secret = secret;
        self.key = key;
        self.sequence_number += 1;

        Ok(())
    }

    /// Encrypt the given payload.
    pub fn encrypt(&mut self, payload: &Payload) -> Result<QueueMessage, EncryptionError> {
        // TODO: We want domain separation: FQDN, UserID & ClientID.
        let ciphertext = payload.encrypt(&self.key)?.into();

        let queue_message = QueueMessage {
            sequence_number: self.sequence_number,
            ciphertext,
        };

        self.ratchet_forward()?;

        Ok(queue_message)
    }

    /// Decrypt the given payload.
    pub fn decrypt(&mut self, queue_message: QueueMessage) -> Result<Payload, DecryptionError> {
        let plaintext = Payload::decrypt(&self.key, &queue_message.ciphertext.into())?;
        self.ratchet_forward()?;
        Ok(plaintext)
    }

    /// Sample some fresh entropy and inject it into the current key. Returns the entropy.
    pub fn update(&mut self) -> RatchetKeyUpdate {
        todo!()
    }

    /// Get the current sequence number
    pub fn sequence_number(&self) -> u64 {
        self.sequence_number
    }

    pub fn secret(&self) -> &RatchetSecret {
        &self.secret
    }

    pub fn key(&self) -> &RatchetKey {
        &self.key
    }
}

mod sqlite {
    use sqlx::{Database, Decode, Encode, Sqlite, Type, encode::IsNull, error::BoxDynError};

    use crate::codec::PersistenceCodec;

    use super::*;

    // When adding a variant to this enum, the new variant must be called
    // `CurrentVersion` and the current version must be renamed to `VX`, where `X`
    // is the next version number. The content type of the old `CurrentVersion` must
    // be renamed and otherwise preserved to ensure backwards compatibility.
    #[derive(Serialize, Deserialize)]
    enum VersionedQueueRatchet {
        CurrentVersion(#[serde(with = "serde_bytes")] Vec<u8>),
    }

    impl<CT, Payload: RatchetPayload<CT>> Type<Sqlite> for QueueRatchet<CT, Payload> {
        fn type_info() -> <Sqlite as Database>::TypeInfo {
            <Vec<u8> as Type<Sqlite>>::type_info()
        }
    }

    impl<CT, Payload: RatchetPayload<CT>> Encode<'_, Sqlite> for QueueRatchet<CT, Payload> {
        fn encode_by_ref(
            &self,
            buf: &mut <Sqlite as Database>::ArgumentBuffer<'_>,
        ) -> Result<IsNull, BoxDynError> {
            let ratchet_bytes = PersistenceCodec::to_vec(self)?;
            let versioned_ratchet_bytes =
                PersistenceCodec::to_vec(&VersionedQueueRatchet::CurrentVersion(ratchet_bytes))?;
            Encode::<Sqlite>::encode(versioned_ratchet_bytes, buf)
        }
    }

    impl<CT, Payload: RatchetPayload<CT>> Decode<'_, Sqlite> for QueueRatchet<CT, Payload> {
        fn decode(value: <Sqlite as Database>::ValueRef<'_>) -> Result<Self, BoxDynError> {
            let bytes: &[u8] = Decode::<Sqlite>::decode(value)?;
            let VersionedQueueRatchet::CurrentVersion(ratchet_bytes) =
                PersistenceCodec::from_slice(bytes)?;
            let ratchet = PersistenceCodec::from_slice(&ratchet_bytes)?;
            Ok(ratchet)
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{
        codec::PersistenceCodec,
        crypto::secrets::Secret,
        messages::{EncryptedQsQueueMessageCtype, client_ds::QsQueueMessagePayload},
    };

    use super::*;

    // Note: the type parameters are not important
    fn queue_ratchet() -> QueueRatchet<EncryptedQsQueueMessageCtype, QsQueueMessagePayload> {
        let secret: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz012345";
        let ratchet_secret = RatchetSecret::from(Secret::from(*secret));
        QueueRatchet::try_from(ratchet_secret).unwrap()
    }

    #[test]
    fn test_queue_ratchet_serde_codec() {
        insta::assert_binary_snapshot!(
            ".cbor",
            PersistenceCodec::to_vec(&queue_ratchet()).unwrap()
        );
    }

    #[test]
    fn test_queue_ratchet_serde_json() {
        insta::assert_json_snapshot!(queue_ratchet());
    }
}
