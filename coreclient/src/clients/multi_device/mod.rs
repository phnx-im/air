// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

pub(crate) mod device_link;
mod payloads;
#[cfg(any(test, feature = "test_utils"))]
mod test_utils;
#[cfg(test)]
mod tests;

use airapiclient::{
    ClassifyRequestError, RequestErrorKind, qs_api::QsListenResponder, rs_api::RsRequestError,
};
use aircommon::codec::PersistenceCodec;
use aircommon::credentials::keys::SelfGroupSigningKey;
use aircommon::crypto::RatchetDecryptionKey;
use aircommon::crypto::aead::{
    AeadDecryptable, AeadEncryptable, Ciphertext, keys::MultiDeviceLinkingKey,
};
use aircommon::crypto::indexed_aead::keys::UserProfileKey;
use aircommon::crypto::kdf::keys::RatchetSecret;
use aircommon::crypto::signatures::keys::QsClientSigningKey;
use aircommon::identifiers::Fqdn;
use aircommon::messages::QueueMessage;
use aircommon::mls_group_config::{APQ_CIPHERSUITE, self_group_leaf_node_capabilities};
use airprotos::client::app_data::ClientAppData;
use airprotos::client::self_group::SettingsUpdate;
use airprotos::relay_service::mdl::{MdlMessage, SessionAssigned};
use airprotos::relay_service::v1::{RelayFrame, RendezVousId};
use anyhow::{Context, anyhow};
use apqmls::authentication::ApqCredentialWithKey;
use apqmls::messages::ApqKeyPackage;
use chrono::{DateTime, Utc};
use openmls::group::GroupId;
use openmls::prelude::{Credential, CredentialType, SignaturePublicKey};
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
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use tls_codec::{Deserialize as _, DeserializeBytes, Serialize as _};
use tokio::{
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use tonic::Streaming;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatStatus, ChatType, Contact,
    chats::{
        connection_requests::{requests_snapshot, store_provisioned_requests},
        outgoing_requests::OutgoingRequest,
    },
    clients::{
        CIPHERSUITE, CoreUser, ListenQueueError, ListenResponse,
        api_clients::ApiClients,
        block_contact::persistence::{apply_blocked_contacts_update, blocked_contacts_snapshot},
        create_user::QsRegisteredUserState,
        listen_response,
        own_client_info::{OwnClientInfo, UnlinkReason},
        process::process_qs::ProcessedQsMessages,
        store::{ClientRecord, UserCreationState},
        user_settings::{SettingsUpdateExt, apply_settings_update},
    },
    db::access::DbAccess,
    delete_client_database,
    groups::{
        Group, client_auth_info::StorableUserCredential, client_reference_extension,
        openmls_provider::AirOpenMlsProvider,
    },
    key_stores::{
        MemoryUserKeyStore, indexed_keys::StorableIndexedKey,
        queue_ratchets::StorableQsQueueRatchet,
    },
    outbound_service::timed_tasks::skip_signed_connection_package_upload,
    privacy_pass,
    usernames::sync::{apply_usernames_update, usernames_snapshot},
    utils::{
        global_lock::GlobalLock,
        persistence::{open_air_db, open_client_db, open_lock_file},
    },
};

use device_link::{DeviceLink, DeviceLinkFailure, DeviceLinkState, ProvisionedQueue};

pub(crate) use payloads::{ConnectionContact, HigherLevelGroup};
use payloads::{ProvisioningPackage, SelfGroupJoinRequest};

const EXPORTER_LABEL: &str = "multi-device-linking";

/// How long a linking code stays valid. When no existing device has answered
/// it by then, the new device ends the session and asks for a new code.
#[cfg(not(feature = "test_utils"))]
const CODE_LIFETIME: Duration = Duration::from_secs(60);

#[cfg(feature = "test_utils")]
const CODE_LIFETIME: Duration = Duration::from_secs(15); // to go faster

/// How long a device waits for the relay to let go of the call after the
/// device's last frame, before giving up on it.
const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Pause before the new device reopens its queue stream after it failed
/// while waiting for the self-group Welcome.
const QUEUE_REOPEN_DELAY: Duration = Duration::from_secs(1);

/// How long the new device waits for the self-group Welcome after sending
/// its join request.
const WELCOME_TIMEOUT: Duration = Duration::from_secs(120);

/// How long the existing device waits for the new device to confirm the
/// join after the add committed.
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(60);

/// How long the existing device waits for the outbound service to add the
/// new device to the self group.
const ADD_TIMEOUT: Duration = Duration::from_secs(90);

/// Pause between two looks for the new device in the self group.
const ADD_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// How long a device link may stay unfinished before the outbound service
/// undoes it.
const LINK_DEADLINE: Duration = Duration::from_secs(10 * 60);

/// What the new device sends once it joined the self group. Without it the
/// existing device takes the new device out again.
#[derive(Serialize, Deserialize)]
struct LinkingComplete;

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
    fn seal<T>(value: &T, cipher: &MultiDeviceLinkingKey) -> anyhow::Result<RelayFrame>
    where
        T: Serialize,
    {
        let bytes = PersistenceCodec::to_vec(value)?;
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
        .as_slice()
        .try_into()
        .map_err(|_| anyhow!("invalid key length"))?;
    Ok(MultiDeviceLinkingKey::from_bytes(key_bytes))
}

