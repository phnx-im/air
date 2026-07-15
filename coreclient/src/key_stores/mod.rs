// SPDX-FileCopyrightText: 2023 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fmt, iter::once, ops::Deref};

use aircommon::{
    crypto::hpke::{ClientIdEncryptionKey, HpkeEncryptable},
    identifiers::{ClientConfig, QsClientId, QsReference},
    mls_group_config::{
        APQ_CIPHERSUITE, QS_CLIENT_REFERENCE_EXTENSION_TYPE, default_key_package_extensions,
        default_leaf_node_capabilities, default_leaf_node_extensions, vc_leaf_node_extensions,
    },
};
use airprotos::client::component::AirComponent;
use anyhow::{Context, Result, ensure};
use apqmls::{
    authentication::ApqCredentialWithKey, key_package::ApqKeyPackageBuilder,
    messages::ApqKeyPackage,
};
use openmls::{
    components::vc_derivation_info::{EpochId, KeyPackageInfo},
    prelude::{
        Credential, CredentialType, CredentialWithKey, Extension, KeyPackage, KeyPackageBuilder,
        KeyPackageRef, LastResortExtension, OpenMlsProvider, SignaturePublicKey, UnknownExtension,
        VcKeyPackageBatchBuilder,
    },
};
use openmls_traits::storage::StorageProvider;
use tls_codec::Serialize as TlsSerializeTrait;

use crate::{
    clients::{CIPHERSUITE, api_clients::ApiClients},
    db::access::WriteConnection,
    groups::openmls_provider::AirOpenMlsProvider,
};

use aircommon::{
    credentials::keys::ClientSigningKey,
    crypto::{
        RatchetDecryptionKey,
        aead::keys::{PushTokenEarKey, WelcomeAttributionInfoEarKey},
        signatures::keys::{QsClientSigningKey, QsUserSigningKey},
    },
    messages::FriendshipToken,
};
use serde::{Deserialize, Serialize};

