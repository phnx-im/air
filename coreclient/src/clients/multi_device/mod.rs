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

mod pairing;
mod payloads;
#[cfg(test)]
mod tests;

use airapiclient::rs_api::RsRequestError;
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
use chrono::Utc;
use openmls::prelude::{Credential, CredentialType, SignaturePublicKey};
use openmls::prelude::{CredentialWithKey, Extension, UnknownExtension};
use rand::TryRng;
use std::time::Duration;
use tls_codec::{Serialize as _, VLByteSlice};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;
use tonic::Streaming;
use tracing::{debug, error, info, warn};
use url::Url;
use uuid::Uuid;

use crate::{
    Chat, ChatId, ChatStatus, ChatType, Contact,
    clients::{
        CoreUser,
        api_clients::ApiClients,
        create_user::QsRegisteredUserState,
        listen_response,
        own_client_info::OwnClientInfo,
        process::process_qs::ProcessedQsMessages,
        store::{ClientRecord, UserCreationState},
        user_settings::{SettingsUpdateExt, apply_settings_update},
    },
    groups::{
        Group, client_auth_info::StorableUserCredential, openmls_provider::AirOpenMlsProvider,
    },
    job::chat_operation::ChatOperation,
    key_stores::{
        MemoryUserKeyStore, indexed_keys::StorableIndexedKey,
        queue_ratchets::StorableQsQueueRatchet,
    },
    privacy_pass,
    utils::persistence::{open_air_db, open_client_db, open_lock_file},
};

use pairing::{PairingGroup, PairingIdentity};

pub(crate) use payloads::{ConnectionContact, HigherLevelGroup};
use payloads::{ProvisioningPackage, SelfGroupJoinRequest};

/// Bytes of the CPace session id the new device draws per session.
const SID_LEN: usize = 16;

/// How long a device waits for the relay to let go of the call after the
/// device's last frame, before giving up on it.
const TEARDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// A step of the new device's provisioning run, reported to the UI.
#[derive(Debug)]
pub enum MultiDeviceProvisionStep {
    /// The relay assigned a session. The string is the full linking code the
    /// user carries over to their existing device.
    Code(String),
    /// The existing device answered the code and linking is under way.
    Linking,
}

/// Why the existing device could not start linking.
#[derive(Debug, thiserror::Error)]
pub enum MultiDeviceLinkClientError {
    #[error("session ID not found")]
    SessionNotFound,
    /// The code was too short or its check digit did not match, so the relay
    /// was never contacted and no session was burned.
    #[error("the linking code is malformed")]
    InvalidCode,
}

/// Why a linking session ended badly.
#[derive(Debug, thiserror::Error)]
pub(crate) enum LinkingError {
    /// The pairing group's Welcome did not open, so the two devices derived
    /// different PSKs. Either the code was mistyped past the check digit, or
    /// somebody tried to intercept the linking.
    #[error("authentication failed, the linking code was wrong or the session was intercepted")]
    AuthenticationFailed,
    #[error("linking protocol error")]
    Protocol(#[source] anyhow::Error),
    #[error("linking validation failed: {0}")]
    Validation(String),
    #[error("the user declined the link")]
    UserRejected,
    #[error("the peer aborted the link: {0:?}")]
    PeerAborted(AbortCode),
    /// The new device speaks a version this device does not.
    #[error("unsupported linking protocol version {0}")]
    UnsupportedVersion(u16),
    /// The relay call ended under us, so there is nobody left to tell.
    #[error("the linking session closed")]
    SessionClosed(#[source] anyhow::Error),
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
            Self::PeerAborted(_) => None,
            Self::UnsupportedVersion(_) => Some(AbortCode::UnsupportedVersion),
            Self::SessionClosed(_) => None,
        }
    }
}

impl From<anyhow::Error> for LinkingError {
    fn from(error: anyhow::Error) -> Self {
        Self::Protocol(error)
    }
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

/// Looks for the peer's abort in what is left of a call that ended under us.
///
/// The relay ends both streams once one peer closes, so a refused send or a
/// closed stream can hide an abort the peer sent just before. The call is
/// already over, so draining is short, and bounded regardless.
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
                // The completion frame is still queued. Let it out before the
                // call goes away, or the existing device sees a dropped
                // stream where linking in fact succeeded.
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

        let key_package = core_user.generate_self_group_key_package().await?;
        // Store our own entry locally and hand a copy to the old device, which
        // publishes it on the add commit.
        let device = core_user
            .store_own_device_entry(Utc::now(), Some(&device_name))
            .await?;
        let request = PersistenceCodec::to_vec(&SelfGroupJoinRequest {
            key_package,
            device,
        })
        .context("encode the self-group join request")?;
        let frame = pairing.send(LinkingPayloadType::SelfGroupJoinRequest, request)?;
        send_frame(tx, frame, "send the self-group join request").await?;
        info!("sent self-group key package and device entry to old device");

        core_user.join_self_group_from_queue().await?;
        info!("joined self group");

        let frame = pairing.send(LinkingPayloadType::LinkingComplete, Vec::new())?;
        send_frame(tx, frame, "signal that linking is done").await?;

        core_user.outbound_service().notify_vc_onboarding();

