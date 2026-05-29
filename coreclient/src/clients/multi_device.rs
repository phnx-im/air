// SPDX-FileCopyrightText: 2026 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::ApiClient;
use aircommon::crypto::aead::{
    AeadDecryptable, AeadEncryptable, Ciphertext, keys::MultiDevicePairingKey,
};
use airprotos::relay_service::v1::LinkingSessionId;
use anyhow::{Context, anyhow, bail};
use openmls::{
    group::{MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, StagedWelcome},
    prelude::{
        BasicCredential, CredentialWithKey, KeyPackage, MlsMessageBodyIn, MlsMessageIn,
        MlsMessageOut, ProtocolVersion,
    },
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use sha2::{Digest, Sha256};
use tls_codec::{Deserialize, DeserializeBytes, Serialize};
use tokio_stream::StreamExt;
use tracing::info;

use crate::clients::{CIPHERSUITE, CoreUser};

const EXPORTER_LABEL: &str = "multi-device-pairing";

#[derive(Debug)]
pub struct EncryptedPingPongCtype;
pub type EncryptedPingPong = Ciphertext<EncryptedPingPongCtype>;

#[derive(
    Debug, Clone, tls_codec::TlsSerialize, tls_codec::TlsSize, tls_codec::TlsDeserializeBytes,
)]
pub struct PingPong {
    msg: Vec<u8>,
}

impl AeadEncryptable<MultiDevicePairingKey, EncryptedPingPongCtype> for PingPong {}
impl AeadDecryptable<MultiDevicePairingKey, EncryptedPingPongCtype> for PingPong {}

fn make_provider_and_credential(
    identity: &[u8],
) -> anyhow::Result<(OpenMlsRustCrypto, CredentialWithKey, SignatureKeyPair)> {
    let provider = OpenMlsRustCrypto::default();
    let credential = BasicCredential::new(identity.to_vec());
    let signature_keys =
        SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).context("keygen")?;
    signature_keys
        .store(provider.storage())
        .map_err(|e| anyhow!("store keys: {e}"))?;
    let credential_with_key = CredentialWithKey {
        credential: credential.into(),
        signature_key: signature_keys.to_public_vec().into(),
    };
    Ok((provider, credential_with_key, signature_keys))
}

// we consume the group and provider, so we can't keep using them
fn export_aead_key(
    group: MlsGroup,
    provider: OpenMlsRustCrypto,
) -> anyhow::Result<MultiDevicePairingKey> {
    let key_bytes = group
        .export_secret(provider.crypto(), EXPORTER_LABEL, &[], 32)
        .context("export_secret")?
        .try_into()
        .map_err(|_| anyhow!("invalid key length"))?;
    Ok(MultiDevicePairingKey::from_bytes(key_bytes))
}

impl CoreUser {
    pub async fn provision_multi_device_pairing(
        api_client: &ApiClient,
        session_id_tx: tokio::sync::oneshot::Sender<LinkingSessionId>,
    ) -> anyhow::Result<String> {
        let (provider, credential_with_key, signature_keys) =
            make_provider_and_credential(b"initiator")?;

        let key_package_bundle = KeyPackage::builder()
            .build(CIPHERSUITE, &provider, &signature_keys, credential_with_key)
            .context("build key package")?;
        let key_package_bytes = MlsMessageOut::from(key_package_bundle)
            .to_bytes()
            .context("serialize key package")?;
        let key_package_checksum: [u8; 32] = Sha256::digest(&key_package_bytes).into();

        let (tx, mut rx) = api_client.rs_provision_client().await?;

        // send the key package to the server
        tx.send(key_package_bytes.into()).await?;

        // The relay echoes back the session ID as the first frame
        let session_id_length = rx
            .next()
            .await
            .context("relay connection closed")??
            .as_u32()
            .context("unexpected format for first frame")?;

        // we recompose the session ID from our key package digest and the session ID length
        let session_id = LinkingSessionId::from_digest(&key_package_checksum, session_id_length)
            .context("invalid session ID")?;

        session_id_tx
            .send(session_id)
            .map_err(|_| anyhow!("session ID receiver dropped"))?;

        // wait for the existing (old) client to send us the welcome
        let welcome_bytes = rx.next().await.context("relay connection closed")??;
        let welcome_msg = MlsMessageIn::tls_deserialize_exact(welcome_bytes.as_slice())
            .context("failed to deserialize welcome")?;
        let welcome = match welcome_msg.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => bail!("expected a Welcome message"),
        };

