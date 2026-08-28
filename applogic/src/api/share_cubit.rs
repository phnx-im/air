// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Share feature
//!
//! Backs the share UI hosted by the iOS share extension. Loads the default
//! user, exposes the chats to pick from and sends the shared content into the
//! chosen chats.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use aircommon::{OpenMlsRand, RustCrypto};
use aircoreclient::{
    AttachmentProgressEvent, ChatId, MarkChatAsRead, MessageId, ProvisionAttachmentError,
    UploadTaskError, clients::CoreUser,
};
use anyhow::Context as _;
use flutter_rust_bridge::frb;
use futures_util::StreamExt;
use mimi_content::MimiContent;
use tokio::sync::{Mutex, watch};
use tracing::error;
use uuid::Uuid;

use crate::{
    StreamSink,
    api::{
        chat_details_cubit::load_chat_details,
        types::{UiChatDetails, UiChatStatus, UiChatType},
        user::User,
    },
    util::{Cubit, CubitCore, spawn_from_sync},
};

/// Maximal number of attachments accepted in a single share.
const MAX_SHARED_ATTACHMENTS: usize = 10;

/// A single attachment shared into the app from the native share sheet
///
/// The native host extracts the shared items into files it owns and hands
/// them over as `(path, mime_type)` pairs. The MIME type only selects the
/// preview on the Dart side. The upload pipeline sniffs the content type
/// from the file itself.
#[derive(Debug, Clone)]
pub struct UiSharedAttachment {
    pub path: String,
    pub mime_type: Option<String>,
}

/// Error which can occur when sending shared content
#[derive(Debug, Clone, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiShareSendError {
    AttachmentTooLarge {
        max_size_bytes: u64,
        actual_size_bytes: u64,
    },
    TooManyAttachments {
        max: u32,
    },
    Other,
}

/// Progress of sending shared content
#[derive(Debug, Clone, Default, PartialEq)]
#[frb(dart_metadata = ("freezed"))]
pub enum UiShareSendStatus {
    #[default]
    Idle,
    Uploading {
        /// 1-based index of the attachment currently uploading.
        current: u32,
        /// Total number of attachments to upload.
        total: u32,
        /// Overall progress over all attachments in `0.0..=1.0`.
        progress: f64,
    },
    Sending,
    Done,
    /// The messages are persisted locally but could not be delivered yet.
    /// The main app's outbound service retries them.
    Queued,
    Failed {
        error: UiShareSendError,
    },
}

/// How much of the shared content a single chat already received
///
/// Kept across send calls so that retrying after a failure does not hand the
/// same content to the outbound service twice.
#[derive(Debug, Default)]
#[frb(ignore)]
struct ChatSendProgress {
    /// Number of leading attachments already sent.
    attachments: usize,
    /// Whether the trailing text message was already sent.
    text: bool,
}

/// The state of the share feature
#[frb(dart_metadata = ("freezed"))]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShareState {
    pub loaded: bool,
    pub signed_in: bool,
    pub chats: Vec<UiChatDetails>,
    pub send_status: UiShareSendStatus,
}

/// The cubit backing the share UI
///
/// Unlike the other cubits it is not derived from a [`UserCubitBase`]. The
/// share UI runs in its own process, without the full app. The cubit loads
/// the default user itself and does not listen to the server queues.
///
/// [`UserCubitBase`]: crate::api::user_cubit::UserCubitBase
#[frb(opaque)]
pub struct ShareCubitBase {
    core: CubitCore<ShareState>,
    core_user: Arc<OnceLock<CoreUser>>,
    /// What each chat already received, tracked over retries.
    progress: Mutex<HashMap<ChatId, ChatSendProgress>>,
}