        Ok(core_user)
    }

    /// Answers a new device's linking `code` on this (existing) device.
    pub async fn multi_device_link_client(
        &self,
        code: String,
        connected_tx: oneshot::Sender<()>,
        confirmation_rx: oneshot::Receiver<String>,
    ) -> anyhow::Result<Result<(), MultiDeviceLinkClientError>> {
        // The check digit is verified before the relay is contacted. A
        // mistyped code would otherwise burn the session for good.
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
                return Err(match message {
                    Ok(message) => unexpected(&message),
                    Err(error) => error,
                });
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

        // Build the provisioning package (this creates a fresh queue for the
        // new device) and hand it over inside the pairing group.
        let package = self.build_provisioning_package(device_name).await?;
        let package =
            PersistenceCodec::to_vec(&package).context("encode the provisioning package")?;
        let frame = pairing.send(LinkingPayloadType::ProvisioningPackage, package)?;
        send_frame(tx, frame, "send the provisioning package").await?;
        info!("sent provisioning package to new device");

        // Receive the new device's self-group KeyPackage and its metadata
        // entry, and add it to the self group via the DS.
        let request =
            receive_payload(&mut pairing, rx, LinkingPayloadType::SelfGroupJoinRequest).await?;
        let request: SelfGroupJoinRequest =
            PersistenceCodec::from_slice(&request).context("decode the self-group join request")?;
        self.add_client_to_self_group(request).await?;
        info!("added new device to self group");

        // The new device reports that it joined, and both sides tear the
        // pairing group down.
        receive_payload(&mut pairing, rx, LinkingPayloadType::LinkingComplete).await?;
        info!("linking completed");

        Ok(())
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
        let groups = self.higher_level_groups().await?;

        // Snapshot the current synced settings so the new device starts with
        // our values. An empty update means we have no stored settings, which
        // the new device applies as a no-op.
        let synced_settings = self
            .db()
            .with_write_transaction(async |txn| SettingsUpdate::collect(txn).await)
            .await?;
        let token_seeds = privacy_pass::committed_seeds(self.db().read().await?).await?;

        Ok(ProvisioningPackage {
            user_id: self.user_id().clone(),
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
            identity_link_wrapper_key,
            synced_settings,
            token_seeds,
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

    /// Adds the new device's leaf to the self group.
    ///
    /// The entry already carries the name the confirming user chose: it travelled
    /// to the new device in the provisioning package, so both sides agree without
    /// this side substituting anything.
    ///
    /// The commit is staged and sent by the job system, so a failed send leaves
    /// the self group either retryable or clean, never stuck on a dead staged
    /// commit.
    async fn add_client_to_self_group(&self, request: SelfGroupJoinRequest) -> anyhow::Result<()> {
        let SelfGroupJoinRequest {
            key_package,
            device,
        } = request;

        // Sibling commits (e.g. the key-package upload cycle) may have advanced
        // the self group. Catch up on queued messages, so that the add commit
        // is built at the current epoch.
        let messages = self.qs_fetch_messages().await?;
        let processed = self.fully_process_qs_messages(messages).await;
        if let Some(error) = processed.errors.first() {
            warn!(%error, "failed to process queued messages before self-group add");
        }

        let chat_id = self.self_chat_id().await?.context("no self group")?;
        self.execute_job(ChatOperation::add_client(chat_id, key_package, device))
            .await?;

        Ok(())
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
                // The emulation epoch was registered while processing the
                // self-group Welcome.
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

        let drained = loop {
            match stream.next().await {
                Some(Ok(message)) => match message.event {
                    // Empty event is the sentinel: the queue is drained.
                    Some(listen_response::Event::Empty(_)) => break true,
                    Some(listen_response::Event::Message(queue_message)) => {
                        if let Ok(queue_message) = queue_message.try_into() {
                            messages.push(queue_message);
                        }
                    }
                    Some(listen_response::Event::Payload(_))
                    | Some(listen_response::Event::VersionStatus(_))
                    | None => {}
                },
                // Terminal status => stream is over, acks cannot be confirmed
                Some(Err(error)) => {
                    warn!(%error, "qs listen stream failed during drain");
                    break false;
                }
                // EOF without our half-close (old server) => stream is over
                None => break false,
            }
        };

        let num_messages = messages.len();
        let max_sequence_number = messages.last().map(|m| m.sequence_number);
        let processed = self.fully_process_qs_messages(messages).await;

        if processed.processed == num_messages {
            if let Some(max_sequence_number) = max_sequence_number {
                // Acks all messages before max_sequence_number + 1 (exclusive).
                responder.ack(max_sequence_number + 1).await;
                if drained {
                    // half-close the request stream, then wait for the server to apply the ack
                    responder.close(&mut stream).await;
                }
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
    ///
    /// Onboarding into the virtual client's higher-level groups is queued here,
    /// but only performed once we joined the emulation group: the outbound
    /// service defers a queued onboarding until the self group is there.
    async fn link_new_device(
        api_clients: ApiClients,
        db_path: &str,
        package: ProvisioningPackage,
    ) -> anyhow::Result<CoreUser> {
        let air_db = open_air_db(db_path).await?;
        let client_record_id = uuid::Uuid::new_v4();
        let client_db = open_client_db(db_path, client_record_id).await?;
        let global_lock = open_lock_file(db_path)?;

        let ProvisioningPackage {
            user_id,
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
            identity_link_wrapper_key: _,
            synced_settings,
            token_seeds,
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

                // Queue the onboarding into the groups the virtual client is
                // already a member of. This is committed before the client
                // record is finished below, so an interrupted linking (crash, exit)
                // leaves the onboarding to be picked up.
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

    /// Whether a sibling device removed this device from the self group.
    pub async fn is_account_unlinked(&self) -> anyhow::Result<bool> {
        OwnClientInfo::is_account_unlinked(self.db().read().await?).await
    }
}
