// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use aircoreclient::{
    ChatId, ChatMessage,
    clients::CoreUser,
    link_preview::{DraftLinkPreview, LinkPreview, select_links},
};
use parking_lot::{Mutex, MutexGuard};
use tokio::sync::{OnceCell, mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::error;

use super::{ComposerLinkPreview, OriginalPreview};
use crate::{
    api::{chat_details_cubit::ChatDetailsState, user_settings_cubit::UserSettings},
    util::spawn_from_sync,
};

/// Card of `message` with the stored bytes of its image.
pub(crate) async fn original_preview(
    core_user: &CoreUser,
    message: &ChatMessage,
) -> Option<OriginalPreview> {
    let preview = LinkPreview::from_content(message.message().mimi_content()?)?;
    let image = match preview.image() {
        Some((_, part_index)) => {
            let attachments = core_user.attachment_infos_for_message(message.id()).await;
            attachments
                .find(part_index, None)
                .map(|info| info.attachment_id)
        }
        None => None,
    };
    let image_bytes = match image {
        Some(attachment_id) => core_user
            .load_attachment(attachment_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to load the card image"))
            .ok()
            .and_then(|content| content.into_bytes()),
        None => None,
    };
    Some(OriginalPreview::new(preview.card().clone(), image_bytes))
}

/// Composer link preview shared with background tasks.
///
/// Emits the preview to the state and stores it with the draft on change.
#[derive(Clone)]
pub(crate) struct SharedLinkPreview {
    core_user: CoreUser,
    chat_id: ChatId,
    preview: Arc<Mutex<ComposerLinkPreview>>,
    state_tx: watch::Sender<ChatDetailsState>,
    user_settings_rx: watch::Receiver<UserSettings>,
    store_tx: mpsc::UnboundedSender<Option<DraftLinkPreview>>,
    restored: Arc<OnceCell<()>>,
}

impl SharedLinkPreview {
    /// Creates the preview and spawns the task storing it.
    ///
    /// The task ends when all clones are dropped.
    pub(crate) fn new(
        core_user: CoreUser,
        chat_id: ChatId,
        state_tx: watch::Sender<ChatDetailsState>,
        user_settings_rx: watch::Receiver<UserSettings>,
    ) -> Self {
        let (store_tx, mut store_rx) = mpsc::unbounded_channel::<Option<DraftLinkPreview>>();
        let store_core_user = core_user.clone();
        spawn_from_sync(async move {
            while let Some(preview) = store_rx.recv().await {
                if let Err(error) = store_core_user
                    .store_draft_link_preview(chat_id, preview.as_ref())
                    .await
                {
                    error!(%error, "Failed to store the link preview of the draft");
                }
            }
        });
        Self {
            core_user,
            chat_id,
            preview: Default::default(),
            state_tx,
            user_settings_rx,
            store_tx,
            restored: Default::default(),
        }
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, ComposerLinkPreview> {
        self.preview.lock()
    }

    /// Applies `f`, emits the preview and stores it if it changed.
    pub(crate) fn update<R>(&self, f: impl FnOnce(&mut ComposerLinkPreview) -> R) -> R {
        self.apply(true, f)
    }

    /// Applies `f` and emits the preview without storing it.
    fn update_unstored<R>(&self, f: impl FnOnce(&mut ComposerLinkPreview) -> R) -> R {
        self.apply(false, f)
    }

    fn apply<R>(&self, store: bool, f: impl FnOnce(&mut ComposerLinkPreview) -> R) -> R {
        let mut preview = self.preview.lock();
        let key = store.then(|| preview.stored_key());
        let result = f(&mut preview);
        if let Some(key) = key
            && key != preview.stored_key()
        {
            // Sent under the lock to keep the writes in order
            let _ = self.store_tx.send(preview.to_stored());
        }
        let ui = self
            .user_settings_rx
            .borrow()
            .generate_link_previews
            .then(|| preview.ui())
            .flatten();
        self.state_tx.send_if_modified(|state| {
            let changed = state.link_preview != ui;
            state.link_preview = ui;
            changed
        });
        result
    }

    /// Waits until the preview stored with the draft is restored.
    ///
    /// The first call restores it.
    pub(crate) async fn restored(&self, parent: &CancellationToken) {
        self.restored.get_or_init(|| self.restore(parent)).await;
    }

    async fn restore(&self, parent: &CancellationToken) {
        if !self.user_settings_rx.borrow().generate_link_previews {
            return;
        }
        let core_user = &self.core_user;
        let chat_id = self.chat_id;

        let draft = core_user
            .message_draft(chat_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to load the draft"))
            .ok()
            .flatten();

        let editing_id = draft.as_ref().and_then(|draft| draft.editing_id);
        let seeded = editing_id.is_some_and(|editing_id| self.lock().is_editing(editing_id));
        if let Some(editing_id) = editing_id
            && !seeded
        {
            match core_user.message(editing_id).await {
                Ok(Some(message)) => {
                    let original = original_preview(core_user, &message).await;
                    self.update(|lp| lp.start_edit(editing_id, original));
                }
                Ok(None) => {}
                Err(error) => error!(%error, "Failed to load the edited message"),
            }
        }

        let Some(stored) = core_user
            .draft_link_preview(chat_id)
            .await
            .inspect_err(|error| error!(%error, "Failed to load the link preview of the draft"))
            .ok()
            .flatten()
        else {
            return;
        };
        let url = match &stored {
            DraftLinkPreview::Fetched { preview, .. } => preview.card().url(),
            DraftLinkPreview::Dismissed { url } => url.as_str(),
        };
        let first = draft
            .as_ref()
            .and_then(|draft| select_links(&draft.message).next());
        if first != Some(url) || self.lock().is_original(url) {
            let _ = self.store_tx.send(None);
            return;
        }

        match stored {
            DraftLinkPreview::Fetched {
                preview,
                fetched_at,
            } => {
                self.update_unstored(|lp| lp.restore(preview, fetched_at));
                self.spawn_refresh(parent);
            }
            DraftLinkPreview::Dismissed { url } => {
                self.update_unstored(|lp| lp.restore_dismissed(url));
            }
        }
    }

    /// Fetches the ready preview again in the background.
    ///
    /// The preview stays shown while fetching and is kept if the fetch fails.
    fn spawn_refresh(&self, parent: &CancellationToken) {
        let Some((url, token)) = self.lock().start_refresh(parent) else {
            return;
        };
        let this = self.clone();
        let core_user = self.core_user.clone();
        spawn_from_sync(async move {
            let fetch = core_user.fetch_link_preview(&url, token.clone());
            if let Some(fetched) = token.run_until_cancelled(fetch).await {
                this.update(|lp| lp.finish_refresh(&token, fetched));
            }
        });
    }
}
