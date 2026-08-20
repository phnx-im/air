// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Thin, non-panicking wrappers around [`CoreUser`] operations.
//!
//! Unlike the invariant-checking helpers in `test_harness`, everything here
//! returns a `Result` instead of asserting: a stress run should keep going
//! and count failures rather than abort on the first divergence.

use std::{str::FromStr, time::Duration};

use aircommon::identifiers::{MimiId, UserId, Username};
use aircoreclient::{ChatId, DisplayName, Message, UserProfile, UsernameRecord, clients::CoreUser};
use anyhow::Context;
use rand::{Rng, RngExt, distr::Alphanumeric, seq::IteratorRandom};
use tokio_stream::StreamExt;
use tracing::debug;

pub struct DrainOutcome {
    pub fetched: usize,
    /// Messages `fully_process_qs_messages` couldn't apply. Coreclient
    /// already rolls each of these back to a savepoint and advances the
    /// ratchet past them, so they don't indicate a stuck queue or corrupted
    /// state. Surfaced as a count rather than failing the drain.
    ///
    /// The dominant source under concurrent walking is
    /// `SecretTreeError(TooDistantInThePast)`, which despite its name means
    /// the message's *epoch* fell out of the receiver's `MAX_PAST_EPOCHS`
    /// window (or predates the receiver's join). The walk advances the group
    /// ~20 epochs per round, the DS forwards application messages without
    /// checking their epoch, and every member's background outbound service
    /// sends read receipts from whatever epoch it last drained to -- so one
    /// stale member's receipt is fanned out and dropped by every receiver.
    /// The rest are messages for a group the member was meanwhile removed
    /// from (`UseAfterEviction`).
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

/// How many times to re-check a side of the handshake before giving up. Each
/// attempt drains again, so this is really "how many fan-out delays to wait
/// out".
const HANDSHAKE_ATTEMPTS: usize = 5;

/// Has `initiator` connect to `responder` by username. `responder` must
/// already have a registered username (see [`ensure_username`]). Drives both
/// sides of the handshake and returns the resulting connection [`ChatId`].
///
/// Verified rather than optimistic: the returned `Ok` means `initiator` really
/// holds `responder` as a contact. Each side of the handshake waits on a fan-out
/// it cannot see the timing of, so a single drain often runs before the message
/// it is waiting for exists -- and a connect that reported success without
/// checking left the contact half-formed and the caller none the wiser. That is
/// how a mesh build can report every edge connected while most members end up
/// with a fraction of their contacts, which in turn starves the walk's invites.
pub async fn connect(
    initiator: &CoreUser,
    responder: &CoreUser,
    responder_record: &UsernameRecord,
) -> anyhow::Result<ChatId> {
    let chat_id = initiator
        .add_contact(responder_record.username.clone(), responder_record.hash)
        .await?
        .map_err(|error| anyhow::anyhow!("add_contact rejected: {error:?}"))?;

    // Responder picks up the connection request from its username queue. The
    // listener is server-streaming, so a bounded drain with a short idle
    // timeout is used instead of waiting indefinitely, retried until the
    // request actually shows up.
    let mut request_seen = false;
    for attempt in 0..HANDSHAKE_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let (mut stream, responder_ack) = responder.listen_username(responder_record).await?;
        let mut acked_any = false;
        while let Some(Some(message)) =
            tokio::time::timeout(Duration::from_millis(500), stream.next())
                .await
                .unwrap_or(None)
        {
            let message_id = message.message_id.context("username message has no id")?;
            // A processing failure must not kill the whole edge: a lost ack
            // (see below) means a later listener gets an already-processed
            // offer redelivered, and re-processing one fails permanently
            // because its one-time package key was deleted when it was
            // accepted. Skip it and keep going; the offer this edge is
            // waiting for is behind it in the queue.
            match responder
                .process_username_queue_message(responder_record.username.clone(), message)
                .await
            {
                Ok(_) => request_seen = true,
                Err(error) => {
                    debug!(%error, "skipping unprocessable username queue message");
                }
            }
            // Acked either way. For a processed offer this is the normal ack;
            // for an unprocessable redelivery it is what stops the message
            // from poisoning every later listen. A real client would keep it,
            // but the harness would otherwise wedge the member for good.
            responder_ack.ack(message_id.into()).await;
            acked_any = true;
        }
        if acked_any {
            // The ack only enqueues into a local channel, and dropping the
            // stream cancels the RPC, discarding unflushed acks. The server
            // sends its empty-queue sentinel right after the last message, so
            // without this grace period the loop exits microseconds after
            // acking and the ack is lost -- the offer is then redelivered to
            // the next listener, which cannot decrypt it (key deleted at
            // accept). Remove once acks are confirmed server-side.
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        if request_seen {
            break;
        }
    }
    if !request_seen {
        anyhow::bail!(
            "connection request from {:?} never reached {:?}'s username queue",
            initiator.user_id(),
            responder.user_id()
        );
    }

    responder
        .accept_contact_request(chat_id)
        .await?
        .map_err(|error| anyhow::anyhow!("accept_contact_request rejected: {error}"))?;

    // Initiator picks up the acceptance to finish materializing its side of the
    // connection group. Retried, because the acceptance has to be fanned out
    // before the drain can see it.
    for attempt in 0..HANDSHAKE_ATTEMPTS {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        drain(initiator).await?;
        if has_contact(initiator, responder.user_id()).await {
            return Ok(chat_id);
        }
    }

    anyhow::bail!(
        "{:?} never materialised a contact for {:?} after accepting",
        initiator.user_id(),
        responder.user_id()
    )
}

/// Whether `user` holds `peer` as a fully established contact.
pub async fn has_contact(user: &CoreUser, peer: &UserId) -> bool {
    user.contacts()
        .await
        .is_ok_and(|contacts| contacts.iter().any(|contact| &contact.user_id == peer))
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

/// Sends a text message carrying `marker`, so receivers can be checked for
/// its arrival. The caller must have drained `user` first: a message built
/// from a stale local epoch, or from a leaf the DS has since blanked, is
/// rejected server-side (e.g. as "unknown sender").
pub async fn send_message(user: &CoreUser, chat_id: ChatId, marker: &str) -> anyhow::Result<()> {
    let suffix: String = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect();
    let salt: [u8; 16] = rand::rng().random();
    let content =
        mimi_content::MimiContent::simple_markdown_message(format!("{marker} {suffix}"), salt);
    user.send_message(chat_id, content, None).await?;
    Ok(())
}

/// Whether one of the most recent messages in `user`'s view of the chat is a
/// content message carrying `marker`. Fan-out to every current member
/// completes within the send request, so a member that was in the group when
/// [`send_message`] returned must see the marker once drained.
pub async fn has_message_with_marker(
    user: &CoreUser,
    chat_id: ChatId,
    marker: &str,
) -> anyhow::Result<bool> {
    let messages = user.messages(chat_id, 30).await?;
    Ok(messages.iter().any(|message| {
        matches!(
            message.message(),
            Message::Content(content)
                if content
                    .content()
                    .string_rendering()
                    .map(|text| text.contains(marker))
                    .unwrap_or(false)
        )
    }))
}

/// Emoji pool for [`react`]. Repeating an emoji on the same message is a
/// no-op in coreclient, so a handful of distinct ones keeps repeat picks of
/// the same message meaningful.
const REACTION_EMOJIS: &[&str] = &["😭", "❤️‍🩹", "🚑", "🔨", "🤯"];

/// What [`react`] did, so receivers can be checked for the reaction: the
/// mimi id of the reacted-to message (the only cross-client message
/// identifier) and the emoji used.
pub struct ReactionProof {
    pub target: MimiId,
    pub emoji: String,
}

/// Adds a random emoji reaction from `user` to a random recent message in
/// the chat, then flushes the outbound service so the reaction is actually
/// sent. Returns `None` if the chat holds no reactable message yet.
///
/// Takes the walk's seeded rng so which message gets which emoji is part of
/// the replayable order of operations.
///
/// The caller must have drained `user` first, like for [`send_message`].
pub async fn react(
    user: &CoreUser,
    chat_id: ChatId,
    rng: &mut (impl Rng + Send),
) -> anyhow::Result<Option<ReactionProof>> {
    let messages = user.messages(chat_id, 20).await?;
    let candidates: Vec<_> = messages
        .iter()
        // Only content messages carry a mimi id to react to.
        .filter(|message| message.message().mimi_id().is_some())
        .collect();
    let Some(target) = candidates.iter().choose(rng) else {
        return Ok(None);
    };
    let target_mimi_id = target
        .message()
        .mimi_id()
        .copied()
        .expect("candidates are filtered on having a mimi id");
    let emoji = REACTION_EMOJIS
        .iter()
        .choose(rng)
        .expect("emoji pool is not empty");
    user.send_reaction(chat_id, target.id(), (*emoji).to_owned())
        .await?;
    // The reaction is only queued; the outbound service sends it.
    user.outbound_service().run_once().await;
    Ok(Some(ReactionProof {
        target: target_mimi_id,
        emoji: (*emoji).to_owned(),
    }))
}

/// Whether `user` sees `reactor`'s `emoji` on the message identified by the
/// proof. `Ok(None)` means the reacted-to message itself is not in `user`'s
/// recent history -- a member that joined after it was sent legitimately
/// never received it, so there is nothing to assert.
pub async fn reaction_visible(
    user: &CoreUser,
    chat_id: ChatId,
    proof: &ReactionProof,
    reactor: &UserId,
) -> anyhow::Result<Option<bool>> {
    let messages = user.messages(chat_id, 40).await?;
    let Some(message) = messages
        .iter()
        .find(|message| message.message().mimi_id() == Some(&proof.target))
    else {
        return Ok(None);
    };
    let reactions = user.message_reactions(message.id()).await?;
    Ok(Some(
        reactions
            .get(&proof.emoji)
            .is_some_and(|users| users.contains(reactor)),
    ))
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
