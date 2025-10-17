// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::{
    DisplayName,
    groups::client_auth_info::StorableClientCredential,
    key_stores::{
        MemoryUserKeyStoreBase, as_credentials::AsCredentials, indexed_keys::StorableIndexedKey,
        queue_ratchets::StorableQsQueueRatchet,
    },
    outbound_service,
    user_profiles::generate::NewUserProfile,
};
use aircommon::{
    credentials::{
        AsIntermediateCredential, VerifiableClientCredential, keys::PreliminaryClientSigningKey,
    },
    crypto::{
        indexed_aead::{ciphertexts::IndexEncryptable, keys::UserProfileKey},
        signatures::{DEFAULT_SIGNATURE_SCHEME, signable::Verifiable},
    },
    messages::{
        client_as_out::EncryptedUserProfile,
        client_qs::CreateUserRecordResponse,
        connection_package_v1::ConnectionPackageV1,
        push_token::{EncryptedPushToken, PushToken},
    },
};
use tracing::debug;

use super::*;

/// State before any network queries
///
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct BasicUserData {
    pub(super) user_id: UserId,
    pub(super) server_url: String,
    pub(super) push_token: Option<PushToken>,
}

impl BasicUserData {
    pub(super) fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub(super) fn server_url(&self) -> &str {
        &self.server_url
    }

    pub(super) async fn prepare_as_registration(
        self,
        pool: &SqlitePool,
        api_clients: &ApiClients,
    ) -> Result<InitialUserState> {
        // Prepare user account creation
        debug!(?self.user_id, "Creating new client");
        // Let's turn TLS off for now.
        let domain = self.user_id.domain();
        // Fetch credentials from AS
        let as_intermediate_credential =
            AsCredentials::get_intermediate_credential(pool, api_clients, domain).await?;

        // We already fetch the QS encryption key here, so we don't have to do
        // it in a later step, where we otherwise don't have to perform network
        // queries and failing here means we can just start from the beginning
        // next time without incurring any costs.
        let qs_encryption_key = api_clients
            .default_client()?
            .qs_encryption_key()
            .await?
            .encryption_key;

        // Create CSR for AS to sign
        let (client_credential_csr, prelim_signing_key) =
            ClientCredentialCsr::new(self.user_id.clone(), DEFAULT_SIGNATURE_SCHEME)?;

        let client_credential_payload = ClientCredentialPayload::new(
            client_credential_csr,
            None,
            *as_intermediate_credential.fingerprint(),
        );

        let qs_initial_ratchet_secret = RatchetSecret::random()?;
        StorableQsQueueRatchet::initialize(pool, qs_initial_ratchet_secret.clone()).await?;
        let qs_queue_decryption_key = RatchetDecryptionKey::generate()?;
        let qs_client_signing_key = QsClientSigningKey::generate()?;
        let qs_user_signing_key = QsUserSigningKey::generate()?;

        // TODO: The following keys should be derived from a single
        // friendship key. Once that's done, remove the random constructors.
        let friendship_token = FriendshipToken::random()?;
        let wai_ear_key: WelcomeAttributionInfoEarKey = WelcomeAttributionInfoEarKey::random()?;
        let push_token_ear_key = PushTokenEarKey::random()?;

        let key_store = MemoryUserKeyStoreBase {
            signing_key: prelim_signing_key,
            qs_client_signing_key,
            qs_user_signing_key,
            qs_queue_decryption_key,
            push_token_ear_key,
            friendship_token,
            wai_ear_key,
            qs_client_id_encryption_key: qs_encryption_key,
        };

        let encrypted_push_token = match self.push_token {
            Some(push_token) => Some(push_token.encrypt(&key_store.push_token_ear_key)?),
            None => None,
        };

        let user_profile_key = UserProfileKey::random(&self.user_id)?;

        let mut connection = pool.acquire().await?;
        user_profile_key.store_own(connection.as_mut()).await?;

        let encrypted_user_profile = NewUserProfile::new(
            &key_store.signing_key,
            self.user_id.clone(),
            user_profile_key.index().clone(),
            DisplayName::from_user_id(&self.user_id),
            None,
        )?
        .store(connection.as_mut(), &mut StoreNotifier::noop())
        .await?
        .encrypt_with_index(&user_profile_key)?;

        let initial_user_state = InitialUserState {
            client_credential_payload: client_credential_payload.clone(),
            server_url: self.server_url,
            as_intermediate_credential,
            encrypted_push_token,
            encrypted_user_profile,
            key_store,
            qs_initial_ratchet_secret,
        };

        Ok(initial_user_state)
    }
}

