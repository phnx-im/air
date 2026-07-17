// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use airapiclient::rs_api::RsRequestError;
use aircommon::codec::PersistenceCodec;
use aircommon::credentials::keys::{ClientSigningKey, PreliminaryClientSigningKey};
use aircommon::crypto::RatchetDecryptionKey;
use aircommon::crypto::aead::keys::{
    IdentityLinkWrapperKey, PushTokenEarKey, WelcomeAttributionInfoEarKey,
};
use aircommon::crypto::aead::{
    AeadDecryptable, AeadEncryptable, Ciphertext, keys::MultiDeviceLinkingKey,
};
use aircommon::crypto::hpke::ClientIdEncryptionKey;
use aircommon::crypto::indexed_aead::keys::UserProfileKey;
use aircommon::crypto::kdf::keys::RatchetSecret;
use aircommon::crypto::signatures::keys::{QsClientSigningKey, QsUserSigningKey};
use aircommon::identifiers::{Fqdn, QsClientId, QsUserId, UserId};
use aircommon::messages::{FriendshipToken, QueueMessage};
use aircommon::mls_group_config::{
    APQ_CIPHERSUITE, QS_CLIENT_REFERENCE_EXTENSION_TYPE, default_key_package_extensions,
    default_leaf_node_capabilities, default_leaf_node_extensions,
};
use airprotos::client::component::AirComponent;
use airprotos::relay_service::v1::{LinkingSessionId, RelayFrame};
use anyhow::{Context, anyhow, bail};
use apqmls::authentication::ApqCredentialWithKey;
use apqmls::messages::ApqKeyPackage;
use openmls::components::vc_derivation_info::EpochId;
use openmls::group::GroupId;
use openmls::prelude::{Credential, CredentialType, SignaturePublicKey};
use openmls::{
    group::{MlsGroup, MlsGroupCreateConfig, MlsGroupJoinConfig, StagedWelcome},
    prelude::{
        BasicCredential, CredentialWithKey, Extension, KeyPackage, MlsMessageBodyIn, MlsMessageIn,
        MlsMessageOut, ProtocolVersion, UnknownExtension,
    },
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider;
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tls_codec::{Deserialize, DeserializeBytes, Serialize as _};
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::groups::self_group::SelfGroup;
use crate::{
    clients::{
        CIPHERSUITE, CoreUser,
        api_clients::ApiClients,
        create_user::QsRegisteredUserState,
        listen_response,
        own_client_info::OwnClientInfo,
        process::process_qs::ProcessedQsMessages,
        store::{ClientRecord, UserCreationState},
    },
    contacts::{ContactAddInfos, ContactKeyPackage},
    groups::{
        Group, PreparedInvitee, client_auth_info::StorableClientCredential,
        openmls_provider::AirOpenMlsProvider,
    },
    key_stores::{
        MemoryUserKeyStore, indexed_keys::StorableIndexedKey,
        queue_ratchets::StorableQsQueueRatchet,
    },
    utils::persistence::{open_air_db, open_client_db, open_lock_file},
};

const EXPORTER_LABEL: &str = "multi-device-linking";

/// Everything the old (existing) device hands to the new device over the
/// secure linking channel so the new device can bootstrap a working
/// [`CoreUser`] and join the user's self group.
#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct ProvisioningPackage {
    // Identity + AS client credential (shared across devices for the MVP).
    pub(crate) user_id: UserId,
    pub(crate) client_signing_key: ClientSigningKey,
    // User-level QS key material (shared by all of the user's devices).
    pub(crate) qs_user_id: QsUserId,
    pub(crate) qs_user_signing_key: QsUserSigningKey,
    pub(crate) friendship_token: FriendshipToken,
    pub(crate) push_token_ear_key: PushTokenEarKey,
    pub(crate) wai_ear_key: WelcomeAttributionInfoEarKey,
    pub(crate) qs_client_id_encryption_key: ClientIdEncryptionKey,
    // Freshly created queue for the new device (created by the old device).
    pub(crate) qs_client_id: QsClientId,
    pub(crate) qs_client_signing_key: QsClientSigningKey,
    pub(crate) qs_queue_decryption_key: RatchetDecryptionKey,
    pub(crate) qs_initial_ratchet_secret: RatchetSecret,
    // User profile.
    pub(crate) user_profile_key: UserProfileKey,
    // Self-group metadata not carried by the Welcome.
    pub(crate) self_group_id: GroupId,
    pub(crate) identity_link_wrapper_key: IdentityLinkWrapperKey,
}