impl ShareCubitBase {
    /// Creates the cubit and loads the default user from the given database
    /// path in the background.
    ///
    /// When no user is found, the state is marked as loaded but not signed
    /// in.
    #[frb(sync)]
    pub fn new(db_path: String) -> Self {
        let core = CubitCore::new();
        let core_user = Arc::new(OnceLock::new());

        let load_task = core
            .cancellation_token()
            .clone()
            .run_until_cancelled_owned({
                let state_tx = core.state_tx().clone();
                let core_user = core_user.clone();
                async move { load_and_emit_state(db_path, state_tx, core_user).await }
            });
        spawn_from_sync(load_task);

        Self {
            core,
            core_user,
            progress: Mutex::new(HashMap::new()),
        }
    }

    // Cubit interface

    pub fn close(&self) {
        self.core.close();
    }

    #[frb(getter, sync)]
    pub fn is_closed(&self) -> bool {
        self.core.is_closed()
    }

    #[frb(getter, sync)]
    pub fn state(&self) -> ShareState {
        self.core.state()
    }

    pub async fn stream(&self, sink: StreamSink<ShareState>) {
        self.core.stream(sink).await;
    }

    // Cubit methods

    /// Resolves a direct-share target identifier (the chat id donated to
    /// the OS) to a chat available in the picker.
    ///
    /// Returns `None` while the state is not loaded yet or when the chat is
    /// not available.
    #[frb(sync, positional)]
    pub fn chat_id_for_share_target(&self, identifier: String) -> Option<ChatId> {
        let uuid = Uuid::parse_str(identifier.trim()).ok()?;
        let chat_id = ChatId::new(uuid);
        self.core
            .state()
            .chats
            .iter()
            .any(|chat| chat.id == chat_id)
            .then_some(chat_id)
    }

    /// Resets the send status back to idle (e.g. before retrying).
    #[frb(sync)]
    pub fn reset_send_status(&self) {
        self.emit_send_status(UiShareSendStatus::Idle);
    }

    /// Sends the shared content to the given chats.
    ///
    /// Each attachment is sent as its own message, matching the in-app
    /// behavior. The shared text and the caption are sent as one trailing
    /// text message. Every chat receives its own copy. Progress is reported
    /// via the state's `send_status`.
    ///
    /// Content a chat already received in an earlier call is skipped, so
    /// that retrying after a failure does not send it twice.
    ///
    /// All messages are persisted locally before sending. When delivery
    /// fails they stay queued and the main app's outbound service retries
    /// them, reported as [`UiShareSendStatus::Queued`].
    pub async fn send(
        &self,
        chat_ids: Vec<ChatId>,
        attachments: Vec<UiSharedAttachment>,
        message: Option<String>,
    ) -> anyhow::Result<()> {
        let core_user = self.core_user.get().context("user is not loaded")?.clone();

        // Capture store notifications of all writes below. They are
        // persisted at the end, so that the main app in its own process can
        // reload its stores.
        let pending_store_notifications = core_user.pending_db_notifications();

        let status = self
            .send_impl(&core_user, chat_ids, attachments, message)
            .await;

        for notification in pending_store_notifications {
            if let Err(error) = core_user.enqueue_db_notification(&notification).await {
                error!(%error, "Failed to enqueue store notification");
            }
        }

        self.emit_send_status(status);
        Ok(())
    }