        let join_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();

        let group = StagedWelcome::new_from_welcome(&provider, &join_config, welcome, None)
            .context("stage welcome")?
            .into_group(&provider)
            .context("join group")?;

        let cipher = export_aead_key(group, provider)?;
        info!("joined MLS group, AEAD key exported");

        tx.send(
            PingPong {
                msg: b"ping!".to_vec(),
            }
            .encrypt(&cipher)?
            .tls_serialize_detached()?
            .into(),
        )
        .await?;

        let frame = rx.next().await.context("relay connection closed")??;
        let answer = PingPong::decrypt(
            &cipher,
            &EncryptedPingPong::tls_deserialize_exact_bytes(frame.as_slice())?,
        )?;
        let answer_str = String::from_utf8(answer.msg)?;

        info!("got answer from old client, ciao!");

        Ok(answer_str)
    }

    pub async fn link_multi_device_pairing(
        &self,
        session_id: LinkingSessionId,
    ) -> anyhow::Result<String> {
        let client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;
        let qs_user_signing_key = self.key_store().qs_user_signing_key.clone();

        let (tx, mut rx) = client
            .rs_link_client(qs_user_id, &qs_user_signing_key, session_id.clone())
            .await?;

        let key_package_bytes = rx.next().await.context("relay connection closed")??;

        if !session_id.validate(key_package_bytes.as_slice()) {
            bail!("key package does not match session ID");
        }

        let (provider, credential_with_key, signature_keys) =
            make_provider_and_credential(b"existing-client")?;

        let candidate_key_package =
            match MlsMessageIn::tls_deserialize_exact(key_package_bytes.as_slice())
                .context("deserialize key package")?
                .extract()
            {
                MlsMessageBodyIn::KeyPackage(kp) => {
                    kp.validate(provider.crypto(), ProtocolVersion::Mls10)?
                }
                _ => bail!("expected a KeyPackage in first relay frame"),
            };

        let group_config = MlsGroupCreateConfig::builder()
            .use_ratchet_tree_extension(true)
            .ciphersuite(CIPHERSUITE)
            .build();

        let mut group = MlsGroup::new(
            &provider,
            &signature_keys,
            &group_config,
            credential_with_key,
        )
        .context("create group")?;

        let (_commit, welcome, _group_info) = group
            .add_members(&provider, &signature_keys, &[candidate_key_package])
            .context("add_members")?;

        group
            .merge_pending_commit(&provider)
            .context("merge pending commit")?;

        let welcome_bytes = welcome.to_bytes().context("serialize welcome")?;
        tx.send(welcome_bytes.into())
            .await
            .context("send welcome")?;

        // TODO: maybe use OpenMLS application messages instead? (because they're signed?)
        let cipher = export_aead_key(group, provider)?;
        info!("joined MLS group, AEAD key exported");

        // wait for ping
        let frame = rx.next().await.context("relay connection closed")??;
        let answer = PingPong::decrypt(
            &cipher,
            &EncryptedPingPong::tls_deserialize_exact_bytes(frame.as_slice())?,
        )?;
        let answer_str = String::from_utf8(answer.msg)?;
        info!(answer_str, "got ping from new client");

        tx.send(
            PingPong {
                msg: b"pong!".to_vec(),
            }
            .encrypt(&cipher)?
            .tls_serialize_detached()?
            .into(),
        )
        .await?;

        Ok(answer_str)
    }
}