pub(crate) mod as_credentials;
pub(crate) mod indexed_keys;
pub(crate) mod key_package_refs;
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
            client_homeserver_domain: self.signing_key.credential().user_id().domain().clone(),
            sealed_reference,
        }
    }

    // pub(crate) fn generate_vc_key_package(
    //     &self,
    //     mut connection: impl WriteConnection,
    //     qs_client_id: &QsClientId,
    //     last_resort: bool,
    //     epoch_id: EpochId,
    //     count: u32,
    // ) -> Result<VcKeyPackageBatch> {
    //     let credential_with_key = CredentialWithKey {
    //         credential: self.signing_key.credential().try_into()?,
    //         signature_key: SignaturePublicKey::from(
    //             self.signing_key.credential().verifying_key().clone(),
    //         ),
    //     };
    //
    //     let mut leaf_node_extensions = vc_leaf_node_extensions::<AirComponent>();
    //
    //     let client_reference = self.create_own_client_reference(qs_client_id);
    //     let client_ref_extension = Extension::Unknown(
    //         QS_CLIENT_REFERENCE_EXTENSION_TYPE,
    //         UnknownExtension(client_reference.tls_serialize_detached()?),
    //     );
    //     leaf_node_extensions.add(client_ref_extension)?;
    //
    //     let mut key_package_extensions = default_key_package_extensions::<AirComponent>();
    //     if last_resort {
    //         let last_resort_extension = Extension::LastResort(LastResortExtension::new());
    //         key_package_extensions.add(last_resort_extension)?;
    //     };
    //
    //     let provider = AirOpenMlsProvider::new(connection.as_mut());
    //
    //     let vc_batch = KeyPackage::builder()
    //         .key_package_extensions(key_package_extensions)
    //         .leaf_node_capabilities(default_leaf_node_capabilities())
    //         .leaf_node_extensions(leaf_node_extensions)
    //         .build_vc_batch(
    //             CIPHERSUITE,
    //             &provider,
    //             &self.signing_key,
    //             credential_with_key,
    //             epoch_id,
    //             count,
    //         )?;
    //     Ok(vc_batch)
    // }

    fn key_package_builder(
        &self,
        qs_client_id: &QsClientId,
        last_resort: bool,
        virtual_client: bool,
    ) -> Result<KeyPackageBuilder> {
        let mut leaf_node_extensions = if virtual_client {
            vc_leaf_node_extensions::<AirComponent>()
        } else {
            default_leaf_node_extensions::<AirComponent>()
        };

        let client_reference = self.create_own_client_reference(qs_client_id);
        let client_ref_extension = Extension::Unknown(
            QS_CLIENT_REFERENCE_EXTENSION_TYPE,
            UnknownExtension(client_reference.tls_serialize_detached()?),
        );
        leaf_node_extensions.add(client_ref_extension)?;

        let key_package_extensions = if last_resort {
            let mut extensions = default_key_package_extensions::<AirComponent>();
            let last_resort_extension = Extension::LastResort(LastResortExtension::new());
            extensions.add(last_resort_extension)?;
            extensions
        } else {
            default_key_package_extensions::<AirComponent>()
        };

        let builder = KeyPackage::builder()
            .key_package_extensions(key_package_extensions)
            .leaf_node_capabilities(default_leaf_node_capabilities())
            .leaf_node_extensions(leaf_node_extensions);
        Ok(builder)
    }

    pub(crate) fn generate_key_package(
        &self,
        mut connection: impl WriteConnection,
        qs_client_id: &QsClientId,
        last_resort: bool,
    ) -> Result<KeyPackage> {
        let credential_with_key = CredentialWithKey {
            credential: self.signing_key.credential().try_into()?,
            signature_key: SignaturePublicKey::from(
                self.signing_key.credential().verifying_key().clone(),
            ),
        };
        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let kp = self
            .key_package_builder(qs_client_id, last_resort, false)?
            .build(
                CIPHERSUITE,
                &provider,
                &self.signing_key,
                credential_with_key,
            )?;
        Ok(kp.into_key_package())
    }

    fn apq_key_package_builder(
        &self,
        qs_client_id: &QsClientId,
        last_resort: bool,
        virtual_client: bool,
    ) -> Result<ApqKeyPackageBuilder> {
        let mut leaf_node_extensions = if virtual_client {
            vc_leaf_node_extensions::<AirComponent>()
        } else {
            default_leaf_node_extensions::<AirComponent>()
        };

        let client_reference = self.create_own_client_reference(qs_client_id);
        let client_ref_extension = Extension::Unknown(
            QS_CLIENT_REFERENCE_EXTENSION_TYPE,
            UnknownExtension(client_reference.tls_serialize_detached()?),
        );
        leaf_node_extensions.add(client_ref_extension)?;

        let key_package_extensions = if last_resort {
            let mut extensions = default_key_package_extensions::<AirComponent>();
            let last_resort_extension = Extension::LastResort(LastResortExtension::new());
            extensions.add(last_resort_extension)?;
            extensions
        } else {
            default_key_package_extensions::<AirComponent>()
        };

        Ok(ApqKeyPackage::builder()
            .key_package_extensions(key_package_extensions)
            .leaf_node_capabilities(default_leaf_node_capabilities())
            .leaf_node_extensions(leaf_node_extensions))
    }

    pub(crate) fn generate_apq_key_package(
        &self,
        mut connection: impl WriteConnection,
        qs_client_id: &QsClientId,
        last_resort: bool,
    ) -> Result<ApqKeyPackage> {
        let t_credential = CredentialWithKey {
            credential: self.signing_key.credential().try_into()?,
            signature_key: SignaturePublicKey::from(
                self.signing_key.credential().verifying_key().clone(),
            ),
        };

        // Skip storing the same credential twice
        let pq_credential = CredentialWithKey {
            credential: Credential::new(CredentialType::Basic, Vec::new()),
            signature_key: SignaturePublicKey::from(
                self.signing_key.credential().verifying_key().clone(),
            ),
        };

        let apq_credential_with_key = ApqCredentialWithKey {
            t_credential,
            pq_credential,
        };

        let provider = AirOpenMlsProvider::new(connection.as_mut());
        let kp = self
            .apq_key_package_builder(qs_client_id, last_resort, false)?
            .build(
                &provider,
                APQ_CIPHERSUITE,
                &self.signing_key,
                apq_credential_with_key,
            )?;

        Ok(kp.into_key_package())
    }

    pub(crate) fn generate_vc_key_package_batch(
        &self,
        mut connection: impl WriteConnection,
        qs_client_id: &QsClientId,
        epoch_id: EpochId,
        config: VcKeyPackageBatchConfig,
    ) -> Result<HeterogeneousVcKeyPackageBatch> {
        let t_credential = CredentialWithKey {
            credential: self.signing_key.credential().try_into()?,
            signature_key: SignaturePublicKey::from(
                self.signing_key.credential().verifying_key().clone(),
            ),
        };

        // Skip storing the same credential twice
        let pq_credential = CredentialWithKey {
            credential: Credential::new(CredentialType::Basic, Vec::new()),
            signature_key: SignaturePublicKey::from(
                self.signing_key.credential().verifying_key().clone(),
            ),
        };

        let provider = AirOpenMlsProvider::new(connection.as_mut());

        let mut batch_builder =
            VcKeyPackageBatchBuilder::with_capacity(&provider, epoch_id, config.num_plain())?;

        for is_last_resort in (0..config.key_packages)
            .map(|_| false)
            .chain(once(config.last_resort.then_some(true)).flatten())
        {
            let kp_builder = self.key_package_builder(qs_client_id, is_last_resort, true)?;
            batch_builder.add_key_package(
                kp_builder,
                CIPHERSUITE,
                provider.crypto(),
                &self.signing_key,
                t_credential.clone(),
            )?;
        }

        for is_last_resort in (0..config.apq_key_packages)
            .map(|_| false)
            .chain(once(config.apq_last_resort.then_some(true)).flatten())
        {
            let kp_builder = self.apq_key_package_builder(qs_client_id, is_last_resort, true)?;
            let (t_kp_builder, pq_kp_builder) = kp_builder.split(APQ_CIPHERSUITE)?;
            batch_builder.add_key_package(
                t_kp_builder,
                APQ_CIPHERSUITE.t_ciphersuite(),
                provider.crypto(),
                &self.signing_key,
                t_credential.clone(),
            )?;
            batch_builder.add_key_package(
                pq_kp_builder,
                APQ_CIPHERSUITE.pq_ciphersuite(),
                provider.crypto(),
                &self.signing_key,
                pq_credential.clone(),
            )?;
        }

        let batch = batch_builder.finalize(&provider)?;

        let mut res = HeterogeneousVcKeyPackageBatch {
            generation: batch.generation,
            key_packages: Vec::with_capacity(config.num_plain()),
            apq_key_packages: Vec::with_capacity(config.num_apq()),
            infos: Vec::with_capacity(config.total()),
        };

        let mut iter = batch.key_packages.into_iter().fuse();
        for (full_kp, info) in iter.by_ref().take(config.num_plain()) {
            res.key_packages.push(full_kp.into_key_package());
            res.infos.push(info);
        }
        while let (Some((t_kp, t_info)), Some((pq_kp, pq_info))) = (iter.next(), iter.next()) {
            let apq_kp = ApqKeyPackage::new(t_kp.into_key_package(), pq_kp.into_key_package());
            res.apq_key_packages.push(apq_kp);
            res.infos.push(t_info);
            res.infos.push(pq_info);
        }

        Ok(res)
    }

    pub(crate) fn delete_key_package(
        &self,
        mut connection: impl WriteConnection,
        key_package_ref: KeyPackageRef,
    ) -> Result<()> {
        let provider = AirOpenMlsProvider::new(connection.as_mut());

        provider.storage().delete_key_package(&key_package_ref)?;
        Ok(())
    }
}