    async fn send_impl(
        &self,
        core_user: &CoreUser,
        chat_ids: Vec<ChatId>,
        attachments: Vec<UiSharedAttachment>,
        message: Option<String>,
    ) -> UiShareSendStatus {
        if attachments.len() > MAX_SHARED_ATTACHMENTS {
            return UiShareSendStatus::Failed {
                error: UiShareSendError::TooManyAttachments {
                    max: MAX_SHARED_ATTACHMENTS as u32,
                },
            };
        }

        let message = message.filter(|text| !text.trim().is_empty());
        let mut chats_progress = self.progress.lock().await;

        // Each chat uploads its own copy of every attachment, so the work is
        // counted over all chats.
        let total = (attachments.len() * chat_ids.len()) as u32;
        let mut done: u32 = 0;
        let mut enqueued: Vec<MessageId> = Vec::new();
        let mut failure: Option<UiShareSendError> = None;

        'chats: for chat_id in chat_ids {
            let progress = chats_progress.entry(chat_id).or_default();

            for (index, attachment) in attachments.iter().enumerate() {
                done += 1;
                if index < progress.attachments {
                    continue;
                }
                let current = done;
                // Each upload contributes an equal share of the overall
                // progress, scaled by the byte progress of the running one.
                let report_progress = |fraction: f64| {
                    self.emit_send_status(UiShareSendStatus::Uploading {
                        current,
                        total,
                        progress: (f64::from(current) - 1.0 + fraction.clamp(0.0, 1.0))
                            / f64::from(total),
                    });
                };
                report_progress(0.0);
                match upload_attachment(core_user, chat_id, attachment, &report_progress).await {
                    Ok(message_id) => {
                        enqueued.push(message_id);
                        progress.attachments = index + 1;
                    }
                    Err(error) => {
                        failure = Some(error);
                        break 'chats;
                    }
                }
            }

            if let Some(text) = &message
                && !progress.text
            {
                match send_text_message(core_user, chat_id, text.clone()).await {
                    Ok(message_id) => {
                        enqueued.push(message_id);
                        progress.text = true;
                    }
                    Err(error) => {
                        error!(%error, "Failed to send shared text message");
                        failure = Some(UiShareSendError::Other);
                        break 'chats;
                    }
                }
            }
        }

        // Drain the outbound queues even when a later item failed. Earlier
        // messages are already enqueued and must still go out.
        if !enqueued.is_empty() {
            self.emit_send_status(UiShareSendStatus::Sending);
            core_user.outbound_service().run_once().await;
        }

        if let Some(error) = failure {
            return UiShareSendStatus::Failed { error };
        }

        if all_sent(core_user, &enqueued).await {
            UiShareSendStatus::Done
        } else {
            UiShareSendStatus::Queued
        }
    }

    fn emit_send_status(&self, status: UiShareSendStatus) {
        self.core
            .state_tx()
            .send_modify(|state| state.send_status = status);
    }
}

async fn load_and_emit_state(
    db_path: String,
    state_tx: watch::Sender<ShareState>,
    core_user_cell: Arc<OnceLock<CoreUser>>,
) {
    let user = match User::load_default(db_path).await {
        Ok(user) => user,
        Err(error) => {
            error!(%error, "Failed to load default user");
            None
        }
    };

    let Some(user) = user else {
        state_tx.send_modify(|state| {
            state.loaded = true;
            state.signed_in = false;
        });
        return;
    };

    let core_user = user.user;
    let chats = load_share_chats(&core_user).await;
    // The cell is only set here, so the error cannot happen.
    let _ = core_user_cell.set(core_user);

    state_tx.send_modify(|state| {
        state.loaded = true;
        state.signed_in = true;
        state.chats = chats;
    });
}

/// Loads the chats the user can share into, most recently used first.
///
/// Only active, confirmed chats are offered. Incoming connection
/// requests must be accepted in the main app before messages can be sent.
async fn load_share_chats(core_user: &CoreUser) -> Vec<UiChatDetails> {
    let chat_ids = share_chat_ids(core_user).await;
    let mut chats = Vec::with_capacity(chat_ids.len());
    for chat_id in chat_ids {
        let Some(details) = load_share_chat(core_user, &chat_id).await else {
            continue;
        };
        chats.push(details);
    }
    chats
}

/// The chats to consider for sharing, most recently used first.
async fn share_chat_ids(core_user: &CoreUser) -> Vec<ChatId> {
    core_user
        .ordered_chat_ids()
        .await
        .inspect_err(|error| error!(%error, "Failed to load chats"))
        .unwrap_or_default()
}

/// Loads the chat details when the chat can be shared into.
async fn load_share_chat(core_user: &CoreUser, chat_id: &ChatId) -> Option<UiChatDetails> {
    let chat = core_user.chat(chat_id).await?;
    let details = load_chat_details(core_user, chat).await;
    let is_confirmed = matches!(
        details.chat_type,
        UiChatType::Connection(_) | UiChatType::Group(_)
    );
    (matches!(details.status, UiChatStatus::Active) && is_confirmed).then_some(details)
}

