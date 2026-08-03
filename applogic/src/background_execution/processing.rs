// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::any::Any;
use std::future::Future;
use std::panic::{self, AssertUnwindSafe};

use anyhow::Context;
use chrono::{DateTime, Utc};
use tokio::runtime::Builder;
use tracing::{error, info};
use uuid::Uuid;

use crate::{
    api::user::User,
    background_execution::{IncomingNotificationContent, IncomingNotificationDismissal, stack},
    logging::init_logger,
    messages::FetchAndProcessAllMessagesError,
    notifications::{NotificationContent, NotificationId},
};

use aircoreclient::ChatId;

use super::NotificationBatch;

const SECOND_THREAD_STACK_SIZE: usize = 1024 * 1024; // 1 MB
const TOKIO_THREAD_STACK_SIZE: usize = 1024 * 1024; // 1 MB
const TOKIO_WORKER_THREADS: usize = 2; // Two threads for background tasks should be enough

pub(crate) fn init_environment(content: &str) -> Option<NotificationBatch> {
    let incoming_content: IncomingNotificationContent = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(error) => {
            error!(%error, "Failed to parse incoming notification payload");
            return None;
        }
    };
    init_logger(incoming_content.log_file_path.clone());

    let path = incoming_content.path;
    run_in_background_runtime("process-new-messages", move || retrieve_messages(path))
}

/// Processes a notification dismissal payload and persists the chat's `notified_until` watermark.
pub(crate) fn init_dismissal_environment(content: &str) -> Option<()> {
    let incoming_dismissal: IncomingNotificationDismissal = match serde_json::from_str(content) {
        Ok(value) => value,
        Err(error) => {
            error!(%error, "Failed to parse incoming notification dismissal payload");
            return None;
        }
    };
    init_logger(incoming_dismissal.log_file_path.clone());

    let IncomingNotificationDismissal {
        path,
        chat_id,
        newest_timestamp,
        ..
    } = incoming_dismissal;
    run_in_background_runtime("notification-dismissal", move || {
        persist_notification_dismissal(path, chat_id, newest_timestamp)
    })
}

/// Runs the future produced by `make_future` to completion on a fresh tokio runtime, on a thread
/// with an enlarged stack.
///
/// The extra thread is needed because the platform-provided stack of the caller can be tiny (e.g.
/// in the iOS Notification Service Extension). Panics in the thread or in the runtime are converted
/// into errors. Returns `None` on error or panic (logged).
fn run_in_background_runtime<T, F, Fut>(task: &'static str, make_future: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<T>>,
{
    // Log stack size and remaining bytes
    info!(
        stack_size = stack::size(),
        remaining_bytes = stack::remaining(),
        "Stack info in original thread"
    );

    let thread = match std::thread::Builder::new()
        .stack_size(SECOND_THREAD_STACK_SIZE)
        .spawn(move || {
            info!(
                stack_size = stack::size(),
                remaining_bytes = stack::remaining(),
                "Stack info in second thread"
            );
            block_on_runtime(task, make_future())
        }) {
        Ok(thread) => thread,
        Err(error) => {
            error!(%error, "Failed to spawn thread with increased stack size");
            return None;
        }
    };

    thread
        .join()
        .map_err(|payload| {
            anyhow::format_err!("Task {task} panicked: {}", panic_message(payload.as_ref()))
        })
        .flatten()
        .inspect_err(|error| {
            error!(%error, task, "Failed to run background task");
        })
        .ok()
}

/// Wraps with a tokio runtime to block on the async function
fn block_on_runtime<T, Fut>(task: &'static str, future: Fut) -> anyhow::Result<T>
where
    Fut: Future<Output = anyhow::Result<T>>,
{
    let runtime = Builder::new_multi_thread()
        .thread_name(task)
        .enable_all()
        .thread_stack_size(TOKIO_THREAD_STACK_SIZE)
        .worker_threads(TOKIO_WORKER_THREADS)
        .on_thread_start(|| {
            // Log stack size and remaining bytes
            info!(
                stack_size = stack::size(),
                remaining_bytes = stack::remaining(),
                "Stack info in worker thread"
            );
        })
        .build()
        .context("Failed to initialize tokio runtime")?;
    panic::catch_unwind(AssertUnwindSafe(|| runtime.block_on(Box::pin(future))))
        .map_err(|payload| {
            anyhow::format_err!(
                "Panic in tokio runtime: {}",
                panic_message(payload.as_ref())
            )
        })
        .flatten()
}

/// Extracts a human-readable message from a thread join or unwind panic payload.
fn panic_message(payload: &(dyn Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message
    } else {
        "unknown payload type"
    }
}

/// Load the user and retrieve messages
async fn retrieve_messages(path: String) -> anyhow::Result<NotificationBatch> {
    info!(path, "Retrieving messages with DB path");
    let user = User::load_default(path)
        .await
        .context("Failed to load user")?
        .context("User not found: the database contained no user data")?;

    // capture store notification in below store calls
    let pending_store_notifications = user.user.pending_db_notifications();

    let (notifications, removals) =
        match Box::pin(user.fetch_and_process_all_messages_in_background()).await {
            Ok(processed_messages) => {
                info!("All messages fetched and processed");
                let removals = processed_messages
                    .empty_chat_ids
                    .iter()
                    .map(|chat_id| chat_id.uuid().to_string())
                    .collect();
                (processed_messages.notifications_content, removals)
            }
            Err(e) => match e {
                FetchAndProcessAllMessagesError::UnsupportedClientVersion => {
                    error!("Unsupported client version");
                    let notifications = vec![NotificationContent {
                        identifier: NotificationId::update_required_id(),
                        title: "Software update required".to_string(),
                        body: "Update to keep using Air".to_string(),
                        chat_id: ChatId::new(Uuid::nil()),
                        conversation: None,
                    }];
                    (notifications, Vec::new())
                }
                FetchAndProcessAllMessagesError::Fatal(error) => {
                    return Err(error.context("fatal error while fetching messages"));
                }
            },
        };

    let badge_count = user.global_unread_messages_count().await;

    for store_notification in pending_store_notifications {
        if let Err(error) = user.user.enqueue_db_notification(&store_notification).await {
            error!(%error, "Failed to enqueue store notification");
        }
    }

    Ok(NotificationBatch {
        badge_count,
        removals,
        additions: notifications,
    })
}

async fn persist_notification_dismissal(
    path: String,
    chat_id: String,
    newest_timestamp: String,
) -> anyhow::Result<()> {
    let chat_id = Uuid::parse_str(&chat_id)
        .context("Failed to parse chat id")
        .map(ChatId::new)?;

    // The watermark is what the dismissed notification actually displayed. It must not be
    // recomputed from the database: a message that arrived after the dismissal would be watermarked
    // over and never notify.
    let notified_until = DateTime::parse_from_rfc3339(&newest_timestamp)
        .context("Failed to parse newest timestamp")?
        .with_timezone(&Utc);

    let user = User::load_default(path)
        .await
        .context("Failed to load user")?
        .context("User not found: the database contained no user data")?;

    user.user
        .set_chat_notified_until(chat_id, notified_until)
        .await
        .context("Failed to persist notification watermark")
}