pub(crate) struct VcKeyPackageBatchConfig {
    pub(crate) key_packages: usize,
    pub(crate) last_resort: bool,
    pub(crate) apq_key_packages: usize,
    pub(crate) apq_last_resort: bool,
}

impl VcKeyPackageBatchConfig {
    fn total(&self) -> usize {
        self.num_plain() + 2 * self.num_apq()
    }

    fn num_plain(&self) -> usize {
        self.key_packages + usize::from(self.last_resort)
    }

    fn num_apq(&self) -> usize {
        self.apq_key_packages + usize::from(self.apq_last_resort)
    }
}

pub(crate) struct HeterogeneousVcKeyPackageBatch {
    pub(crate) generation: u32,
    pub(crate) key_packages: Vec<KeyPackage>,
    pub(crate) apq_key_packages: Vec<ApqKeyPackage>,
    pub(crate) infos: Vec<KeyPackageInfo>,
}

impl HeterogeneousVcKeyPackageBatch {
    pub(crate) fn split_vc_batch_refs(
        infos: &[KeyPackageInfo],
    ) -> anyhow::Result<(Vec<KeyPackageRef>, Vec<KeyPackageRef>)> {
        let mut infos: Vec<&KeyPackageInfo> = infos.iter().collect();
        // Don't trust the data from the DS
        infos.sort_unstable_by_key(|info| info.key_package_index);
        let plain_end = infos
            .iter()
            .position(|info| info.cipher_suite == APQ_CIPHERSUITE.pq_ciphersuite())
            .map(|index| index.checked_sub(1).context("APQ packages are not paired"))
            .transpose()?
            .unwrap_or(infos.len());
        ensure!(
            (infos.len() - plain_end).is_multiple_of(2),
            "APQ packages are not paired",
        );
        let plain_refs = infos[..plain_end]
            .iter()
            .map(|info| info.key_package_ref.clone())
            .collect();
        let apq_refs = infos[plain_end..]
            .iter()
            .map(|info| info.key_package_ref.clone())
            .collect();
        Ok((plain_refs, apq_refs))
    }
}

