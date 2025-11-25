// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{num::NonZeroUsize, sync::Arc};

use aircoreclient::{ChatId, clients::CoreUser, store::Store};
use flutter_rust_bridge::frb;
use lru::LruCache;
use parking_lot::Mutex;
use tokio_util::sync::{CancellationToken, DropGuard};
use tracing::{debug, error};

use crate::{
    api::{chat_details_cubit::load_chat_details, types::UiChatDetails, user_cubit::UserCubitBase},
    util::spawn_from_sync,
};

type ChatsLruCache = Arc<Mutex<LruCache<ChatId, UiChatDetails>>>;

/// A repository that provides access to chats.
///
/// This is a simple LRU cache. No interface is exposed to dart. But the cache lifetime is managed
/// through Dart as a mounted repository in the widget tree.
#[frb(opaque)]
#[derive(Clone)]
pub struct ChatsRepository {
    inner: Arc<ChatsRepositoryInner>,
}

struct ChatsRepositoryInner {
    chats: ChatsLruCache,
    _cancel: DropGuard,
}

const CHAT_REPOSITORY_CACHE_SIZE: NonZeroUsize = NonZeroUsize::new(100).unwrap();

impl ChatsRepository {
    #[frb(sync)]
    pub fn new(user_cubit: &UserCubitBase) -> Self {
        let store = user_cubit.core_user().clone();

        let chats = LruCache::new(CHAT_REPOSITORY_CACHE_SIZE);
        let chats = Arc::new(Mutex::new(chats));

        let cancel = CancellationToken::new();

        let load_on_startup_task = cancel
            .clone()
            .run_until_cancelled_owned(Self::load_on_startup_task(store.clone(), chats.clone()));
        spawn_from_sync(load_on_startup_task);

        let inner = Arc::new(ChatsRepositoryInner {
            chats,
            _cancel: cancel.drop_guard(),
        });
        Self { inner }
    }

    /// Returns the chat details for the given chat id if it is in the cache.
    ///
    /// Return `None` if the chat is not in the cache or if the chat is not found.
    pub(crate) fn get(&self, chat_id: ChatId) -> Option<UiChatDetails> {
        debug!(%chat_id, "get");
        self.inner.chats.lock().get(&chat_id).cloned()
    }

    /// Stores the chat details in the cache.
    pub(crate) fn put(&self, chat_details: UiChatDetails) {
        let chat_id = chat_details.id;
        debug!(%chat_id, "put");
        self.inner.chats.lock().put(chat_id, chat_details);
    }

    async fn load_on_startup_task(store: CoreUser, chats: ChatsLruCache) {
        let Ok(chat_ids) = store.ordered_chat_ids().await.inspect_err(|error| {
            error!(%error, "Failed to load chats");
        }) else {
            return;
        };
        let len = chat_ids.len().min(CHAT_REPOSITORY_CACHE_SIZE.get());
        debug!(%len, "load_on_startup_task");
        for chat_id in &chat_ids[..len] {
            if let Some(chat) = store.chat(chat_id).await {
                let chat_details = load_chat_details(&store, chat).await;
                chats.lock().put(chat_details.id, chat_details);
            }
        }
    }
}
