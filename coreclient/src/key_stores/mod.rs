// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, ops::Deref};

use aircommon::{
    crypto::hpke::{ClientIdEncryptionKey, HpkeEncryptable},
    identifiers::{ClientConfig, QsClientId, QsReference},
    mls_group_config::{QS_CLIENT_REFERENCE_EXTENSION_TYPE, default_capabilities},
};
use anyhow::Result;
use openmls::prelude::{
    CredentialWithKey, Extension, Extensions, KeyPackage, LastResortExtension, SignaturePublicKey,
    UnknownExtension,
};
use sqlx::SqlitePool;
use tls_codec::Serialize as TlsSerializeTrait;

use crate::{
    clients::{CIPHERSUITE, api_clients::ApiClients},
    groups::openmls_provider::AirOpenMlsProvider,
};

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{
        RatchetDecryptionKey,
        ear::keys::{PushTokenEarKey, WelcomeAttributionInfoEarKey},
        signatures::keys::{QsClientSigningKey, QsUserSigningKey},
    },
    messages::FriendshipToken,
};
use serde::{Deserialize, Serialize};

pub(crate) mod as_credentials;
pub(crate) mod indexed_keys;
pub(crate) mod queue_ratchets;

// For now we persist the key store along with the user. Any key material that gets rotated in the future needs to be persisted separately.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct MemoryUserKeyStoreBase<K> {
    // Client credential secret key
    pub(super) signing_key: K,
    // QS-specific key material
    pub(super) qs_client_signing_key: QsClientSigningKey,
    pub(super) qs_user_signing_key: QsUserSigningKey,
    pub(super) qs_queue_decryption_key: RatchetDecryptionKey,
    pub(super) qs_client_id_encryption_key: ClientIdEncryptionKey,
    pub(super) push_token_ear_key: PushTokenEarKey,
    // These are keys that we send to our contacts
    pub(super) friendship_token: FriendshipToken,
    pub(super) wai_ear_key: WelcomeAttributionInfoEarKey,
}

pub(crate) type MemoryUserKeyStore = MemoryUserKeyStoreBase<ClientSigningKey>;

impl<K> fmt::Debug for MemoryUserKeyStoreBase<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryUserKeyStore").finish_non_exhaustive()
    }
}

impl MemoryUserKeyStore {
    pub(crate) fn create_own_client_reference(&self, qs_client_id: &QsClientId) -> QsReference {
        let sealed_reference = ClientConfig {
            client_id: *qs_client_id,
            push_token_ear_key: Some(self.push_token_ear_key.clone()),
        }
        .encrypt(&self.qs_client_id_encryption_key, &[], &[]);
        QsReference {
            client_homeserver_domain: self.signing_key.credential().identity().domain().clone(),
            sealed_reference,
        }
    }

    pub(crate) async fn generate_key_package(
        &self,
        pool: &SqlitePool,
        qs_client_id: &QsClientId,
        last_resort: bool,
    ) -> Result<KeyPackage> {
        let credential_with_key = CredentialWithKey {
            credential: self.signing_key.credential().try_into()?,
            signature_key: SignaturePublicKey::from(
                self.signing_key.credential().verifying_key().clone(),
            ),
        };
        let capabilities = default_capabilities();
        let client_reference = self.create_own_client_reference(qs_client_id);
        let client_ref_extension = Extension::Unknown(
            QS_CLIENT_REFERENCE_EXTENSION_TYPE,
            UnknownExtension(client_reference.tls_serialize_detached()?),
        );
        let leaf_node_extensions = Extensions::single(client_ref_extension);
        let key_package_extensions = if last_resort {
            let last_resort_extension = Extension::LastResort(LastResortExtension::new());
            Extensions::single(last_resort_extension)
        } else {
            Extensions::default()
        };

        let mut connection = pool.acquire().await?;
        let provider = AirOpenMlsProvider::new(&mut connection);

        let kp = KeyPackage::builder()
            .key_package_extensions(key_package_extensions)
            .leaf_node_capabilities(capabilities)
            .leaf_node_extensions(leaf_node_extensions)
            .build(
                CIPHERSUITE,
                &provider,
                &self.signing_key,
                credential_with_key,
            )?;

        Ok(kp.key_package().clone())
    }
}
