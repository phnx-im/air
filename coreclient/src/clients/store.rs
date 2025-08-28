// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    crypto::{
        ear::{Ciphertext, EarDecryptable, keys::DatabaseKek},
        errors::RandomnessError,
        secrets::Secret,
    },
    messages::push_token::PushToken,
};
use anyhow::bail;

use super::{
    create_user::{
        AsRegisteredUserState, BasicUserData, PersistedUserState, PostAsRegistrationState,
        QsRegisteredUserState, UnfinalizedRegistrationState,
    },
    *,
};

/// WARNING: This enum is stored in sqlite as a blob. If any changes are made to
/// this enum, a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) enum UserCreationState {
    BasicUserData(BasicUserData),
    InitialUserState(InitialUserState),
    PostRegistrationInitState(PostAsRegistrationState),
    UnfinalizedRegistrationState(UnfinalizedRegistrationState),
    AsRegisteredUserState(AsRegisteredUserState),
    QsRegisteredUserState(QsRegisteredUserState),
    FinalUserState(PersistedUserState),
}

impl UserCreationState {
    pub(super) fn user_id(&self) -> &UserId {
        match self {
            Self::BasicUserData(state) => state.user_id(),
            Self::InitialUserState(state) => state.user_id(),
            Self::PostRegistrationInitState(state) => state.user_id(),
            Self::UnfinalizedRegistrationState(state) => state.user_id(),
            Self::AsRegisteredUserState(state) => state.user_id(),
            Self::QsRegisteredUserState(state) => state.user_id(),
            Self::FinalUserState(state) => state.user_id(),
        }
    }

    pub(super) fn server_url(&self) -> &str {
        match self {
            Self::BasicUserData(state) => state.server_url(),
            Self::InitialUserState(state) => state.server_url(),
            Self::PostRegistrationInitState(state) => state.server_url(),
            Self::UnfinalizedRegistrationState(state) => state.server_url(),
            Self::AsRegisteredUserState(state) => state.server_url(),
            Self::QsRegisteredUserState(state) => state.server_url(),
            Self::FinalUserState(state) => state.server_url(),
        }
    }

    pub(super) async fn new(
        client_db: &SqlitePool,
        user_id: UserId,
        server_url: impl ToString,
        push_token: Option<PushToken>,
    ) -> Result<Self> {
        let basic_user_data = BasicUserData {
            user_id: user_id.clone(),
            server_url: server_url.to_string(),
            push_token,
        };

        let user_creation_state = UserCreationState::BasicUserData(basic_user_data);

        user_creation_state.store(client_db).await?;

        Ok(user_creation_state)
    }

    pub(super) async fn step(
        self,
        air_db: &SqlitePool,
        client_db: &SqlitePool,
        api_clients: &ApiClients,
    ) -> Result<Self> {
        // If we're already in the final state, there is nothing to do.
        if matches!(self, UserCreationState::FinalUserState(_)) {
            return Ok(self);
        }

        let new_state = match self {
            UserCreationState::BasicUserData(state) => {
                let state = state
                    .prepare_as_registration(client_db, api_clients)
                    .await?;
                Self::InitialUserState(state)
            }
            UserCreationState::InitialUserState(state) => {
                Self::PostRegistrationInitState(state.as_registration(api_clients).await?)
            }
            UserCreationState::PostRegistrationInitState(state) => {
                let state = state.process_server_response(client_db).await?;
                Self::UnfinalizedRegistrationState(state)
            }
            UserCreationState::UnfinalizedRegistrationState(state) => {
                Self::AsRegisteredUserState(state.noop())
            }
            UserCreationState::AsRegisteredUserState(state) => {
                Self::QsRegisteredUserState(state.register_with_qs(api_clients).await?)
            }
            UserCreationState::QsRegisteredUserState(state) => {
                let persisted_user_state =
                    state.upload_key_packages(client_db, api_clients).await?;
                Self::FinalUserState(persisted_user_state)
            }
            UserCreationState::FinalUserState(_) => self,
        };

        new_state.store(client_db).await?;

        // If we just transitioned into the final state, we need to update the
        // client record.
        if let UserCreationState::FinalUserState(_) = new_state {
            let mut client_record = ClientRecord::load(air_db, new_state.user_id())
                .await?
                .ok_or(anyhow!("Client record not found"))?;
            client_record.finish();
            client_record.store(air_db).await?;
        }

        Ok(new_state)
    }