// State pre-AS Registration
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct InitialUserState {
    client_credential_payload: ClientCredentialPayload,
    server_url: String,
    as_intermediate_credential: AsIntermediateCredential,
    encrypted_push_token: Option<EncryptedPushToken>,
    encrypted_user_profile: EncryptedUserProfile,
    key_store: MemoryUserKeyStoreBase<PreliminaryClientSigningKey>,
    qs_initial_ratchet_secret: RatchetSecret,
}

impl InitialUserState {
    #[expect(clippy::wrong_self_convention)]
    pub(super) async fn as_registration(
        self,
        api_clients: &ApiClients,
    ) -> Result<PostAsRegistrationState> {
        // Register the user with the backend.
        let response = api_clients
            .default_client()?
            .as_register_user(
                self.client_credential_payload.clone(),
                self.encrypted_user_profile.clone(),
            )
            .await?;

        let post_registration_init_state = PostAsRegistrationState {
            initial_user_state: self,
            client_credential: response.client_credential,
        };

        Ok(post_registration_init_state)
    }

    pub(super) fn user_id(&self) -> &UserId {
        self.client_credential_payload.identity()
    }

    pub(super) fn server_url(&self) -> &str {
        &self.server_url
    }
}

// State after server response to OPAKE initialization
//
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct PostAsRegistrationState {
    initial_user_state: InitialUserState,
    client_credential: VerifiableClientCredential,
}

impl PostAsRegistrationState {
    pub(super) async fn process_server_response(
        self,
        pool: &SqlitePool,
    ) -> Result<UnfinalizedRegistrationState> {
        let InitialUserState {
            client_credential_payload: _,
            server_url,
            as_intermediate_credential,
            encrypted_push_token,
            encrypted_user_profile: _,
            key_store,
            qs_initial_ratchet_secret,
        } = self.initial_user_state;

        let client_credential: ClientCredential = self
            .client_credential
            .verify(as_intermediate_credential.verifying_key())?;
        StorableClientCredential::new(client_credential.clone())
            .store(pool)
            .await?;

        let signing_key =
            ClientSigningKey::from_prelim_key(key_store.signing_key, client_credential.clone())?;

        // Store the own client credential in the DB
        StorableClientCredential::new(client_credential.clone())
            .store(pool)
            .await?;

        // Replace preliminary signing key in the key store
        let key_store = MemoryUserKeyStore {
            signing_key,
            qs_client_signing_key: key_store.qs_client_signing_key,
            qs_user_signing_key: key_store.qs_user_signing_key,
            qs_queue_decryption_key: key_store.qs_queue_decryption_key,
            push_token_ear_key: key_store.push_token_ear_key,
            friendship_token: key_store.friendship_token,
            wai_ear_key: key_store.wai_ear_key,
            qs_client_id_encryption_key: key_store.qs_client_id_encryption_key,
        };

        let unfinalized_registration_state = UnfinalizedRegistrationState {
            key_store,
            server_url,
            qs_initial_ratchet_secret,
            connection_packages: Vec::new(),
            encrypted_push_token,
        };

        Ok(unfinalized_registration_state)
    }

    pub(super) fn user_id(&self) -> &UserId {
        self.client_credential.user_id()
    }

    pub(super) fn server_url(&self) -> &str {
        &self.initial_user_state.server_url
    }
}

// State after server response to OPAKE initialization
//
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct UnfinalizedRegistrationState {
    key_store: MemoryUserKeyStore,
    server_url: String,
    qs_initial_ratchet_secret: RatchetSecret,
    connection_packages: Vec<ConnectionPackageV1>,
    encrypted_push_token: Option<EncryptedPushToken>,
}

impl UnfinalizedRegistrationState {
    // Previously, this published connection packages. Now, these are published on user handle
    // creation.
    pub(super) fn noop(self) -> AsRegisteredUserState {
        let UnfinalizedRegistrationState {
            key_store,
            server_url,
            qs_initial_ratchet_secret,
            connection_packages: _,
            encrypted_push_token,
        } = self;
        AsRegisteredUserState {
            key_store,
            server_url,
            qs_initial_ratchet_secret,
            encrypted_push_token,
        }
    }

    pub(super) fn user_id(&self) -> &UserId {
        self.key_store.signing_key.credential().identity()
    }

    pub(super) fn server_url(&self) -> &str {
        &self.server_url
    }
}