async fn upload_attachment(
    core_user: &CoreUser,
    chat_id: ChatId,
    attachment: &UiSharedAttachment,
    report_progress: impl Fn(f64),
) -> Result<MessageId, UiShareSendError> {
    let path = PathBuf::from(&attachment.path);
    // Sharing happens without the user looking at the chat, so it must not
    // mark older messages as read.
    let provisioned =
        Box::pin(core_user.upload_chat_attachment(chat_id, &path, MarkChatAsRead::No))
            .await
            .map_err(|error| {
                error!(%error, "Failed to provision shared attachment");
                UiShareSendError::Other
            })?;

    let (attachment_id, progress, upload_task) = match provisioned {
        Ok(result) => result,
        Err(ProvisionAttachmentError::TooLarge(detail)) => {
            return Err(UiShareSendError::AttachmentTooLarge {
                max_size_bytes: detail.max_size_bytes,
                actual_size_bytes: detail.actual_size_bytes,
            });
        }
    };

    // Forward the upload progress while the upload task runs. The stream
    // ends with a terminal event when the task finishes.
    let forward_progress = async {
        let mut events = progress.stream();
        while let Some(event) = events.next().await {
            match event {
                AttachmentProgressEvent::Init => {}
                AttachmentProgressEvent::Progress { bytes_loaded } => {
                    if let Some(bytes_total) = progress.total_bytes() {
                        report_progress(bytes_loaded as f64 / bytes_total as f64);
                    }
                }
                AttachmentProgressEvent::Completed
                | AttachmentProgressEvent::Failed
                | AttachmentProgressEvent::NotFound => break,
            }
        }
    };

    let (upload_result, ()) = tokio::join!(upload_task, forward_progress);
    match upload_result {
        Ok(message) => {
            core_user
                .outbound_service()
                .enqueue_chat_message(message.id())
                .await
                .map_err(|error| {
                    error!(%error, "Failed to enqueue shared attachment message");
                    UiShareSendError::Other
                })?;
            Ok(message.id())
        }
        Err(UploadTaskError { message_id, error }) => {
            error!(%error, ?attachment_id, "Failed to upload shared attachment");
            if let Err(error) = core_user
                .outbound_service()
                .fail_enqueued_chat_message(message_id)
                .await
            {
                error!(%error, "Failed to mark attachment message as failed");
            }
            Err(UiShareSendError::Other)
        }
    }
}

async fn send_text_message(
    core_user: &CoreUser,
    chat_id: ChatId,
    text: String,
) -> anyhow::Result<MessageId> {
    let salt: [u8; 16] = RustCrypto::default().random_array()?;
    let content = MimiContent::simple_markdown_message(text, salt);
    // Sharing happens without the user looking at the chat, so it must not
    // mark older messages as read.
    let message =
        Box::pin(core_user.send_message(chat_id, content, None, MarkChatAsRead::No)).await?;
    Ok(message.id())
}

async fn all_sent(core_user: &CoreUser, message_ids: &[MessageId]) -> bool {
    for message_id in message_ids {
        match core_user.message(*message_id).await {
            Ok(Some(message)) if message.is_sent() => {}
            Ok(_) => return false,
            Err(error) => {
                error!(%error, "Failed to load sent message");
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn loads_signed_out_state_when_there_is_no_user() {
        let temp_dir = tempfile::tempdir().unwrap();
        let (state_tx, _state_rx) = watch::channel(ShareState::default());
        let core_user = Arc::new(OnceLock::new());

        load_and_emit_state(
            temp_dir.path().to_str().unwrap().to_owned(),
            state_tx.clone(),
            core_user.clone(),
        )
        .await;

        let state = state_tx.borrow().clone();
        assert!(state.loaded);
        assert!(!state.signed_in);
        assert!(state.chats.is_empty());
        assert!(core_user.get().is_none());
    }
}