    pub(super) fn final_state(self) -> Result<PersistedUserState> {
        if let UserCreationState::FinalUserState(state) = self {
            Ok(state)
        } else {
            bail!("Unexpected user creation state")
        }
    }

    /// A convenience function that performs the `step` function until the final state is reached.
    pub(super) async fn complete_user_creation(
        mut self,
        air_db: &SqlitePool,
        client_db: &SqlitePool,
        api_clients: &ApiClients,
    ) -> Result<PersistedUserState> {
        while !matches!(self, UserCreationState::FinalUserState(_)) {
            self = self.step(air_db, client_db, api_clients).await?
        }

        self.final_state()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientRecordState {
    InProgress,
    Finished,
}

impl ClientRecordState {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            ClientRecordState::InProgress => "in_progress",
            ClientRecordState::Finished => "finished",
        }
    }

    pub(crate) fn from_str(state: &str) -> Option<Self> {
        match state {
            "in_progress" => Some(ClientRecordState::InProgress),
            "finished" => Some(ClientRecordState::Finished),
            _ => None,
        }
    }
}

const DATABASE_DEK_LENGTH: usize = 32;
#[derive(Debug)]
pub struct EncryptedDatabaseDekCtype;
pub type EncryptedDek = Ciphertext<EncryptedDatabaseDekCtype>;

impl EarEncryptable<DatabaseKek, EncryptedDatabaseDekCtype> for DatabaseEncryptionKey {}
impl EarDecryptable<DatabaseKek, EncryptedDatabaseDekCtype> for DatabaseEncryptionKey {}

#[derive(Serialize, Deserialize)]
pub struct DatabaseEncryptionKey(Secret<DATABASE_DEK_LENGTH>);

impl DatabaseEncryptionKey {
    pub(crate) fn random() -> Result<Self, RandomnessError> {
        let secret = Secret::random()?;
        Ok(Self(secret))
    }

    pub(crate) fn to_hex_string(&self) -> String {
        hex::encode(&self.0.secret())
    }
}

impl From<Secret<DATABASE_DEK_LENGTH>> for DatabaseEncryptionKey {
    fn from(secret: Secret<DATABASE_DEK_LENGTH>) -> Self {
        Self(secret)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientRecord {
    pub user_id: UserId,
    pub client_record_state: ClientRecordState,
    pub created_at: DateTime<Utc>,
    pub is_default: bool,
    pub encrypted_dek: EncryptedDek,
}

impl ClientRecord {
    pub(super) fn new(
        user_id: UserId,
        kek: &aircommon::crypto::ear::keys::DatabaseKek,
    ) -> anyhow::Result<Self> {
        // Generate new DEK and encrypt it
        let dek = DatabaseEncryptionKey::random()?;

        let encrypted_dek = kek
            .encrypt(GenericSerializable::serialize(&dek)?.as_slice())
            .map_err(|e| anyhow::anyhow!("Failed to encrypt DEK: {}", e))?;

        Ok(Self {
            user_id,
            client_record_state: ClientRecordState::InProgress,
            created_at: Utc::now(),
            is_default: false,
            encrypted_dek: encrypted_dek.into(),
        })
    }

    pub(super) fn finish(&mut self) {
        self.client_record_state = ClientRecordState::Finished;
    }

    /// Get the encrypted DEK
    pub fn encrypted_dek(&self) -> &EncryptedDek {
        &self.encrypted_dek
    }

    /// Decrypt the DEK using the provided KEK
    fn decrypt_dek(&self, kek: &DatabaseKek) -> anyhow::Result<DatabaseEncryptionKey> {
        let bytes = kek
            .decrypt(&self.encrypted_dek.aead_ciphertext())
            .map_err(|e| anyhow::anyhow!("Failed to decrypt DEK: {}", e))?;
        let slice: [u8; DATABASE_DEK_LENGTH] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| anyhow::anyhow!("Decrypted DEK has invalid length"))?;
        let secret = Secret::<DATABASE_DEK_LENGTH>::try_from(slice)?;

        Ok(secret.into())
    }

    /// Open the client database using the encrypted DEK and provided KEK
    pub async fn open_client_db(
        &self,
        client_db_path: &str,
        kek: &DatabaseKek,
    ) -> anyhow::Result<sqlx::SqlitePool> {
        let dek = self.decrypt_dek(kek)?;
        crate::utils::persistence::open_client_db(&self.user_id, client_db_path, &dek).await
    }
}
