// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Thin, non-panicking wrappers around [`CoreUser`] operations.
//!
//! Unlike the invariant-checking helpers in `test_harness`, everything here
//! returns a `Result` instead of asserting: a stress run should keep going
//! and count failures rather than abort on the first divergence.

use std::{str::FromStr, time::Duration};

use aircommon::identifiers::{UserId, Username};
use aircoreclient::{ChatId, DisplayName, UserProfile, UsernameRecord, clients::CoreUser};
use anyhow::Context;
use rand::{RngExt, distr::Alphanumeric};
use tokio_stream::StreamExt;
use tracing::debug;

pub struct DrainOutcome {
    pub fetched: usize,
    /// Messages `fully_process_qs_messages` couldn't apply. Coreclient
    /// already rolls each of these back to a savepoint and advances the
    /// ratchet past them, so they don't indicate a stuck queue or corrupted
    /// state -- typically a message for a group this member has meanwhile
    /// been removed from. Surfaced as a count rather than failing the drain.
    pub message_errors: usize,
}

/// Drains a member's QS queue and runs its outbound service once. This is
/// how a member picks up commits made by others (adds, removes, messages)
/// and flushes its own pending work (key package replenishment, etc.).
pub async fn drain(user: &CoreUser) -> anyhow::Result<DrainOutcome> {
    let messages = user.qs_fetch_messages().await?;
    let fetched = messages.len();
    let result = user.fully_process_qs_messages(messages).await;
    for error in &result.errors {
        debug!(%error, "qs message could not be applied, skipped");
    }
    user.outbound_service().run_once().await;
    Ok(DrainOutcome {
        fetched,
        message_errors: result.errors.len(),
    })
}

/// Sets `user`'s display name to a deterministic, human-legible label, so
/// members show up as more than a bare UUID (in logs, in the app if one is
/// pointed at the same server).
///
/// Call this only on a freshly created member: it rotates the user profile
/// key and republishes it to the DS once per group the member is in. The
/// early return keeps a retried creation from re-signing a profile it
/// already set.
pub async fn ensure_profile(user: &CoreUser, index: usize) -> anyhow::Result<()> {
    let display_name = DisplayName::from_str(&format!("Stress {:04}", index + 1))?;
    let current = user.own_user_profile().await?;
    if current.display_name == display_name {
        return Ok(());
    }
    user.set_own_user_profile(UserProfile {
        user_id: user.user_id().clone(),
        display_name,
        profile_picture: None,
    })
    .await?;
    Ok(())
}

/// Registers a username for `user` if it doesn't have one yet, returning its
/// local [`UsernameRecord`] either way. The username is derived from the
/// user's own id, so recreating it is idempotent across resumed runs.
pub async fn ensure_username(user: &CoreUser) -> anyhow::Result<UsernameRecord> {
    if let Some(existing) = user.username_records().await?.into_iter().next() {
        return Ok(existing);
    }
    // A fresh member has no replenished privacy pass tokens yet; give the
    // outbound service a chance to fetch some before requesting a username.
    user.outbound_service().run_once().await;
    let raw = format!("stress-{}", user.user_id().uuid().simple());
    let username = Username::new(raw)?;
    user.add_username(username)
        .await?
        .context("username was already taken by someone else")
}

/// Has `initiator` connect to `responder` by username. `responder` must
/// already have a registered username (see [`ensure_username`]). Drives both
/// sides of the handshake and returns the resulting connection [`ChatId`].
pub async fn connect(
    initiator: &CoreUser,
    responder: &CoreUser,
    responder_record: &UsernameRecord,
) -> anyhow::Result<ChatId> {
    let chat_id = initiator
        .add_contact(responder_record.username.clone(), responder_record.hash)
        .await?
        .map_err(|error| anyhow::anyhow!("add_contact rejected: {error:?}"))?;

    // Responder picks up the connection request from its username queue.
    // The listener is server-streaming, so a bounded drain with a short idle
    // timeout is used instead of waiting indefinitely.
    let (mut stream, responder_ack) = responder.listen_username(responder_record).await?;
    while let Some(Some(message)) = tokio::time::timeout(Duration::from_millis(500), stream.next())
        .await
        .unwrap_or(None)
    {
        let message_id = message.message_id.context("username message has no id")?;
        responder
            .process_username_queue_message(responder_record.username.clone(), message)
            .await?;
        responder_ack.ack(message_id.into()).await;
    }

    responder
        .accept_contact_request(chat_id)
        .await?
        .map_err(|error| anyhow::anyhow!("accept_contact_request rejected: {error}"))?;

    // Initiator picks up the acceptance to finish materializing its side of
    // the connection group.
    drain(initiator).await?;

    Ok(chat_id)
}

pub async fn create_group(creator: &CoreUser, title: String) -> anyhow::Result<ChatId> {
    creator.create_chat(title, None, false).await
}

pub async fn invite(
    inviter: &CoreUser,
    chat_id: ChatId,
    invitees: &[UserId],
) -> anyhow::Result<()> {
    inviter
        .invite_users(chat_id, invitees)
        .await?
        .map_err(|error| anyhow::anyhow!("invite rejected: {error}"))?;
    Ok(())
}

pub async fn remove(
    remover: &CoreUser,
    chat_id: ChatId,
    targets: Vec<UserId>,
) -> anyhow::Result<()> {
    remover.remove_users(chat_id, targets).await?;
    Ok(())
}

pub async fn leave(user: &CoreUser, chat_id: ChatId) -> anyhow::Result<()> {
    user.leave_chat(chat_id).await
}

/// Sends a random text message. The caller must have drained `user` first:
/// a message built from a stale local epoch, or from a leaf the DS has since
/// blanked, is rejected server-side (e.g. as "unknown sender").
pub async fn send_message(user: &CoreUser, chat_id: ChatId) -> anyhow::Result<()> {
    let text: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect();
    let salt: [u8; 16] = rand::rng().random();
    let content = mimi_content::MimiContent::simple_markdown_message(text, salt);
    user.send_message(chat_id, content, None).await?;
    Ok(())
}

/// Has `committer` commit a plain key update, rotating its own leaf key
/// material. A commit also sweeps in whatever proposals the group has staged,
/// so this doubles as the way to land a pending self-remove from [`leave`].
///
/// `committer` must already be caught up: a commit staged from a stale epoch
/// is rejected, and a self-remove proposal only becomes visible locally once
/// its notification has been drained and processed.
pub async fn update_key(committer: &CoreUser, chat_id: ChatId) -> anyhow::Result<()> {
    committer.update_key(chat_id).await?;
    Ok(())
}

/// Whether `user` still holds usable local state for `chat_id`, i.e. it is a
/// participant of its own view of the group. Used to tell a target that an
/// operation left in the group from one it evicted.
pub async fn is_member(user: &CoreUser, chat_id: ChatId) -> bool {
    let Some(participants) = user.chat_participants(chat_id).await else {
        return false;
    };
    participants.contains(user.user_id())
}