#[derive(Debug)]
pub struct EncryptedLinkingMessageCtype;
pub type EncryptedLinkingMessage = Ciphertext<EncryptedLinkingMessageCtype>;

#[derive(
    Debug, Clone, tls_codec::TlsSerialize, tls_codec::TlsSize, tls_codec::TlsDeserializeBytes,
)]
struct LinkingMessage {
    bytes: Vec<u8>,
}

impl AeadEncryptable<MultiDeviceLinkingKey, EncryptedLinkingMessageCtype> for LinkingMessage {}
impl AeadDecryptable<MultiDeviceLinkingKey, EncryptedLinkingMessageCtype> for LinkingMessage {}

impl LinkingMessage {
    /// Serialize and Encrypt `value` under the linking key into a serialized relay frame.
    fn seal<T>(value: T, cipher: &MultiDeviceLinkingKey) -> anyhow::Result<RelayFrame>
    where
        T: Serialize,
    {
        let bytes = PersistenceCodec::to_vec(&value)?;
        let frame = LinkingMessage { bytes }
            .encrypt(cipher)?
            .tls_serialize_detached()?;
        Ok(frame.into())
    }

    /// Decrypt a relay frame's bytes and deserialize it into the payload.
    fn open<T: DeserializeOwned>(
        frame: &[u8],
        cipher: &MultiDeviceLinkingKey,
    ) -> anyhow::Result<T> {
        let message = LinkingMessage::decrypt(
            cipher,
            &EncryptedLinkingMessage::tls_deserialize_exact_bytes(frame)?,
        )?;
        Ok(PersistenceCodec::from_slice(&message.bytes)?)
    }
}

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

// Consumes the group and provider by value; they can't be used after export.
fn export_aead_key(
    group: MlsGroup,
    provider: OpenMlsRustCrypto,
) -> anyhow::Result<MultiDeviceLinkingKey> {
    let key_bytes = group
        .export_secret(provider.crypto(), EXPORTER_LABEL, &[], 32)
        .context("export_secret")?
        .try_into()
        .map_err(|_| anyhow!("invalid key length"))?;
    Ok(MultiDeviceLinkingKey::from_bytes(key_bytes))
}

#[derive(Debug)]
pub enum MultiDeviceProvisionStep {
    /// When the session is open and the server acknowledges it, we can use the session ID.
    SessionId(LinkingSessionId),
    /// When the existing client has connected on the other side.
    Linking,
}

#[derive(Debug, thiserror::Error)]
pub enum MultiDeviceLinkClientError {
    #[error("session ID not found")]
    SessionNotFound,
}

