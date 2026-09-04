// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! The multi-device linking protocol.
//!
//! A new device opens a rendezvous session at the relay, shows the user a
//! linking code, and runs a CPace exchange over that code with the existing
//! device the user types it into. The exchange yields an external PSK that
//! goes into the key schedule of an ephemeral two-member MLS group, and the
//! account material travels as application messages inside that group.

pub(crate) mod device_link;
mod pairing;
mod payloads;
#[cfg(test)]
mod tests;

use airapiclient::{qs_api::QsListenResponder, rs_api::RsRequestError};
use aircommon::codec::PersistenceCodec;
use aircommon::credentials::keys::SelfGroupSigningKey;
use aircommon::crypto::RatchetDecryptionKey;
use aircommon::crypto::indexed_aead::keys::UserProfileKey;
use aircommon::crypto::kdf::keys::RatchetSecret;
use aircommon::crypto::mdl::code::LinkingCode;
use aircommon::crypto::mdl::pake::{self, MdlInitiator};
use aircommon::crypto::signatures::keys::QsClientSigningKey;
use aircommon::identifiers::Fqdn;
use aircommon::messages::QueueMessage;
use aircommon::mls_group_config::{
    APQ_CIPHERSUITE, QS_CLIENT_REFERENCE_EXTENSION_TYPE, self_group_leaf_node_capabilities,
};
use airprotos::client::app_data::ClientAppData;
use airprotos::client::self_group::SettingsUpdate;
use airprotos::relay_service::mdl::{
    Abort, AbortCode, LinkingPayloadType, MDL_INITIATOR_LABEL, MDL_PROTOCOL_VERSION, MdlContext,
    MdlKdfContext, MdlMessage, PakeShareA, PakeShareB, SessionAssigned,
};
use airprotos::relay_service::v1::{RelayFrame, RendezvousId};
use anyhow::{Context, anyhow};
use apqmls::authentication::ApqCredentialWithKey;
use apqmls::messages::ApqKeyPackage;
use chrono::{DateTime, Utc};
use openmls::group::GroupId;
use openmls::prelude::{Credential, CredentialType, SignaturePublicKey};
use openmls::prelude::{CredentialWithKey, Extension, UnknownExtension};
use rand::TryRng;
use std::time::Duration;
use tls_codec::{Serialize as _, VLByteSlice};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::{Stream, StreamExt};
use tonic::Streaming;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatStatus, ChatType, Contact,
    clients::{
        CoreUser, ListenResponse,
        api_clients::ApiClients,
        block_contact::persistence::{apply_blocked_contacts_update, blocked_contacts_snapshot},
        create_user::QsRegisteredUserState,
        listen_response,
        own_client_info::OwnClientInfo,
        process::process_qs::ProcessedQsMessages,
        store::{ClientRecord, UserCreationState},
        user_settings::{SettingsUpdateExt, apply_settings_update},
    },
    delete_client_database,
    groups::{
        Group, client_auth_info::StorableUserCredential, openmls_provider::AirOpenMlsProvider,
    },
    key_stores::{
        MemoryUserKeyStore, indexed_keys::StorableIndexedKey,
        queue_ratchets::StorableQsQueueRatchet,
    },
    privacy_pass,
    utils::persistence::{open_air_db, open_client_db, open_lock_file},
};

use device_link::{DeviceLink, DeviceLinkFailure, DeviceLinkState, ProvisionedQueue};
use pairing::{PairingGroup, PairingIdentity};

pub(crate) use payloads::{ConnectionContact, HigherLevelGroup};
use payloads::{ProvisioningPackage, SelfGroupJoinRequest};

/// Bytes of the CPace session id the new device draws per session.
const SID_LEN: usize = 16;

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
    /// The code was too short.
    #[error("the linking code is malformed")]
    InvalidCode,
    #[error("device limit reached: max = {max_devices}")]
    DeviceLimitReached { max_devices: u32 },
}

