use aircommon::identifiers::Fqdn;
use airprotos::relay_service::v1::RelayFrame;
use anyhow::{Context, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
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
use tls_codec::Deserialize;
use tokio_stream::StreamExt;
use tracing::info;
use url::Url;

use crate::clients::{CIPHERSUITE, CoreUser, api_clients::ApiClients};

const EXPORTER_LABEL: &str = "multi-device-pairing";

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

// fn export_aead_key(
//     group: &MlsGroup,
//     provider: &OpenMlsRustCrypto,
// ) -> anyhow::Result<MultiDevicePairingKey> {
//     let key_bytes = group
//         .export_secret(provider.crypto(), EXPORTER_LABEL, &[], 32)
//         .context("export_secret")?
//         .try_into()
//         .map_err(|_| anyhow!("invalid key length"))?;
//     Ok(MultiDevicePairingKey::from_bytes(key_bytes))
// }

// /// Encrypt `plaintext` and return `nonce || ciphertext`.
// fn aead_seal(cipher: &AeadKey, plaintext: &[u8]) -> Result<Vec<u8>> {
//     let ct = cipher
//         .encrypt(&nonce, plaintext)
//         .map_err(|e| anyhow!("encrypt: {e}"))?;
//     let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
//     out.extend_from_slice(&nonce);
//     out.extend_from_slice(&ct);
//     Ok(out)
// }

// /// Decrypt a `nonce || ciphertext` frame and return the plaintext.
// fn aead_open(cipher: &Aes256Gcm, frame: &[u8]) -> Result<Vec<u8>> {
//     if frame.len() < NONCE_LEN {
//         bail!("frame too short");
//     }
//     let nonce = Nonce::from_slice(&frame[..NONCE_LEN]);
//     cipher
//         .decrypt(nonce, &frame[NONCE_LEN..])
//         .map_err(|e| anyhow!("decrypt: {e}"))
// }

impl CoreUser {
    pub async fn provision_multi_device_pairing(
        domain: Fqdn,
        server_url: Option<Url>,
        session_id_tx: tokio::sync::oneshot::Sender<String>,
    ) -> anyhow::Result<String> {
        let api_clients = ApiClients::new(domain, server_url);
        let api_client = api_clients.default_client()?;

        let (provider, credential_with_key, signature_keys) =
            make_provider_and_credential(b"initiator")?;

        let key_package_bundle = KeyPackage::builder()
            .build(CIPHERSUITE, &provider, &signature_keys, credential_with_key)
            .context("build key package")?;
        let fingerprint = key_package_bundle
            .key_package()
            .hash_ref(provider.crypto())?;
        let key_package_bytes = MlsMessageOut::from(key_package_bundle)
            .to_bytes()
            .context("serialize key package")?;

        let fingerprint_base64 = B64.encode(fingerprint.as_slice());

        let (tx, mut rx) = api_client
            .rs_provision_client(fingerprint_base64.clone())
            .await?;

        dbg!("REPORTING");
        session_id_tx.send(fingerprint_base64).unwrap(); // FIXME: not correct
        dbg!("REPORTED");

        // send a fresh key package to the peer, (TODO? for validation reasons) the server will look at it and check if its valid
        tx.send(RelayFrame::from_bytes(key_package_bytes)).await?;

        // wait fo the existing (old) client to send us the welcome
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

        // let cipher = export_aead_key(&group, &provider)?;
        info!("joined MLS group, AEAD key exported");

        // TODO: start decrypting and encrypting stuff here

        Ok(String::new())
    }

    pub async fn link_multi_device_pairing(&self, session_id: String) -> anyhow::Result<()> {
        let client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;
        let qs_user_signing_key = self.key_store().qs_user_signing_key.clone();

        let (tx, mut rx) = client
            .rs_link_client(qs_user_id, &qs_user_signing_key, session_id)
            .await?;

        let key_package_bytes = rx.next().await.context("relay connection closed")??;

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

        // let cipher = export_aead_key(&group, &provider)?;

        let welcome_bytes = welcome.to_bytes().context("serialize welcome")?;
        tx.send(RelayFrame::from_bytes(welcome_bytes))
            .await
            .context("send welcome")?;

        Ok(())
    }
}