#[cfg(test)]
mod tests {
    use openmls::prelude::Ciphersuite;

    use super::*;

    fn info(index: u32, cipher_suite: Ciphersuite) -> KeyPackageInfo {
        KeyPackageInfo {
            key_package_ref: KeyPackageRef::from_slice(&[index as u8; 16]),
            cipher_suite,
            key_package_index: index,
        }
    }

    fn refs(indices: impl IntoIterator<Item = u32>) -> Vec<KeyPackageRef> {
        indices
            .into_iter()
            .map(|index| KeyPackageRef::from_slice(&[index as u8; 16]))
            .collect()
    }

    #[test]
    fn split_vc_batch_refs_splits_plain_block_and_apq_pairs() {
        let t = APQ_CIPHERSUITE.t_ciphersuite();
        let pq = APQ_CIPHERSUITE.pq_ciphersuite();
        // Batch layout: plain block (0..3), then adjacent T/PQ pairs. Shuffled
        // on the wire: the splitter must order by key_package_index.
        let infos = [
            info(3, t),
            info(6, pq),
            info(0, CIPHERSUITE),
            info(4, pq),
            info(2, CIPHERSUITE),
            info(5, t),
            info(1, CIPHERSUITE),
        ];
        let (plain, apq) = HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&infos).unwrap();
        assert_eq!(plain, refs(0..3));
        assert_eq!(apq, refs(3..7));
    }

    #[test]
    fn split_vc_batch_refs_handles_all_plain_batch() {
        let infos = [info(0, CIPHERSUITE), info(1, CIPHERSUITE)];
        let (plain, apq) = HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&infos).unwrap();
        assert_eq!(plain, refs(0..2));
        assert!(apq.is_empty());
    }

    #[test]
    fn split_vc_batch_refs_handles_empty_batch() {
        let (plain, apq) = HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&[]).unwrap();
        assert!(plain.is_empty());
        assert!(apq.is_empty());
    }

    #[test]
    fn split_vc_batch_refs_rejects_leading_pq_package() {
        let t = APQ_CIPHERSUITE.t_ciphersuite();
        let pq = APQ_CIPHERSUITE.pq_ciphersuite();
        // A PQ leg at index 0 has no preceding T leg to pair with.
        let infos = [info(0, pq), info(1, t)];
        assert!(HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&infos).is_err());
    }

    #[test]
    fn split_vc_batch_refs_rejects_odd_pair_block() {
        let t = APQ_CIPHERSUITE.t_ciphersuite();
        let pq = APQ_CIPHERSUITE.pq_ciphersuite();
        // Pair block of odd length: [t, pq, pq] leaves the trailing PQ leg
        // unpaired.
        let infos = [info(0, t), info(1, pq), info(2, pq)];
        assert!(HeterogeneousVcKeyPackageBatch::split_vc_batch_refs(&infos).is_err());
    }
}