/// Why a linking session ended badly.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LinkingError {
    /// The pairing group's Welcome did not open, so the two devices derived
    /// different PSKs. Either the code was mistyped, or somebody tried to
    /// intercept the linking.
    #[error("authentication failed, the linking code was wrong or the session was intercepted")]
    AuthenticationFailed,
    #[error("linking protocol error")]
    Protocol(#[source] anyhow::Error),
    #[error("linking validation failed: {0}")]
    Validation(String),
    #[error("the user declined the link")]
    UserRejected,
    /// The peer did not take its next step in time.
    #[error("timed out waiting for {0}")]
    TimedOut(&'static str),
    #[error("the peer aborted the link: {0:?}")]
    PeerAborted(AbortCode),
    /// The new device speaks a version this device does not.
    #[error("unsupported linking protocol version {0}")]
    UnsupportedVersion(u16),
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

impl LinkingError {
    pub(crate) fn validation(reason: impl Into<String>) -> Self {
        Self::Validation(reason.into())
    }

    /// The code to send the peer, or `None` when the peer ended the session
    /// itself and there is nobody left to tell.
    fn abort_code(&self) -> Option<AbortCode> {
        match self {
            Self::AuthenticationFailed => Some(AbortCode::AuthenticationFailed),
            Self::Protocol(_) => Some(AbortCode::ProtocolError),
            Self::Validation(_) => Some(AbortCode::ValidationFailed),
            Self::UserRejected => Some(AbortCode::UserRejected),
            Self::TimedOut(_) => Some(AbortCode::Timeout),
            Self::PeerAborted(_) => None,
            Self::UnsupportedVersion(_) => Some(AbortCode::UnsupportedVersion),
            Self::SessionClosed(_) => None,
            Self::DeviceLimitReached { .. } => None,
            Self::QueueDeleted => None,
        }
    }
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

/// Reads the next linking message off the relay stream.
async fn next_message(rx: &mut Streaming<RelayFrame>) -> Result<MdlMessage, LinkingError> {
    let frame = match rx.next().await {
        Some(Ok(frame)) => frame,
        Some(Err(status)) => {
            return Err(LinkingError::SessionClosed(
                anyhow::Error::from(status).context("the linking stream failed"),
            ));
        }
        None => {
            return Err(LinkingError::SessionClosed(anyhow!(
                "the relay closed the linking session"
            )));
        }
    };
    Ok(MdlMessage::from_frame(&frame).context("malformed linking message")?)
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

/// The error for a relay message, or the relay's loss, that arrived while
/// the peer was meant to stay quiet.
fn relay_interrupted(message: Result<MdlMessage, LinkingError>) -> LinkingError {
    match message {
        Ok(message) => unexpected(&message),
        Err(error) => error,
    }
}

/// Looks for the peer's abort in what is left of a call that ended under us.
async fn pending_abort(rx: &mut Streaming<RelayFrame>) -> Option<AbortCode> {
    let drain = async {
        while let Some(Ok(frame)) = rx.next().await {
            if let Ok(MdlMessage::Abort(Abort { code })) = MdlMessage::from_frame(&frame) {
                return Some(code);
            }
        }
        None
    };
    tokio::time::timeout(TEARDOWN_TIMEOUT, drain)
        .await
        .ok()
        .flatten()
}

/// Turns a session that ended under us into the peer's abort when there is
/// one, otherwise leaves the error alone.
async fn resolve_closed_session(
    error: LinkingError,
    rx: &mut Streaming<RelayFrame>,
) -> LinkingError {
    match error {
        LinkingError::SessionClosed(_) => match pending_abort(rx).await {
            Some(code) => LinkingError::PeerAborted(code),
            None => error,
        },
        other => other,
    }
}

/// Rejects a new device that speaks another version before any PAKE work is
/// done on its share.
fn check_version(version: u16) -> Result<(), LinkingError> {
    if version == MDL_PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(LinkingError::UnsupportedVersion(version))
    }
}

/// Turns a message that does not belong at this point of the flow into the
/// error the caller should abort with.
fn unexpected(message: &MdlMessage) -> LinkingError {
    match message {
        MdlMessage::Abort(Abort { code }) => LinkingError::PeerAborted(*code),
        other => LinkingError::Protocol(anyhow!("unexpected linking message {}", other.kind())),
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

/// Tells the peer why the session ended, then waits for the relay to let go
/// of the call.
async fn send_abort(tx: mpsc::Sender<RelayFrame>, rx: &mut Streaming<RelayFrame>, code: AbortCode) {
    let Ok(frame) = MdlMessage::Abort(Abort { code }).into_frame() else {
        return;
    };
    if tokio::time::timeout(TEARDOWN_TIMEOUT, tx.send(frame))
        .await
        .is_ok_and(|sent| sent.is_ok())
    {
        close_and_drain(tx, rx).await;
    } else {
        debug!(?code, "the linking abort could not be delivered");
    }
}

/// Reads one linking payload of the expected type out of the pairing group.
async fn receive_payload(
    pairing: &mut PairingGroup,
    rx: &mut Streaming<RelayFrame>,
    expected: LinkingPayloadType,
) -> Result<Vec<u8>, LinkingError> {
    let message = next_message(rx).await?;
    let MdlMessage::GroupMessage(group_message) = message else {
        return Err(unexpected(&message));
    };
    let payload = pairing.receive(&group_message)?;
    if payload.payload_type != expected {
        return Err(LinkingError::Protocol(anyhow!(
            "expected the linking payload {expected:?}, got {:?}",
            payload.payload_type
        )));
    }
    Ok(payload.payload)
}

/// Hands the new device the provisioning package and reads back its
/// self-group join request.
async fn exchange_provisioning(
    package: &ProvisioningPackage,
    pairing: &mut PairingGroup,
    tx: &mpsc::Sender<RelayFrame>,
    rx: &mut Streaming<RelayFrame>,
) -> Result<SelfGroupJoinRequest, LinkingError> {
    let encoded = PersistenceCodec::to_vec(package).context("encode the provisioning package")?;
    let frame = pairing.send(LinkingPayloadType::ProvisioningPackage, encoded)?;
    send_frame(tx, frame, "send the provisioning package").await?;
    info!("sent provisioning package to new device");

    let request = receive_payload(pairing, rx, LinkingPayloadType::SelfGroupJoinRequest).await?;
    Ok(PersistenceCodec::from_slice(&request).context("decode the self-group join request")?)
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
        session_tx: mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> anyhow::Result<CoreUser> {
        let api_clients = ApiClients::new(domain.clone(), server_url);
        let (tx, mut rx) = api_clients
            .default_client()?
            .rs_multi_device_provision_client()
            .await?;

        let session =
            Self::provision_session(api_clients, db_path, &domain, &tx, &mut rx, &session_tx);
        match Box::pin(session).await {
            Ok(core_user) => {
                // The confirmation is still queued. Let it out before the
                // call goes away, or the existing device takes us out again.
                close_and_drain(tx, &mut rx).await;
                Ok(core_user)
            }
            Err(error) => {
                let error = resolve_closed_session(error, &mut rx).await;
                if let Some(code) = error.abort_code() {
                    send_abort(tx, &mut rx, code).await;
                }
                Err(error.into())
            }
        }
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

    async fn provision_session(
        api_clients: ApiClients,
        db_path: &str,
        domain: &Fqdn,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
        session_tx: &mpsc::Sender<MultiDeviceProvisionStep>,
    ) -> Result<CoreUser, LinkingError> {
        let message = next_message(rx).await?;
        let MdlMessage::SessionAssigned(SessionAssigned { rendezvous_id }) = message else {
            return Err(unexpected(&message));
        };

        let code = LinkingCode::generate(&rendezvous_id).map_err(|_| {
            LinkingError::validation("the relay assigned a malformed rendezvous id")
        })?;
        let ci = MdlContext::new(domain.to_string(), rendezvous_id)
            .tls_serialize_detached()
            .context("serialize the linking context")?;

        let mut sid = [0u8; SID_LEN];
        rand::rng().try_fill_bytes(&mut sid);

        let identity = PairingIdentity::new(MDL_INITIATOR_LABEL)?;
        let key_package = identity.key_package()?;
        let initiator = MdlInitiator::start(code.password(), &ci, &sid, &key_package);
        let msg_a = initiator.msg_a().to_vec();

        let share = MdlMessage::PakeShareA(PakeShareA {
            version: MDL_PROTOCOL_VERSION,
            sid: sid.to_vec(),
            msg_a: msg_a.clone(),
        })
        .into_frame()
        .context("frame the pake share")?;
        send_frame(tx, share, "send the pake share").await?;

        session_tx
            .send(MultiDeviceProvisionStep::Code(code.to_digits()))
            .await
            .map_err(|_| anyhow!("the provisioning reporting stream was dropped"))?;

        let message = next_message(rx).await?;
        let MdlMessage::PakeShareB(PakeShareB { msg_b, welcome }) = message else {
            return Err(unexpected(&message));
        };

        session_tx
            .send(MultiDeviceProvisionStep::Linking)
            .await
            .map_err(|_| anyhow!("the provisioning reporting stream was dropped"))?;

        let isk = initiator
            .finish(&msg_b)
            .map_err(|error| LinkingError::Protocol(error.into()))?;
        let kdf_ctx = MdlKdfContext {
            ci: VLByteSlice(&ci),
            sid: VLByteSlice(&sid),
            msg_a: VLByteSlice(&msg_a),
            msg_b: VLByteSlice(&msg_b),
        }
        .tls_serialize_detached()
        .context("serialize the linking kdf context")?;
        let psk = isk.derive_psk(&kdf_ctx);

        let mut pairing = PairingGroup::join(identity, &psk, &welcome)?;
        drop(psk);
        info!("joined the pairing group");

        // The existing device creates nothing for us before it hears this.
        let frame = pairing.send(LinkingPayloadType::PairingConfirmed, Vec::new())?;
        send_frame(tx, frame, "confirm the pairing").await?;

        let package =
            receive_payload(&mut pairing, rx, LinkingPayloadType::ProvisioningPackage).await?;
        let package: ProvisioningPackage =
            PersistenceCodec::from_slice(&package).context("decode the provisioning package")?;
        info!("received the provisioning package");

        // Join the self group:
        // 1. mint a new signing key to use for self-group commits
        // 2. generate a self-group KeyPackage
        // 3. hand it to the old device
        // 4. old device adds us via the DS
        // 5. we then process the Welcome that the QS fans out to our fresh queue.
        // 6. the old client gives us enough information to onboard ourselves (the new client) into all existing groups.
        let device_name = package.device_name.clone();
        let core_user = Self::link_new_device(api_clients, db_path, package).await?;
        info!("bootstrapped linked client");

        // Every failure from here on rolls the device back, so a retry
        // starts clean. The existing device takes the device out of the
        // self group again if it never hears that the join went through.
        let joined = Self::join_self_group(&core_user, &device_name, &mut pairing, tx, rx).await;
        if let Err(error) = joined {
            let client_record_id = core_user.client_record_id();
            drop(core_user);
            Self::roll_back(db_path, client_record_id).await;
            return Err(error);
        }
        core_user.outbound_service().notify_vc_onboarding();

        Ok(core_user)
    }

    /// Hands the existing device a self-group KeyPackage, waits for the
    /// Welcome of the add commit, and confirms the join.
    async fn join_self_group(
        core_user: &CoreUser,
        device_name: &str,
        pairing: &mut PairingGroup,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<(), LinkingError> {
        Self::send_self_group_join_request(core_user, device_name, pairing, tx).await?;
        core_user.await_self_group_welcome(rx).await?;
        info!("joined self group");

        // Without this the existing device removes us again, so failing to
        // deliver it fails the link.
        let frame = pairing.send(LinkingPayloadType::LinkingComplete, Vec::new())?;
        send_frame(tx, frame, "confirm the join").await?;
        Ok(())
    }

    /// Hands the existing device a self-group KeyPackage and this device's
    /// entry for the linked-device list.
    async fn send_self_group_join_request(
        core_user: &CoreUser,
        device_name: &str,
        pairing: &mut PairingGroup,
        tx: &mpsc::Sender<RelayFrame>,
    ) -> Result<(), LinkingError> {
        let key_package = core_user.generate_self_group_key_package().await?;
        // Store our own entry locally and hand a copy to the old device, which
        // publishes it on the add commit.
        let device = core_user
            .store_own_device_entry(Utc::now(), Some(device_name))
            .await?;
        let request = PersistenceCodec::to_vec(&SelfGroupJoinRequest {
            key_package,
            device,
        })
        .context("encode the self-group join request")?;
        let frame = pairing.send(LinkingPayloadType::SelfGroupJoinRequest, request)?;
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
                    Err(error) if error.is_unknown_client() => {
                        return Err(LinkingError::QueueDeleted);
                    }
                    Err(error) => {
                        warn!(%error, "could not listen to the queue while waiting for the welcome");
                    }
                }
                tokio::select! {
                    biased;
                    message = next_message(rx) => return Err(relay_interrupted(message)),
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
                message = next_message(rx) => return Err(relay_interrupted(message)),
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

    /// Answers a new device's linking `code` on this (existing) device.
    pub async fn multi_device_link_client(
        &self,
        code: String,
        connected_tx: oneshot::Sender<()>,
        confirmation_rx: oneshot::Receiver<String>,
    ) -> anyhow::Result<Result<(), MultiDeviceLinkClientError>> {
        let max_devices = self.max_devices().await?;
        let devices = u32::try_from(self.self_group_client_ids().await?.len()).unwrap_or(u32::MAX);
        if max_devices > 0 && devices >= max_devices {
            return Ok(Err(MultiDeviceLinkClientError::DeviceLimitReached {
                max_devices,
            }));
        }

        let code = match LinkingCode::parse(&code) {
            Ok(code) => code,
            Err(error) => {
                warn!(%error, "rejected a malformed linking code");
                return Ok(Err(MultiDeviceLinkClientError::InvalidCode));
            }
        };

        let client = self.api_client()?;
        let qs_user_id = self.inner.qs_user_id;
        let qs_user_signing_key = self.key_store().qs_user_signing_key.clone();
        let rendezvous_id = RendezvousId::new(code.rendezvous_id().to_owned());

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

        let session = self.link_session(&code, &tx, &mut rx, connected_tx, confirmation_rx);
        match Box::pin(session).await {
            Ok(()) => Ok(Ok(())),
            Err(LinkingError::DeviceLimitReached { max_devices }) => {
                Ok(Err(MultiDeviceLinkClientError::DeviceLimitReached {
                    max_devices,
                }))
            }
            Err(error) => {
                let error = resolve_closed_session(error, &mut rx).await;
                if let Some(code) = error.abort_code() {
                    send_abort(tx, &mut rx, code).await;
                }
                Err(error.into())
            }
        }
    }

    async fn link_session(
        &self,
        code: &LinkingCode,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
        connected_tx: oneshot::Sender<()>,
        confirmation_rx: oneshot::Receiver<String>,
    ) -> Result<(), LinkingError> {
        let message = next_message(rx).await?;
        let MdlMessage::PakeShareA(PakeShareA {
            version,
            sid,
            msg_a,
        }) = message
        else {
            return Err(unexpected(&message));
        };
        check_version(version)?;

        let domain = self.user_id().domain().to_string();
        let ci = MdlContext::new(domain, code.rendezvous_id().to_owned())
            .tls_serialize_detached()
            .context("serialize the linking context")?;

        let response = pake::respond(code.password(), &ci, &sid, &msg_a)
            .map_err(|error| LinkingError::Protocol(error.into()))?;
        let kdf_ctx = MdlKdfContext {
            ci: VLByteSlice(&ci),
            sid: VLByteSlice(&sid),
            msg_a: VLByteSlice(&msg_a),
            msg_b: VLByteSlice(&response.msg_b),
        }
        .tls_serialize_detached()
        .context("serialize the linking kdf context")?;
        let psk = response.isk.derive_psk(&kdf_ctx);

        let key_package = pairing::validate_key_package(&response.key_package)?;
        let _ = connected_tx.send(());

        let device_name = tokio::select! {
            confirmation = confirmation_rx => {
                confirmation.map_err(|_| LinkingError::UserRejected)?
            }
            message = next_message(rx) => {
                return Err(relay_interrupted(message));
            }
        };

        let (mut pairing, welcome) = PairingGroup::create(&psk, key_package)?;
        drop(psk);

        let share = MdlMessage::PakeShareB(PakeShareB {
            msg_b: response.msg_b,
            welcome,
        })
        .into_frame()
        .context("frame the pake share")?;
        send_frame(tx, share, "send the pake share").await?;
        info!("created the pairing group and sent the welcome");

        // Only a peer that derived the same PSK can speak in the group, so
        // this is where the new device is authenticated to this side.
        receive_payload(&mut pairing, rx, LinkingPayloadType::PairingConfirmed).await?;
        info!("the new device confirmed the pairing");

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

        let linked = self
            .complete_link(link_id, &package, &mut pairing, tx, rx)
            .await;
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
        pairing: &mut PairingGroup,
        tx: &mpsc::Sender<RelayFrame>,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<(), LinkingError> {
        let request = exchange_provisioning(package, pairing, tx, rx).await?;
        let new_client_id = request.device.client_id;
        DeviceLink::request_add(
            self.db().write().await.context("open the client store")?,
            link_id,
            request,
            link_deadline(),
        )
        .await?;
        self.outbound_service().notify_pending_chat_operations();
        self.await_device_added(link_id, new_client_id, rx).await?;
        info!("added new device to self group");

        // The new device confirms once it has processed the Welcome from
        // its queue. Without the confirmation it is taken out again, so the
        // user's retry starts clean.
        tokio::time::timeout(
            COMPLETION_TIMEOUT,
            receive_payload(pairing, rx, LinkingPayloadType::LinkingComplete),
        )
        .await
        .unwrap_or(Err(LinkingError::TimedOut(
            "the new device to confirm the join",
        )))?;
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
    async fn await_device_added(
        &self,
        link_id: Uuid,
        client_id: Uuid,
        rx: &mut Streaming<RelayFrame>,
    ) -> Result<(), LinkingError> {
        let wait = async {
            loop {
                if self.self_group_client_ids().await?.contains(&client_id) {
                    return Ok(());
                }
                let link = DeviceLink::load(
                    self.db().read().await.context("open the client store")?,
                    link_id,
                )
                .await?
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

                // The new device sends nothing until it joined, so anything
                // on the relay ends the session.
                tokio::select! {
                    biased;
                    message = next_message(rx) => return Err(relay_interrupted(message)),
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
            device_name,
            groups,
        })
    }

    /// Describe every higher-level group the virtual client is a member of, so a
    /// joining emulator client can onboard itself into each of them.
    ///
    /// Skips the emulation group itself, every connection chat that is not
    /// confirmed yet, and every chat that is not active. A pending chat is one
    /// whose onboarding has not landed, so its leaf is not ours to hand on.
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
                    let connection = match chat.chat_type() {
                        ChatType::Group(_) => None,
                        ChatType::Connection(user_id) => {
                            let Some(contact) = Contact::load(&mut *txn, user_id).await? else {
                                warn!(group_id = ?group.group_id, "no contact for connection chat; skipping group");
                                continue;
                            };
                            Some(ConnectionContact {
                                user_id: contact.user_id,
                                wai_ear_key: contact.wai_ear_key,
                                friendship_token: contact.friendship_token,
                            })
                        }
                        // TODO(gabriel): unconfirmed connections need the
                        // partial contact and the connection-offer PSK on top
                        // of the group.
                        ChatType::HandleConnection(_)
                        | ChatType::TargetedMessageConnection(_)
                        | ChatType::PendingConnection(_) => {
                            debug!(group_id = ?group.group_id, "skipping unconfirmed connection chat");
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
        // TODO: don't use Extension::Unknown
        leaf_node_extensions.add(Extension::Unknown(
            QS_CLIENT_REFERENCE_EXTENSION_TYPE,
            UnknownExtension(client_reference.tls_serialize_detached()?),
        ))?;
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
    async fn link_new_device(
        api_clients: ApiClients,
        db_path: &str,
        package: ProvisioningPackage,
    ) -> anyhow::Result<CoreUser> {
        let air_db = open_air_db(db_path).await?;
        let client_record_id = uuid::Uuid::new_v4();
        let client_db = open_client_db(db_path, client_record_id).await?;
        let global_lock = open_lock_file(db_path)?;

        let result: anyhow::Result<CoreUser> = async {
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
                    StorableQsQueueRatchet::initialize(&mut *txn, qs_initial_ratchet_secret)
                        .await?;
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

                    apply_settings_update(txn, &synced_settings).await?;
                    privacy_pass::store_provisioned_seeds(txn, &token_seeds).await?;
                    apply_blocked_contacts_update(txn, &blocked_contacts).await?;
                    privacy_pass::apply_redeemed_tokens(txn, &redeemed_tokens).await?;

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

            let mut client_record = ClientRecord::new(user_id.clone(), client_record_id);
            client_record.finish();
            client_record.store(air_db.write().await?).await?;

            Ok(final_state.final_state()?.into_self_user(
                client_db,
                client_record_id,
                api_clients,
                global_lock,
            ))
        }
        .await;

        if result.is_err() {
            Self::roll_back(db_path, client_record_id).await;
        }
        result
    }

    /// Whether a sibling device removed this device from the self group.
    pub async fn is_account_unlinked(&self) -> anyhow::Result<bool> {
        OwnClientInfo::is_account_unlinked(self.db().read().await?).await
    }
}