/// A step of the new device's provisioning run, reported to the UI.
#[derive(Debug)]
pub enum MultiDeviceProvisionStep {
    Code(String),
    Linking,
}

/// Why the existing device could not link the new device.
#[derive(Debug, thiserror::Error)]
pub enum MultiDeviceLinkClientError {
    #[error("session ID not found")]
    SessionNotFound,
    /// The code is not a rendezvous ID.
    #[error("the linking code is malformed")]
    InvalidCode,
    #[error("device limit reached: max = {max_devices}")]
    DeviceLimitReached { max_devices: u32 },
}

/// Why a linking session ended badly.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LinkingError {
    #[error("linking protocol error")]
    Protocol(#[source] anyhow::Error),
    /// The peer did not take its next step in time.
    #[error("timed out waiting for {0}")]
    TimedOut(&'static str),
    /// The relay call ended under us, so there is nobody left to tell.
    #[error("the linking session closed")]
    SessionClosed(#[source] anyhow::Error),
    /// The DS refused to grow the self group.
    #[error("device limit reached: max = {max_devices}")]
    DeviceLimitReached { max_devices: u32 },
    /// The existing device took back the queue it had created for this
    /// device, so it gave up on the link.
    #[error("the existing device deleted this device's queue")]
    QueueDeleted,
}

impl From<anyhow::Error> for LinkingError {
    fn from(error: anyhow::Error) -> Self {
        Self::Protocol(error)
    }
}

/// When a device link started now counts as abandoned.
fn link_deadline() -> DateTime<Utc> {
    Utc::now() + LINK_DEADLINE
}

/// Reads the next frame off the relay stream.
async fn next_frame(rx: &mut Streaming<RelayFrame>) -> Result<RelayFrame, LinkingError> {
    match rx.next().await {
        Some(Ok(frame)) => Ok(frame),
        Some(Err(status)) => Err(LinkingError::SessionClosed(
            anyhow::Error::from(status).context("the linking stream failed"),
        )),
        None => Err(LinkingError::SessionClosed(anyhow!(
            "the relay closed the linking session"
        ))),
    }
}

/// Hands a frame to the relay call. A refused frame means the call is over.
async fn send_frame(
    tx: &mpsc::Sender<RelayFrame>,
    frame: RelayFrame,
    what: &'static str,
) -> Result<(), LinkingError> {
    tx.send(frame)
        .await
        .map_err(|_| LinkingError::SessionClosed(anyhow!("could not {what}")))
}

/// The error for a relay frame, or the relay's loss, that arrived while the
/// peer was meant to stay quiet.
fn relay_interrupted(frame: Result<RelayFrame, LinkingError>) -> LinkingError {
    match frame {
        Ok(_) => LinkingError::Protocol(anyhow!("unexpected linking message")),
        Err(error) => error,
    }
}

/// Closes the request stream and waits for the relay to end the call.
async fn close_and_drain(tx: mpsc::Sender<RelayFrame>, rx: &mut Streaming<RelayFrame>) {
    drop(tx);
    let drained = async { while rx.next().await.is_some() {} };
    if tokio::time::timeout(TEARDOWN_TIMEOUT, drained)
        .await
        .is_err()
    {
        debug!("the relay did not end the linking call in time");
    }
}

/// Reads the next frame off the relay stream and opens it under the linking
/// key.
async fn receive_sealed<T: DeserializeOwned>(
    cipher: &MultiDeviceLinkingKey,
    rx: &mut Streaming<RelayFrame>,
) -> Result<T, LinkingError> {
    let frame = next_frame(rx).await?;
    Ok(LinkingMessage::open(frame.as_slice(), cipher)?)
}

/// Hands the new device the provisioning package and reads back its
/// self-group join request.
async fn exchange_provisioning(
    package: &ProvisioningPackage,
    cipher: &MultiDeviceLinkingKey,
    tx: &mpsc::Sender<RelayFrame>,
    rx: &mut Streaming<RelayFrame>,
) -> Result<SelfGroupJoinRequest, LinkingError> {
    let frame = LinkingMessage::seal(package, cipher)?;
    send_frame(tx, frame, "send the provisioning package").await?;
    info!("sent provisioning package to new device");

    receive_sealed(cipher, rx).await
}

/// The new device's local stores, opened before linking starts.
struct LocalStores {
    air_db: DbAccess,
    client_db: DbAccess,
    global_lock: GlobalLock,
    client_record_id: Uuid,
}

/// The new device's half of the pairing group, and the Welcome that an
/// existing device answered the code with.
struct AnsweredCode {
    provider: OpenMlsRustCrypto,
    welcome: RelayFrame,
}

impl CoreUser {
    /// Provisions a new client for linking by connecting to the relay at `domain`.
    ///
    /// Aborts and removes the new local client database when `cancellation_token` is cancelled
    /// before linking finished.
    ///
    /// On success returns a fully bootstrapped [`CoreUser`] for the freshly
    /// linked device, persisted under `db_path`.
    pub async fn new_linked(
        db_path: &str,
        domain: Fqdn,
        server_url: Option<Url>,
        session_tx: mpsc::Sender<MultiDeviceProvisionStep>,
        cancellation_token: CancellationToken,
    ) -> anyhow::Result<CoreUser> {
        let air_db = open_air_db(db_path).await?;
        let client_record_id = Uuid::new_v4();
        let client_db = open_client_db(db_path, client_record_id).await?;
        let global_lock = open_lock_file(db_path)?;

        let result = tokio::select! {
            biased;
            _ = cancellation_token.cancelled() => Err(anyhow!("multi-device provisioning cancelled")),
            result = Self::new_linked_with_dbs(
                air_db,
                client_db.clone(),
                global_lock,
                client_record_id,
                domain,
                server_url,
                session_tx,
            ) => result,
        };

        if result.is_err() {
            warn!("multi-device linking failed, deleting new client database");
            client_db.close().await;
            Self::roll_back(db_path, client_record_id).await;
        }
        result
    }

    async fn new_linked_with_dbs(
        air_db: DbAccess,
        client_db: DbAccess,
        global_lock: GlobalLock,
        client_record_id: Uuid,
        domain: Fqdn,
        server_url: Option<Url>,
        session_tx: mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> anyhow::Result<CoreUser> {
        let api_clients = ApiClients::new(domain, server_url);

        // Each code gets its own relay call. Dropping an unanswered call ends
        // its session at the relay.
        let (tx, mut rx, answer) = loop {
            let (tx, mut rx) = api_clients
                .default_client()?
                .rs_multi_device_provision_client()
                .await?;
            if let Some(answer) = Self::offer_code(&tx, &mut rx, &session_tx).await? {
                break (tx, rx, answer);
            }
            info!("the linking code expired unanswered, requesting a new one");
        };

        let stores = LocalStores {
            air_db,
            client_db,
            global_lock,
            client_record_id,
        };
        let session =
            Self::provision_session(api_clients, stores, answer, &tx, &mut rx, &session_tx);
        let core_user = Box::pin(session).await?;
        // The confirmation is still queued. Let it out before the call goes
        // away, or the existing device takes us out again.
        close_and_drain(tx, &mut rx).await;
        Ok(core_user)
    }

    /// Deletes the client database of a device whose linking failed.
    async fn roll_back(db_path: &str, client_record_id: Uuid) {
        delete_client_database(db_path, client_record_id)
            .await
            .inspect_err(|error| {
                error!(%error, "failed to delete client database");
            })
            .ok();
    }

    /// Shows a fresh linking code and waits for an existing device to answer
    /// it. Returns `None` when nobody answered within [`CODE_LIFETIME`].
    async fn offer_code(
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
        session_tx: &mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> Result<Option<AnsweredCode>, LinkingError> {
        let (provider, credential_with_key, signature_keys) =
            make_provider_and_credential(b"initiator")?;

        let key_package_bundle = KeyPackage::builder()
            .build(CIPHERSUITE, &provider, &signature_keys, credential_with_key)
            .context("build key package")?;
        let key_package_bytes = MlsMessageOut::from(key_package_bundle)
            .to_bytes()
            .context("serialize key package")?;

        // The relay answers the provisioning request with the session's
        // rendezvous ID, which is the code the user types on the existing
        // device.
        let frame = next_frame(rx).await?;
        let MdlMessage::SessionAssigned(SessionAssigned { rendezvous_id }) =
            MdlMessage::from_frame(&frame).context("malformed linking message")?
        else {
            return Err(LinkingError::Protocol(anyhow!(
                "expected a session assignment from the relay"
            )));
        };

        // The relay holds the key package back until the existing device
        // answers the code.
        send_frame(tx, key_package_bytes.into(), "send the key package").await?;

        session_tx
            .send(MultiDeviceProvisionStep::Code(rendezvous_id))
            .await
            .map_err(|_| anyhow!("reporting stream dropped"))?;

        // wait for the existing (old) client to send us the welcome
        match timeout(CODE_LIFETIME, next_frame(rx)).await {
            Ok(welcome) => Ok(Some(AnsweredCode {
                provider,
                welcome: welcome?,
            })),
            Err(_) => Ok(None),
        }
    }

    async fn provision_session(
        api_clients: ApiClients,
        stores: LocalStores,
        answer: AnsweredCode,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
        session_tx: &mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> Result<CoreUser, LinkingError> {
        let LocalStores {
            air_db,
            client_db,
            global_lock,
            client_record_id,
        } = stores;
        let AnsweredCode {
            provider,
            welcome: welcome_bytes,
        } = answer;

        session_tx
            .send(MultiDeviceProvisionStep::Linking)
            .await
            .map_err(|_| anyhow!("reporting stream dropped"))?;

        let welcome_msg = MlsMessageIn::tls_deserialize_exact(welcome_bytes.as_slice())
            .context("failed to deserialize welcome")?;
        let welcome = match welcome_msg.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => {
                return Err(LinkingError::Protocol(anyhow!(
                    "expected a Welcome message"
                )));
            }
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
        let package: ProvisioningPackage = receive_sealed(&cipher, rx).await?;
        info!("received the provisioning package");

        // Join the self group:
        // 1. mint a new signing key to use for self-group commits
        // 2. generate a self-group KeyPackage
        // 3. hand it to the old device
        // 4. old device adds us via the DS
        // 5. we then process the Welcome that the QS fans out to our fresh queue.
        // 6. the old client gives us enough information to onboard ourselves (the new client) into all existing groups.
        let device_name = package.device_name.clone();
        let core_user = Self::link_new_device(
            api_clients,
            client_db,
            global_lock,
            client_record_id,
            package,
        )
        .await?;
        info!("bootstrapped linked client");

        // The existing device takes the device out of the self group again
        // if it never hears that the join went through.
        Self::join_self_group(&core_user, &device_name, &cipher, tx, rx).await?;

        // Only now does the account become loadable.
        let mut client_record = ClientRecord::new(core_user.user_id().clone(), client_record_id);
        client_record.finish();
        client_record
            .store(air_db.write().await.context("open the air database")?)
            .await
            .context("finish the client record")?;
        core_user.outbound_service().notify_vc_onboarding();

        Ok(core_user)
    }

    /// Hands the existing device a self-group KeyPackage, waits for the
    /// Welcome of the add commit, and confirms the join.
    async fn join_self_group(
        core_user: &CoreUser,
        device_name: &str,
        cipher: &MultiDeviceLinkingKey,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<(), LinkingError> {
        Self::send_self_group_join_request(core_user, device_name, cipher, tx).await?;
        core_user.await_self_group_welcome(rx).await?;
        info!("joined self group");

        // Without this the existing device removes us again, so failing to
        // deliver it fails the link.
        let frame = LinkingMessage::seal(&LinkingComplete, cipher)?;
        send_frame(tx, frame, "confirm the join").await?;
        Ok(())
    }

    /// Hands the existing device a self-group KeyPackage and this device's
    /// entry for the linked-device list.
    async fn send_self_group_join_request(
        core_user: &CoreUser,
        device_name: &str,
        cipher: &MultiDeviceLinkingKey,
        tx: &mpsc::Sender<RelayFrame>,
    ) -> Result<(), LinkingError> {
        let key_package = core_user.generate_self_group_key_package().await?;
        // Store our own entry locally and hand a copy to the old device, which
        // publishes it on the add commit.
        let device = core_user
            .store_own_device_entry(Utc::now(), Some(device_name))
            .await?;
        let frame = LinkingMessage::seal(
            &SelfGroupJoinRequest {
                key_package,
                device,
            },
            cipher,
        )?;
        send_frame(tx, frame, "send the self-group join request").await?;
        info!("sent self-group key package and device entry to old device");
        Ok(())
    }
    /// Waits for the self-group Welcome to show up in this device's queue
    /// while the relay session stays up.
    async fn await_self_group_welcome(
        &self,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<(), LinkingError> {
        let self_group_id = OwnClientInfo::load_self_group_id(
            self.db().read().await.context("open the client store")?,
        )
        .await
        .context("load the self group id")?
        .context("no self group id")?;

        let wait = async {
            loop {
                match self.listen_queue().await {
                    Ok((stream, responder)) => {
                        if self
                            .watch_queue_for_welcome(&self_group_id, stream, responder, rx)
                            .await?
                        {
                            return Ok(());
                        }
                    }
                    Err(ListenQueueError::Qs(error))
                        if matches!(error.kind(), RequestErrorKind::NotFound) =>
                    {
                        return Err(LinkingError::QueueDeleted);
                    }
                    Err(error) => {
                        warn!(%error, "could not listen to the queue while waiting for the welcome");
                    }
                }
                tokio::select! {
                    biased;
                    frame = next_frame(rx) => return Err(relay_interrupted(frame)),
                    () = tokio::time::sleep(QUEUE_REOPEN_DELAY) => {}
                }
            }
        };
        tokio::time::timeout(WELCOME_TIMEOUT, wait)
            .await
            .unwrap_or(Err(LinkingError::TimedOut("the self-group welcome")))
    }

    /// Processes the queue as it fills until the self group shows up.
    ///
    /// Returns whether this device joined. `false` means the queue stream
    /// ended and has to be reopened.
    async fn watch_queue_for_welcome(
        &self,
        self_group_id: &GroupId,
        mut stream: impl Stream<Item = Result<ListenResponse, tonic::Status>> + Unpin,
        responder: QsListenResponder,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<bool, LinkingError> {
        let mut messages: Vec<QueueMessage> = Vec::new();
        loop {
            // The existing device sends nothing more until it has our
            // confirmation, so anything on the relay ends the session.
            let event = tokio::select! {
                biased;
                frame = next_frame(rx) => return Err(relay_interrupted(frame)),
                event = stream.next() => event,
            };
            let event = match event {
                Some(Ok(response)) => response.event,
                Some(Err(error)) => {
                    warn!(%error, "qs listen stream failed while waiting for the welcome");
                    return Ok(false);
                }
                None => return Ok(false),
            };
            match event {
                Some(listen_response::Event::Message(queue_message)) => {
                    if let Ok(queue_message) = queue_message.try_into() {
                        messages.push(queue_message);
                    }
                }
                // Empty event is the sentinel: everything queued so far is here.
                Some(listen_response::Event::Empty(_)) => {
                    let processed = self
                        .process_and_ack_qs_messages(std::mem::take(&mut messages), &responder)
                        .await;
                    for error in &processed.errors {
                        warn!(%error, "error while processing self-group queue message");
                    }
                    if self.has_joined_self_group(self_group_id).await? {
                        // Wait for the server to apply the acks.
                        responder.close(&mut stream).await;
                        return Ok(true);
                    }
                }
                Some(listen_response::Event::Payload(_))
                | Some(listen_response::Event::VersionStatus(_))
                | Some(listen_response::Event::SiblingClientState(_))
                | None => {}
            }
        }
    }

    /// Whether the self group is in the store. Processing its Welcome puts it
    /// there.
    async fn has_joined_self_group(&self, self_group_id: &GroupId) -> anyhow::Result<bool> {
        Ok(self
            .db()
            .with_read_transaction(async |txn| Group::load(txn, self_group_id).await)
            .await?
            .is_some())
    }

    /// Establishes a session with the new device that shows the linking
    /// `code`. The `connected_tx` and `confirmation_rx` are channels to report
    /// established connection and wait for the user's confirmation (with a
    /// device name).
    pub async fn multi_device_link_client(
        &self,
        code: String,
        connected_tx: oneshot::Sender<()>,
        confirmation_rx: oneshot::Receiver<String>,
    ) -> anyhow::Result<Result<(), MultiDeviceLinkClientError>> {
        let Ok(rendezvous_id) = RendezVousId::new(code) else {
            warn!("rejected a malformed linking code");
            return Ok(Err(MultiDeviceLinkClientError::InvalidCode));
        };
        let max_devices = self.max_devices().await?;
        let devices = u32::try_from(self.self_group_client_ids().await?.len()).unwrap_or(u32::MAX);
        if max_devices > 0 && devices >= max_devices {
            return Ok(Err(MultiDeviceLinkClientError::DeviceLimitReached {
                max_devices,
            }));
        }

        let client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;
        let qs_user_signing_key = self.key_store().qs_user_signing_key.clone();

        let (tx, mut rx) = match client
            .rs_multi_device_link_client(qs_user_id, &qs_user_signing_key, rendezvous_id)
            .await
        {
            Ok(streams) => streams,
            Err(RsRequestError::SessionNotFound) => {
                return Ok(Err(MultiDeviceLinkClientError::SessionNotFound));
            }
            Err(error) => return Err(error.into()),
        };

        // Signal that we're connected to the relay
        let _ = connected_tx.send(());

        let session = self.link_session(&tx, &mut rx, confirmation_rx);
        match Box::pin(session).await {
            Ok(()) => Ok(Ok(())),
            Err(LinkingError::DeviceLimitReached { max_devices }) => {
                Ok(Err(MultiDeviceLinkClientError::DeviceLimitReached {
                    max_devices,
                }))
            }
            Err(error) => Err(error.into()),
        }
    }

    async fn link_session(
        &self,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
        confirmation_rx: oneshot::Receiver<String>,
    ) -> Result<(), LinkingError> {
        let key_package_bytes = next_frame(rx).await?;

        let (provider, credential_with_key, signature_keys) =
            make_provider_and_credential(b"existing-client")?;

        let candidate_key_package =
            match MlsMessageIn::tls_deserialize_exact(key_package_bytes.as_slice())
                .context("deserialize key package")?
                .extract()
            {
                MlsMessageBodyIn::KeyPackage(kp) => kp
                    .validate(provider.crypto(), ProtocolVersion::Mls10)
                    .context("validate key package")?,
                _ => {
                    return Err(LinkingError::Protocol(anyhow!(
                        "expected a KeyPackage in first relay frame"
                    )));
                }
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
            .commit_builder()
            .propose_adds([candidate_key_package])
            .load_psks(provider.storage())
            .context("load psks")?
            .build(provider.rand(), provider.crypto(), &signature_keys, |_| {
                true
            })
            .context("build commit")?
            .stage_commit(&provider)
            .context("stage commit")?
            .into_messages();
        let welcome = welcome.context("no welcome after add")?;

        group
            .merge_pending_commit(&provider)
            .context("merge pending commit")?;

        let welcome_bytes = welcome.to_bytes().context("serialize welcome")?;
        send_frame(tx, welcome_bytes.into(), "send the welcome").await?;

        // Wait for the user to approve the link on this (existing) device.
        let device_name = confirmation_rx
            .await
            .context("confirmation channel closed")?;

        let cipher = export_aead_key(group, provider)?;
        info!("secure linking channel established, AEAD key exported");

        // Build the provisioning package (this creates a fresh queue for the
        // new device). From here on the device link undoes everything if the
        // link does not complete, even if this device goes away meanwhile.
        let package = self.build_provisioning_package(device_name).await?;
        let queue = ProvisionedQueue {
            qs_client_id: package.qs_client_id,
            qs_client_signing_key: package.qs_client_signing_key.clone(),
        };
        let link_id = DeviceLink::create(
            self.db().write().await.context("open the client store")?,
            queue,
            link_deadline(),
        )
        .await
        .context("record the device link")?;

        let linked = self.complete_link(link_id, &package, &cipher, tx, rx).await;
        if linked.is_err() {
            self.abandon_link(link_id).await;
        }
        linked
    }

    /// Hands over the provisioning package, has the outbound service add the
    /// new device to the self group, and waits for the new device to confirm.
    async fn complete_link(
        &self,
        link_id: Uuid,
        package: &ProvisioningPackage,
        cipher: &MultiDeviceLinkingKey,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<(), LinkingError> {
        let request = exchange_provisioning(package, cipher, tx, rx).await?;
        let new_client_id = request.device.client_id;
        DeviceLink::request_add(
            self.db().write().await.context("open the client store")?,
            link_id,
            request,
            link_deadline(),
        )
        .await?;
        self.outbound_service().notify_pending_chat_operations();
        let early_confirmation = self.await_device_added(link_id, new_client_id, rx).await?;
        info!("added new device to self group");
        self.outbound_service().notify_key_package_upload();

        // The new device confirms once it has processed the Welcome from
        // its queue. Without the confirmation it is taken out again, so the
        // user's retry starts clean.
        let confirmation = match early_confirmation {
            Some(frame) => frame,
            None => tokio::time::timeout(COMPLETION_TIMEOUT, next_frame(rx))
                .await
                .unwrap_or(Err(LinkingError::TimedOut(
                    "the new device to confirm the join",
                )))?,
        };
        let LinkingComplete = LinkingMessage::open(confirmation.as_slice(), cipher)?;
        DeviceLink::complete(
            self.db().write().await.context("open the client store")?,
            link_id,
        )
        .await?;
        info!("linking completed");

        Ok(())
    }

    /// Waits for the outbound service to add the new device to the self
    /// group.
    ///
    /// Returns the new device's confirmation if it arrived early. The new
    /// device confirms as soon as it processed the Welcome, which can be
    /// before this device sees its own add.
    async fn await_device_added(
        &self,
        link_id: Uuid,
        client_id: Uuid,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<Option<RelayFrame>, LinkingError> {
        let wait = async {
            let mut early_confirmation = None;
            loop {
                if self.self_group_client_ids().await?.contains(&client_id) {
                    return Ok(early_confirmation);
                }
                let link = DeviceLink::load(
                    self.db().read().await.context("open the client store")?,
                    link_id,
                )
                .await
                .context("load the device link")?
                .context("the device link is gone")?;
                match link.state {
                    DeviceLinkState::Adding => (),
                    DeviceLinkState::Failed => {
                        return Err(match link.failure {
                            Some(DeviceLinkFailure::DeviceLimitReached { max_devices }) => {
                                LinkingError::DeviceLimitReached { max_devices }
                            }
                            Some(DeviceLinkFailure::Rejected) | None => LinkingError::Protocol(
                                anyhow!("the DS refused to add the new device"),
                            ),
                        });
                    }
                    DeviceLinkState::Provisioned | DeviceLinkState::Abandoned => {
                        return Err(LinkingError::Protocol(anyhow!(
                            "the device link was abandoned"
                        )));
                    }
                }

                // The confirmation is the only frame the new device sends
                // from here on.
                if early_confirmation.is_some() {
                    tokio::time::sleep(ADD_POLL_INTERVAL).await;
                    continue;
                }
                tokio::select! {
                    biased;
                    frame = next_frame(rx) => early_confirmation = Some(frame?),
                    () = tokio::time::sleep(ADD_POLL_INTERVAL) => {}
                }
            }
        };
        tokio::time::timeout(ADD_TIMEOUT, wait)
            .await
            .unwrap_or(Err(LinkingError::TimedOut("the self-group add")))
    }

    /// Leaves an unfinished link to the outbound service to undo.
    async fn abandon_link(&self, link_id: Uuid) {
        let abandoned = async {
            DeviceLink::abandon(self.db().write().await?, link_id).await?;
            anyhow::Ok(())
        };
        // The link's deadline still catches a link that could not be marked.
        if let Err(error) = abandoned.await {
            error!(%error, "failed to abandon the device link");
        }
        self.outbound_service().notify_pending_chat_operations();
    }

    /// Create a fresh QS queue for a new device and gather all the key material
    /// the new device needs to bootstrap a working [`CoreUser`] and join the
    /// self group.
    async fn build_provisioning_package(
        &self,
        device_name: String,
    ) -> anyhow::Result<ProvisioningPackage> {
        let api_client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;

        let self_group = Box::pin(self.ensure_self_group()).await?;
        let self_group_id = self_group.group_id().clone();

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
        let groups = self.higher_level_groups().await?;

        // Snapshot the current synced settings so the new device starts with
        // our values. An empty update means we have no stored settings, which
        // the new device applies as a no-op.
        let synced_settings = self
            .db()
            .with_write_transaction(async |txn| SettingsUpdate::collect(txn).await)
            .await?;
        let token_seeds = privacy_pass::committed_seeds(self.db().read().await?).await?;
        let blocked_contacts = blocked_contacts_snapshot(self.db().read().await?).await?;
        let redeemed_tokens =
            privacy_pass::redeemed_tokens_snapshot(self.db().read().await?).await?;
        let usernames = usernames_snapshot(self.db().read().await?).await?;
        let connection_requests = self
            .db()
            .with_read_transaction(async |txn| requests_snapshot(txn).await)
            .await?;

        Ok(ProvisioningPackage {
            user_signing_key: key_store.signing_key.clone(),
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
            synced_settings,
            token_seeds,
            blocked_contacts,
            redeemed_tokens,
            usernames,
            connection_requests,
            device_name,
            groups,
        })
    }

    /// Describe every higher-level group the virtual client is a member of, so a
    /// joining emulator client can onboard itself into each of them.
    ///
    /// Skips the emulation group itself, incoming connection requests, and every
    /// chat that is not active. A pending chat is one whose onboarding has not
    /// landed, so its leaf is not ours to hand on. An incoming request has no
    /// group yet and is transferred over the provisioning package instead.
    async fn higher_level_groups(&self) -> anyhow::Result<Vec<HigherLevelGroup>> {
        self.db()
            .with_read_transaction(async |txn| -> anyhow::Result<_> {
                let key_material = Group::load_all_key_material(&mut *txn).await?;
                let mut groups = Vec::new();

                let mut chats = Vec::new();
                for group in key_material {
                    let Ok(chat_id) = ChatId::try_from(&group.group_id) else {
                        warn!(group_id = ?group.group_id, "group id is not a chat id; skipping group");
                        continue;
                    };
                    let Some(chat) = Chat::load(&mut *txn, &chat_id).await? else {
                        continue;
                    };
                    if !matches!(chat.status(), ChatStatus::Active) {
                        debug!(group_id = ?group.group_id, "skipping non-active chat");
                        continue;
                    }
                    chats.push((group, chat));
                }

                // Sort chats in ascending order by last message date: this determines onboarding
                // order by the resync queue.
                //
                // Since a system message is inserted after each onboarding, the presented list
                // should look similar to the one in the original device.
                chats.sort_unstable_by_key(|(_, chat)| chat.last_message_at);

                for (group, chat) in chats {
                    let (connection, outgoing_request) = match chat.chat_type() {
                        ChatType::Group(_) => (None, None),
                        ChatType::Connection(user_id) => {
                            let Some(contact) = Contact::load(&mut *txn, user_id).await? else {
                                warn!(group_id = ?group.group_id, "no contact for connection chat; skipping group");
                                continue;
                            };
                            let connection = ConnectionContact {
                                user_id: contact.user_id,
                                wai_ear_key: contact.wai_ear_key,
                                friendship_token: contact.friendship_token,
                            };
                            (Some(connection), None)
                        }
                        ChatType::HandleConnection(_) | ChatType::TargetedMessageConnection(_) => {
                            let Some(request) = OutgoingRequest::load(&mut *txn, &chat).await?
                            else {
                                warn!(group_id = ?group.group_id, "no partial contact for outgoing request; skipping group");
                                continue;
                            };
                            (None, Some(request))
                        }
                        ChatType::PendingConnection(_) => {
                            debug!(group_id = ?group.group_id, "skipping incoming connection request");
                            continue;
                        }
                    };
                    let Some(vc_leaf_index) =
                        Group::load_own_leaf_index(txn.as_mut(), &group.group_id)
                    else {
                        warn!(group_id = ?group.group_id, "no own leaf index; skipping group");
                        continue;
                    };

                    groups.push(HigherLevelGroup {
                        group_id: group.group_id,
                        pq_group_id: group.pq_group_id,
                        group_state_ear_key: group.group_state_ear_key,
                        identity_link_wrapper_key: group.identity_link_wrapper_key,
                        vc_leaf_index: vc_leaf_index.u32(),
                        connection,
                        outgoing_request,
                    });
                }

                Ok(groups)
            })
            .await
    }

    /// The signing key used for this client's leaf in the self group.
    async fn self_group_signature_key(&self) -> anyhow::Result<SelfGroupSigningKey> {
        let stored: OwnClientInfo = OwnClientInfo::load(self.db().read().await?).await?;
        stored
            .self_group_signing_key
            .context("self-group signer was not initialized")
    }

    /// Generate an APQ KeyPackage for this (freshly linked) device to be added
    /// to the self group.
    async fn generate_self_group_key_package(&self) -> anyhow::Result<ApqKeyPackage> {
        let signer = self.self_group_signature_key().await?;
        // Both T and PQ leaves use this device's signature key (the PQ side is
        // confidentiality-only), which is what the DS expects.
        let signature_key = SignaturePublicKey::from(signer.verifying_key().clone());
        let credential = ApqCredentialWithKey {
            t_credential: CredentialWithKey {
                credential: signer.credential().to_credential()?,
                signature_key: signature_key.clone(),
            },
            pq_credential: CredentialWithKey {
                credential: Credential::new(CredentialType::Basic, Vec::new()),
                signature_key,
            },
        };

        let mut leaf_node_extensions = ClientAppData::current().leaf_node_extensions();
        let client_reference = self.create_own_client_reference();
        leaf_node_extensions.add(client_reference_extension(&client_reference)?)?;
        // add two fields AirComponent Option<QsClientId> and Option<QsUserId>
        let key_package_extensions = ClientAppData::current().key_package_extensions();

        self.db()
            .with_write_transaction(async |txn| -> anyhow::Result<_> {
                let provider = AirOpenMlsProvider::new(txn.as_mut());
                let bundle = ApqKeyPackage::builder()
                    .key_package_extensions(key_package_extensions)
                    .leaf_node_capabilities(self_group_leaf_node_capabilities())
                    .leaf_node_extensions(leaf_node_extensions)
                    .build(&provider, APQ_CIPHERSUITE, &signer, credential)?;
                Ok(bundle.into_key_package())
            })
            .await
    }

    /// Fully processes `messages` and ACKs them once all of them went
    /// through.
    async fn process_and_ack_qs_messages(
        &self,
        messages: Vec<QueueMessage>,
        responder: &QsListenResponder,
    ) -> ProcessedQsMessages {
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
        processed
    }

    /// Bootstrap a [`CoreUser`] on a freshly linked device from the
    /// provisioning package received over the secure linking channel.
    ///
    /// Onboarding into the virtual client's higher-level groups is queued here,
    /// but only performed once we joined the emulation group: the outbound
    /// service defers a queued onboarding until the self group is there.
    async fn link_new_device(
        api_clients: ApiClients,
        client_db: DbAccess,
        global_lock: GlobalLock,
        client_record_id: Uuid,
        package: ProvisioningPackage,
    ) -> anyhow::Result<CoreUser> {
        let ProvisioningPackage {
            user_signing_key,
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
            synced_settings,
            token_seeds,
            blocked_contacts,
            redeemed_tokens,
            usernames,
            connection_requests,
            device_name: _,
            groups,
        } = package;

        let shared_user_credential = user_signing_key.credential().clone();
        let key_store = MemoryUserKeyStore {
            signing_key: user_signing_key,
            qs_client_signing_key,
            qs_user_signing_key,
            qs_queue_decryption_key,
            push_token_ear_key,
            friendship_token,
            wai_ear_key,
            qs_client_id_encryption_key,
        };

        // Each linked device mints its own client id and a per-device self-group signing key.
        let user_id = key_store.signing_key.credential().user_id().clone();
        let client_id = Uuid::new_v4();
        let self_group_signing_key = SelfGroupSigningKey::generate(client_id)?;

        let queued = client_db
            .with_write_transaction(async |txn| -> anyhow::Result<usize> {
                StorableUserCredential::new(key_store.signing_key.credential().clone())
                    .store(&mut *txn)
                    .await?;
                StorableQsQueueRatchet::initialize(&mut *txn, qs_initial_ratchet_secret).await?;
                user_profile_key.store_own(&mut *txn).await?;

                OwnClientInfo {
                    qs_user_id,
                    qs_client_id,
                    user_id: user_id.clone(),
                    client_id,
                    self_group_id: Some(self_group_id),
                    self_group_signing_key: Some(self_group_signing_key),
                }
                .store(&mut *txn)
                .await?;

                // Schedule the fetching operation of our own profile information for when the [`CoreClient`]
                // starts (or more specifically, when the outbound service runs for the first time.)
                Self::schedule_fetch_user_profile(
                    &mut *txn,
                    (shared_user_credential, user_profile_key),
                )
                .await?;

                // Seed the synced settings and the token seeds before the
                // device processes any self-group traffic. A device joining via
                // Welcome cannot decrypt commits from before its join, so the
                // current state has to arrive in the linking payload.
                apply_settings_update(txn, &synced_settings).await?;
                privacy_pass::store_provisioned_seeds(txn, &token_seeds).await?;
                apply_blocked_contacts_update(txn, &blocked_contacts).await?;
                privacy_pass::apply_redeemed_tokens(txn, &redeemed_tokens).await?;
                apply_usernames_update(txn, &usernames).await?;
                store_provisioned_requests(txn, &connection_requests, Utc::now()).await?;
                skip_signed_connection_package_upload(&mut *txn).await?;

                // Queue the onboarding into the groups the virtual client is
                // already a member of.
                Self::enqueue_vc_onboarding(txn, groups).await
            })
            .await?;
        info!(
            queued,
            "queued onboarding into existing higher-level groups"
        );

        let final_state = UserCreationState::FinalUserState(
            QsRegisteredUserState::new(key_store, qs_user_id, qs_client_id)
                .persist()
                .await?,
        );
        final_state.store(client_db.write().await?).await?;

        Ok(final_state.final_state()?.into_self_user(
            client_db,
            client_record_id,
            api_clients,
            global_lock,
        ))
    }

    /// Returns why this device can no longer use the account, or `None` while it can.
    pub async fn account_unlink_reason(&self) -> anyhow::Result<Option<UnlinkReason>> {
        OwnClientInfo::account_unlink_reason(self.db().read().await?).await
    }
}
