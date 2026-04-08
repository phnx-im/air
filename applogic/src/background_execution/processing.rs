// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use anyhow::Context;
use std::panic::{self, AssertUnwindSafe};
use tokio::runtime::Builder;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    api::user::User,
    background_execution::{IncomingNotificationContent, stack},
    logging::init_logger,
    messages::FetchAndProcessAllMessagesError,
    notifications::{NotificationContent, NotificationId},
};

use aircoreclient::{ChatId, store::Store};

use super::NotificationBatch;

const SECOND_THREAD_STACK_SIZE: usize = 1024 * 1024; // 1 MB
const TOKIO_THREAD_STACK_SIZE: usize = 1024 * 1024; // 1 MB
const TOKIO_WORKER_THREADS: usize = 2; // Two threads for background tasks should be enough

pub(crate) fn error_batch(error: anyhow::Error) -> NotificationBatch {
    error!(%error, "Error notification batch");
    NotificationBatch {
        badge_count: 0,
        removals: Vec::new(),
        additions: Vec::new(),
    }
}

pub(crate) fn init_environment(content: &str) -> Option<NotificationBatch> {
    let incoming_content: IncomingNotificationContent = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(error) => {
            error!(%error, "Failed to parse incoming notification payload");
            return None;
        }
    };

    init_logger(incoming_content.log_file_path.clone());

    // Log stack size and remaining bytes
    info!(
        stack_size = stack::size(),
        remaining_bytes = stack::remaining(),
        "Stack info in original thread"
    );

    // Create a new thread with a larger stack
    let Ok(thread) = std::thread::Builder::new()
        .stack_size(SECOND_THREAD_STACK_SIZE)
        .spawn(move || {
            info!(
                stack_size = stack::size(),
                remaining_bytes = stack::remaining(),
                "Stack info in second thread"
            );

            init_tokio(incoming_content.path)
        })
    else {
        error!("Failed to spawn thread with increased stack size");
        return None;
    };

    thread
        .join()
        .map_err(|error| {
            match error.downcast::<&str>() {
                Ok(panic) => {
                    anyhow::format_err!("Thread panicked while initializing logger: {panic}")
                }
                Err(error) => match error.downcast::<String>() {
                    Ok(panic) => {
                        anyhow::format_err!("Thread panicked while initializing logger: {panic}")
                    }
                    Err(_) => {
                        anyhow::format_err!("Thread panicked while initializing logger occurred with unknown payload type")
                    }
                },
            }
        })
        .flatten()
        .ok()
}

/// Wraps with a tokio runtime to block on the async functions
pub(crate) fn init_tokio(path: String) -> anyhow::Result<NotificationBatch> {
    Builder::new_multi_thread()
        .thread_name("nse-thread")
        .enable_all()
        .thread_stack_size(TOKIO_THREAD_STACK_SIZE)
        .worker_threads(TOKIO_WORKER_THREADS)
        .on_thread_start(|| {
            // Log stack size and remaining bytes
            info!("Worker thread started");
            info!(
                stack_size = stack::size(),
                remaining_bytes = stack::remaining(),
                "Stack info in worker thread"
            );
        })
        .build()
        .context("Failed to initialize tokio runtime")
        .and_then(|runtime| {
            panic::catch_unwind(AssertUnwindSafe(|| {
                runtime.block_on(async { Box::pin(retrieve_messages(path)).await })
            }))
            .map_err(|payload| {
                if let Some(message) = payload.downcast_ref::<&str>() {
                    anyhow::format_err!("Panic in tokio runtime: {message}")
                } else if let Some(message) = payload.downcast_ref::<String>() {
                    anyhow::format_err!("Panic in tokio runtime: {message}")
                } else {
                    anyhow::format_err!("Panic in tokio runtime occurred with unknown payload type")
                }
            })
            .flatten()
        })
}

/// Load the user and retrieve messages
pub(crate) async fn retrieve_messages(path: String) -> anyhow::Result<NotificationBatch> {
    info!(path, "Retrieving messages with DB path");
    let user = User::load_default(path)
        .await
        .context("Failed to load user")?
        .context("User not found: the database contained no user data")?;

    // capture store notification in below store calls
    let pending_store_notifications = user.user.subscribe_iter();

    let notifications = match Box::pin(user.fetch_and_process_all_messages_in_background()).await {
        Ok(processed_messages) => {
            info!("All messages fetched and processed");
            processed_messages.notifications_content
        }
        Err(e) => match e {
            FetchAndProcessAllMessagesError::UnsupportedClientVersion => {
                error!("Unsupported client version");
                vec![NotificationContent {
                    identifier: NotificationId::update_required_id(),
                    title: "Software update required".to_string(),
                    body: "Update to keep using Air".to_string(),
                    chat_id: ChatId::new(Uuid::nil()),
                }]
            }
            FetchAndProcessAllMessagesError::Fatal(error) => {
                return Err(error.context("fatal error while fetching messages"));
            }
        },
    };

    let badge_count = user.global_unread_messages_count().await;

    for store_notification in pending_store_notifications {
        if let Err(error) = Store::enqueue_notification(&user.user, &store_notification).await {
            error!(%error, "Failed to enqueue store notification");
        }
    }

    Ok(NotificationBatch {
        badge_count,
        removals: Vec::new(),
        additions: notifications,
    })
}