impl CoreUser {
    /// Provisions a new client for linking by connecting to the relay at `domain`.
    ///
    /// On success returns a fully bootstrapped [`CoreUser`] for the freshly
    /// linked device, persisted under `db_path`.
    pub async fn multi_device_provision_client(
        db_path: &str,
        domain: Fqdn,
        server_url: Option<Url>,
        session_tx: tokio::sync::mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> anyhow::Result<CoreUser> {
        let (provider, credential_with_key, signature_keys) =
            make_provider_and_credential(b"initiator")?;

        let key_package_bundle = KeyPackage::builder()
            .build(CIPHERSUITE, &provider, &signature_keys, credential_with_key)
            .context("build key package")?;
        let key_package_bytes = MlsMessageOut::from(key_package_bundle)
            .to_bytes()
            .context("serialize key package")?;
        let key_package_checksum: [u8; 32] = Sha256::digest(&key_package_bytes).into();

        let api_clients = ApiClients::new(domain, server_url);

        let (tx, mut rx) = api_clients
            .default_client()?
            .rs_multi_device_provision_client()
            .await?;

        // Send the key package to the server.
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

        session_tx
            .send(MultiDeviceProvisionStep::SessionId(session_id))
            .await
            .map_err(|_| anyhow!("reporting stream dropped"))?;

        // wait for the existing (old) client to send us the welcome
        let welcome_bytes = rx.next().await.context("relay connection closed")??;

        session_tx
            .send(MultiDeviceProvisionStep::Linking)
            .await
            .map_err(|_| anyhow!("reporting stream dropped"))?;

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
        info!("secure linking channel established, AEAD key exported");

        // The old device creates our queue and sends us everything we need to
        // bootstrap a working client.
        let frame = rx.next().await.context("relay connection closed")??;
        let package: ProvisioningPackage = LinkingMessage::open(frame.as_slice(), &cipher)?;
        info!("received provisioning package");

        // Join the self group:
        // 1. mint a new signing key to use for self-group commits
        // 2. generate a self-group KeyPackage
        // 3. hand it to the old device
        // 4. old device adds us via the DS
        // 5. we then process the Welcome that the QS fans out to our fresh queue.
        let core_user = Self::link_new_device(api_clients, db_path, package).await?;
        info!("bootstrapped linked client");

        let self_group_kp = core_user.generate_self_group_key_package().await?;
        tx.send(LinkingMessage::seal(self_group_kp, &cipher)?)
            .await
            .context("send self-group key package")?;
        info!("sent self-group key package to old device");

        core_user.join_self_group_from_queue().await?;
        info!("joined self group");

        Ok(core_user)
    }

    /// Establishes a session with a new device (with the given `session_id`). The `connected_tx` and `confirmation_rx` are
    /// channels to report established connection and wait for the user's confirmation.
    pub async fn multi_device_link_client(
        &self,
        session_id: LinkingSessionId,
        connected_tx: oneshot::Sender<()>,
        confirmation_rx: oneshot::Receiver<()>,
    ) -> anyhow::Result<Result<(), MultiDeviceLinkClientError>> {
        let client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;
        let qs_user_signing_key = self.key_store().qs_user_signing_key.clone();

        let (tx, mut rx) = match client
            .rs_multi_device_link_client(qs_user_id, &qs_user_signing_key, session_id.clone())
            .await
        {
            Ok((tx, rx)) => (tx, rx),
            Err(RsRequestError::SessionNotFound) => {
                return Ok(Err(MultiDeviceLinkClientError::SessionNotFound));
            }
            Err(e) => return Err(e.into()),
        };

        // Signal that we're connected to the relay
        let _ = connected_tx.send(());

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

        // Wait for the user to approve the link on this (existing) device.
        confirmation_rx
            .await
            .context("confirmation channel closed")?;

        let cipher = export_aead_key(group, provider)?;
        info!("secure linking channel established, AEAD key exported");

        // Build the provisioning package (creates a fresh queue for the new
        // device) and hand it over the secure channel.
        let package = self.build_provisioning_package().await?;
        tx.send(LinkingMessage::seal(package, &cipher)?)
            .await
            .context("send provisioning package")?;
        info!("sent provisioning package to new device");

        // Receive the new device's self-group KeyPackage and add it to the self
        // group via the DS.
        let frame = rx.next().await.context("relay connection closed")??;
        let self_group_kp: ApqKeyPackage = LinkingMessage::open(frame.as_slice(), &cipher)?;
        self.add_client_to_self_group(self_group_kp).await?;
        info!("added new device to self group");

        // Keep the RPC alive until the relay closes our stream, which happens
        // once the new device has finished and disconnected.
        while rx.next().await.is_some() {}

        Ok(Ok(()))
    }

    /// Create a fresh QS queue for a new device and gather all the key material
    /// the new device needs to bootstrap a working [`CoreUser`] and join the
    /// self group.
    async fn build_provisioning_package(&self) -> anyhow::Result<ProvisioningPackage> {
        let api_client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;

        let self_group = self.ensure_self_group().await?;
        let self_group_id = self_group.group_id().clone();
        let identity_link_wrapper_key = self_group.identity_link_wrapper_key().clone();

        // Generate a fresh queue for the new device and register it under our
        // virtual client (QsUserId) at the QS.
        let key_store = self.key_store();
        let qs_client_signing_key = QsClientSigningKey::generate()?;
        let qs_queue_decryption_key = RatchetDecryptionKey::generate()?;
        let qs_initial_ratchet_secret = RatchetSecret::random()?;
        let response = api_client
            .qs_create_client(
                qs_user_id,
                qs_client_signing_key.verifying_key().clone(),
                qs_queue_decryption_key.encryption_key().clone(),
                // MVP: no push token for the new device yet.
                None,
                qs_initial_ratchet_secret.clone(),
                &key_store.qs_user_signing_key,
            )
            .await?;
        let qs_client_id = response.qs_client_id;

        let user_profile_key = UserProfileKey::load_own(self.db().read().await?).await?;

        Ok(ProvisioningPackage {
            user_id: self.user_id().clone(),
            client_signing_key: key_store.signing_key.clone(),
            qs_user_id,
            qs_user_signing_key: key_store.qs_user_signing_key.clone(),
            friendship_token: key_store.friendship_token.clone(),
            push_token_ear_key: key_store.push_token_ear_key.clone(),
            wai_ear_key: key_store.wai_ear_key.clone(),
            qs_client_id_encryption_key: key_store.qs_client_id_encryption_key.clone(),
            qs_client_id,
            qs_client_signing_key,
            qs_queue_decryption_key,
            qs_initial_ratchet_secret,
            user_profile_key,
            self_group_id,
            identity_link_wrapper_key,
        })
    }

    /// The signing key used for this client's leaf in the self group.
    async fn self_group_signature_key(&self) -> anyhow::Result<ClientSigningKey> {
        let stored: OwnClientInfo = OwnClientInfo::load(self.db().read().await?).await?;
        stored
            .self_group_signing_key
            .context("self-group signer was not initialized")
    }

    /// Generate an APQ KeyPackage for this (freshly linked) device to be added
    /// to the self group.
    async fn generate_self_group_key_package(&self) -> anyhow::Result<ApqKeyPackage> {
        let signer = self.self_group_signature_key().await?;
        // Both T and PQ leaves use this device's fresh signature key (the PQ
        // side is confidentiality-only), which is what the DS expects.
        let signature_key = SignaturePublicKey::from(signer.verifying_key().clone());
        let credential = ApqCredentialWithKey {
            t_credential: CredentialWithKey {
                credential: Credential::try_from(signer.credential())?,
                signature_key: signature_key.clone(),
            },
            pq_credential: CredentialWithKey {
                credential: Credential::new(CredentialType::Basic, Vec::new()),
                signature_key,
            },
        };

        let mut leaf_node_extensions = default_leaf_node_extensions::<AirComponent>();
        let client_reference = self.create_own_client_reference();
        // TODO: don't use Extension::Unknown
        leaf_node_extensions.add(Extension::Unknown(
            QS_CLIENT_REFERENCE_EXTENSION_TYPE,
            UnknownExtension(client_reference.tls_serialize_detached()?),
        ))?;
        // add two fields AirComponent Option<QsClientId> and Option<QsUserId>
        let key_package_extensions = default_key_package_extensions::<AirComponent>();

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                let bundle = ApqKeyPackage::builder()
                    .key_package_extensions(key_package_extensions)
                    .leaf_node_capabilities(default_leaf_node_capabilities())
                    .leaf_node_extensions(leaf_node_extensions)
                    .build(&provider, APQ_CIPHERSUITE, &signer, credential)?;
                Ok(bundle.into_key_package())
            })
            .await
    }

    async fn add_client_to_self_group(&self, key_package: ApqKeyPackage) -> anyhow::Result<()> {
        let api_client = self.api_client()?;
        let self_group_signature_key = self.self_group_signature_key().await?;
        let user_id = self.user_id().clone();
        let wai_key = self.key_store().wai_ear_key.clone();
        let qs_client_reference = self.create_own_client_reference();

        // Stage the add commit against a fresh copy of the self group.
        let (mut group, params, group_state_ear_key, encrypted_user_profile_key) = self
            .db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let self_group_id = OwnClientInfo::load_self_group_id(&mut *txn)
                    .await?
                    .context("no self group")?;
                let mut group = Group::load_clean_verified(&mut *txn, &self_group_id)
                    .await?
                    .context("self group not found")?;
                let user_profile_key = UserProfileKey::load_own(&mut *txn).await?;

                let invitee = PreparedInvitee {
                    add_info: ContactAddInfos {
                        key_package: ContactKeyPackage::Apq(Box::new(key_package)),
                        user_profile_key: user_profile_key.clone(),
                    },
                    wai_key: wai_key.clone(),
                    // Only used for its `user_id()` (to bind the profile-key
                    // ciphertext); the new device shares our user id.
                    client_credential: self.signing_key().credential().clone(),
                };
                // Sign the commit with the fresh self-group key, but sign the
                // WAI with the shared client key so the joiner can verify it
                // against our client credential.
                let params = group
                    .group_mut()
                    .stage_apq_invite(
                        &mut *txn,
                        &self_group_signature_key,
                        self.signing_key(),
                        vec![invitee],
                    )?
                    .map_err(|validation| {
                        anyhow!("self-group invite leaf validation: {validation:?}")
                    })?;
                let group_state_ear_key = group.group_state_ear_key().clone();
                let encrypted_user_profile_key =
                    user_profile_key.encrypt(group.identity_link_wrapper_key(), &user_id)?;
                Ok((
                    group,
                    params,
                    group_state_ear_key,
                    encrypted_user_profile_key,
                ))
            })
            .await?;

        // Send the commit to the DS. The MLS commit was signed by our leaf
        // (fresh self-group key), but the DS request envelope is signed with the
        // shared client credential key: the DS authenticates requests against
        // the sender's credential key, not the leaf key.
        let ds_timestamp = api_client
            .ds_apq_group_operation(
                params,
                self.signing_key(),
                &group_state_ear_key,
                qs_client_reference,
                encrypted_user_profile_key,
            )
            .await?;

        // Merge the pending commit if the DS accepted it. The queue handler
        // runs concurrently and may have already merged it while processing
        // the DS commit response, so we might also skip it here.
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                let stored_epoch = Group::load(&mut *txn, group.group_id())
                    .await?
                    .context("self group not found")?
                    .mls_group()
                    .epoch();
                if stored_epoch > group.mls_group().epoch() {
                    debug!("self-group add commit already merged by the queue handler");
                    return Ok(());
                }
                group.merge_pending_commit(txn, None, ds_timestamp).await?;
                group
                    .group_mut()
                    .store_update(&mut *txn, None, None)
                    .await?;
                Ok(())
            })
            .await?;

        // Now that the new device is a member, register the VC emulation epoch
        // at this (post-Add) self-group epoch. The joining device registers at
        // the same epoch, so both siblings derive the same EpochId.
        //
        // NB(gabriel): this is currently just a sanity check, since we don't need the [`EpochId`] here
        self.register_self_group_vc_emulation_epoch().await?;

        Ok(())
    }

    /// Register a virtual-clients emulation epoch on the self group.
    async fn register_self_group_vc_emulation_epoch(&self) -> anyhow::Result<EpochId> {
        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let mut self_group = SelfGroup::load(&mut *txn)
                    .await?
                    .context("self group not found")?;
                let epoch_id = self_group.register_vc_emulation_epoch(&mut *txn)?;
                debug!(?epoch_id, "registered self-group VC emulation epoch");
                Ok(epoch_id)
            })
            .await
    }

    /// Poll our QS queue until the self-group Welcome arrives.
    async fn join_self_group_from_queue(&self) -> anyhow::Result<()> {
        let self_group_id = OwnClientInfo::load_self_group_id(self.db().read().await?)
            .await?
            .context("no self group id")?;

        for _ in 0..500 {
            let already_joined = self
                .db()
                .with_read_transaction(async |txn| Group::load(txn, &self_group_id).await)
                .await?
                .is_some();
            if already_joined {
                // Register the VC emulation epoch at the epoch we just joined
                // into, matching the device that performed the Add.
                self.register_self_group_vc_emulation_epoch().await?;
                return Ok(());
            }

            let processed = self.drain_and_process_qs_queue().await?;
            for error in &processed.errors {
                warn!(%error, "error while processing self-group queue message");
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        anyhow::bail!("timed out waiting for the self-group welcome");
    }

    /// Fetches messages from the QS queue, fully processes and ACKs them.
    ///
    /// Unlike [`CoreUser::qs_fetch_messages`], this ACKs the processed messages
    /// via the responder, so it is safe to use outside of integration tests.
    async fn drain_and_process_qs_queue(&self) -> anyhow::Result<ProcessedQsMessages> {
        let (mut stream, responder) = self.listen_queue().await?;
        let mut messages: Vec<QueueMessage> = Vec::new();

        while let Some(message) = stream.next().await {
            match message.event {
                // Empty event is the sentinel: the queue is drained.
                Some(listen_response::Event::Empty(_)) => break,
                Some(listen_response::Event::Message(queue_message)) => {
                    if let Ok(queue_message) = queue_message.try_into() {
                        messages.push(queue_message);
                    }
                }
                Some(listen_response::Event::Payload(_)) | None => {}
            }
        }

        let num_messages = messages.len();
        let max_sequence_number = messages.last().map(|m| m.sequence_number);
        let processed = self.fully_process_qs_messages(messages).await;

        if processed.processed == num_messages {
            if let Some(max_sequence_number) = max_sequence_number {
                // Acks all messages before max_sequence_number + 1 (exclusive).
                responder.ack(max_sequence_number + 1).await;
            }
        } else {
            error!(
                processed.processed,
                num_messages, "failed to fully process self-group queue messages"
            );
        }

        Ok(processed)
    }

    /// Bootstrap a [`CoreUser`] on a freshly linked device from the
    /// provisioning package received over the secure linking channel.
    async fn link_new_device(
        api_clients: ApiClients,
        db_path: &str,
        package: ProvisioningPackage,
    ) -> anyhow::Result<CoreUser> {
        let air_db = open_air_db(db_path).await?;
        let client_db = open_client_db(&package.user_id, db_path).await?;
        let global_lock = open_lock_file(db_path)?;

        let ProvisioningPackage {
            user_id,
            client_signing_key,
            qs_user_id,
            qs_user_signing_key,
            friendship_token,
            push_token_ear_key,
            wai_ear_key,
            qs_client_id_encryption_key,
            qs_client_id,
            qs_client_signing_key,
            qs_queue_decryption_key,
            qs_initial_ratchet_secret,
            user_profile_key,
            self_group_id,
            identity_link_wrapper_key: _,
        } = package;

        let shared_client_credential = client_signing_key.credential().clone();
        let key_store = MemoryUserKeyStore {
            signing_key: client_signing_key,
            qs_client_signing_key,
            qs_user_signing_key,
            qs_queue_decryption_key,
            push_token_ear_key,
            friendship_token,
            wai_ear_key,
            qs_client_id_encryption_key,
        };

        // Mint a fresh signing key to use in self-group operations
        let self_group_signing_key = ClientSigningKey::from_prelim_key_with_foreign_credential(
            PreliminaryClientSigningKey::generate()?,
            key_store.signing_key.credential().clone(),
        )?;

        client_db
            .with_write_transaction(async |txn| -> anyhow::Result<()> {
                StorableClientCredential::new(key_store.signing_key.credential().clone())
                    .store(&mut *txn)
                    .await?;
                StorableQsQueueRatchet::initialize(&mut *txn, qs_initial_ratchet_secret).await?;
                user_profile_key.store_own(&mut *txn).await?;

                OwnClientInfo {
                    qs_user_id,
                    qs_client_id,
                    user_id: user_id.clone(),
                    self_group_id: Some(self_group_id),
                    self_group_signing_key: Some(self_group_signing_key),
                }
                .store(&mut *txn)
                .await?;

                // Schedule the fetching operation of our own profile information for when the [`CoreClient`]
                // starts (or more specifically, when the outbound service runs for the first time.)
                Self::schedule_fetch_user_profile(
                    txn,
                    (shared_client_credential, user_profile_key),
                )
                .await?;

                Ok(())
            })
            .await?;

        let final_state = UserCreationState::FinalUserState(
            QsRegisteredUserState::new(key_store, qs_user_id, qs_client_id)
                .persist()
                .await?,
        );
        final_state.store(client_db.write().await?).await?;

        let mut client_record = ClientRecord::new(user_id.clone());
        client_record.finish();
        client_record.store(air_db.write().await?).await?;

        Ok(final_state
            .final_state()?
            .into_self_user(client_db, api_clients, global_lock))
    }
}