// State after querying finish user registration
//
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct AsRegisteredUserState {
    key_store: MemoryUserKeyStore,
    server_url: String,
    qs_initial_ratchet_secret: RatchetSecret,
    encrypted_push_token: Option<EncryptedPushToken>,
}

impl AsRegisteredUserState {
    pub(super) async fn register_with_qs(
        self,
        api_clients: &ApiClients,
    ) -> Result<QsRegisteredUserState> {
        let AsRegisteredUserState {
            key_store,
            server_url,
            qs_initial_ratchet_secret,
            encrypted_push_token,
        } = self;

        let CreateUserRecordResponse {
            user_id,
            qs_client_id: client_id,
        } = api_clients
            .default_client()?
            .qs_create_user(
                key_store.friendship_token.clone(),
                key_store.qs_client_signing_key.verifying_key().clone(),
                key_store.qs_queue_decryption_key.encryption_key().clone(),
                encrypted_push_token,
                qs_initial_ratchet_secret,
                &key_store.qs_user_signing_key,
            )
            .await?;

        let qs_registered_user_state = QsRegisteredUserState {
            key_store,
            server_url,
            qs_user_id: user_id,
            qs_client_id: client_id,
        };

        Ok(qs_registered_user_state)
    }

    pub(super) fn user_id(&self) -> &UserId {
        self.key_store.signing_key.credential().identity()
    }

    pub(super) fn server_url(&self) -> &str {
        &self.server_url
    }
}

// State after creating QS user
//
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct QsRegisteredUserState {
    key_store: MemoryUserKeyStore,
    server_url: String,
    qs_user_id: QsUserId,
    qs_client_id: QsClientId,
}

impl QsRegisteredUserState {
    pub(super) async fn upload_key_packages(
        self,
        pool: &SqlitePool,
        api_clients: &ApiClients,
    ) -> Result<PersistedUserState> {
        let QsRegisteredUserState {
            ref key_store,
            server_url: _,
            qs_user_id: _,
            ref qs_client_id,
        } = self;

        let mut qs_key_packages = vec![];
        for _ in 0..KEY_PACKAGES {
            let key_package = key_store
                .generate_key_package(pool, qs_client_id, false)
                .await?;
            qs_key_packages.push(key_package);
        }
        let last_resort_key_package = key_store
            .generate_key_package(pool, qs_client_id, true)
            .await?;
        qs_key_packages.push(last_resort_key_package);

        // Upload add packages
        api_clients
            .default_client()?
            .qs_publish_key_packages(
                *qs_client_id,
                qs_key_packages,
                &key_store.qs_client_signing_key,
            )
            .await?;

        let state = PersistedUserState { state: self };

        Ok(state)
    }

    pub(super) fn user_id(&self) -> &UserId {
        self.key_store.signing_key.credential().identity()
    }

    pub(super) fn server_url(&self) -> &str {
        &self.server_url
    }
}

// State after creating QS user
//
// WARNING: This type is stored in sqlite as a blob. If any changes are made
// a new version in `StorableUserCreationState` must be created.
#[derive(Serialize, Deserialize)]
pub(crate) struct PersistedUserState {
    state: QsRegisteredUserState,
}

impl PersistedUserState {
    pub(super) fn into_self_user(self, pool: SqlitePool, api_clients: ApiClients) -> CoreUser {
        let QsRegisteredUserState {
            key_store,
            server_url: _,
            qs_user_id,
            qs_client_id,
        } = self.state;
        let store_notifications_tx = StoreNotificationsSender::new();
        let outbound_service = outbound_service::OutboundService::new(
            pool.clone(),
            api_clients.clone(),
            key_store.signing_key.clone(),
            store_notifications_tx.clone(),
        );
        let inner = Arc::new(CoreUserInner {
            pool,
            key_store,
            qs_user_id,
            qs_client_id,
            api_clients: api_clients.clone(),
            http_client: reqwest::Client::new(),
            store_notifications_tx,
            outbound_service,
        });
        CoreUser { inner }
    }

    pub(super) fn user_id(&self) -> &UserId {
        self.state.key_store.signing_key.credential().identity()
    }

    pub(super) fn server_url(&self) -> &str {
        &self.state.server_url
    }

    pub(super) fn qs_user_id(&self) -> &QsUserId {
        &self.state.qs_user_id
    }

    pub(super) fn qs_client_id(&self) -> &QsClientId {
        &self.state.qs_client_id
    }
}
