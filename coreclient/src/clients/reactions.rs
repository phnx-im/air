// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use aircommon::{
    identifiers::{MimiId, UserId},
    time::TimeStamp,
};
use anyhow::{Context, bail};
use indexmap::IndexMap;

use crate::{
    Chat, ChatId, ChatMessage, MessageId,
    chats::reactions::{Reaction, reaction_content, reaction_tombstone_content},
    clients::block_contact::BlockedContactError,
    db::access::WriteConnection,
};

use super::CoreUser;

impl CoreUser {
    /// Add an emoji reaction to a message and send it to the other group members.
    ///
    /// Reacting again with the same emoji on the same message is a no-op. A user
    /// can react to a message with multiple different emojis.
    pub async fn send_reaction(
        &self,
        chat_id: ChatId,
        target: MessageId,
        emoji: String,
    ) -> anyhow::Result<()> {
        Box::pin(
            self.db()
                .with_write_transaction(async |txn| -> anyhow::Result<()> {
                    if Chat::is_blocked(&mut *txn, chat_id).await? {
                        bail!(BlockedContactError);
                    }

                    let target_message = ChatMessage::load(&mut *txn, target)
                        .await?
                        .with_context(|| format!("Can't find message with id {target:?}"))?;
                    let target_mimi_id = target_message
                        .message()
                        .mimi_id()
                        .copied()
                        .context("Can't react to a message without a MimiId")?;

                    let chat = Chat::load(&mut *txn, &chat_id)
                        .await?
                        .with_context(|| format!("Can't find chat with id {chat_id}"))?;

                    let sender = self.user_id().clone();
                    let content = reaction_content(&target_mimi_id, &emoji)?;
                    let reaction_mimi_id = MimiId::calculate(chat.group_id(), &sender, &content)?;

                    let reaction = Reaction::new(
                        reaction_mimi_id,
                        target_mimi_id,
                        chat_id,
                        sender,
                        emoji,
                        TimeStamp::now(),
                    );

                    // Idempotent: if we already reacted with this emoji, do nothing.
                    if !reaction.store(&mut *txn).await? {
                        return Ok(());
                    }

                    let bytes = content.serialize()?;
                    self.outbound_service()
                        .enqueue_reaction_in_transaction(
                            txn,
                            chat_id,
                            Some(&reaction_mimi_id),
                            &bytes,
                        )
                        .await?;

                    txn.notifier().update(target);
                    Ok(())
                }),
        )
        .await
    }

    /// Load all reactions on a message, ordered oldest first.
    ///
    /// In group chats this is also how the UI shows who reacted with what.
    pub async fn message_reactions(
        &self,
        message_id: MessageId,
    ) -> anyhow::Result<IndexMap<String, Vec<UserId>>> {
        let mut connection = self.db().read().await?;
        let Some(message) = ChatMessage::load(&mut connection, message_id).await? else {
            return Ok(IndexMap::new());
        };
        let Some(target_mimi_id) = message.message().mimi_id().copied() else {
            return Ok(IndexMap::new());
        };
        let reactions = Reaction::load_by_target(&mut connection, &target_mimi_id).await?;
        Ok(reactions
            .into_iter()
            .fold(IndexMap::new(), |mut reactions, reaction| {
                reactions
                    .entry(reaction.emoji)
                    .or_default()
                    .push(reaction.sender);
                reactions
            }))
    }

    /// Remove an emoji reaction we previously added to a message, and send the
    /// retraction to the other group members.
    ///
    /// A no-op if we have no such reaction.
    pub async fn delete_reaction(
        &self,
        chat_id: ChatId,
        target: MessageId,
        emoji: String,
    ) -> anyhow::Result<()> {
        Box::pin(
            self.db()
                .with_write_transaction(async |txn| -> anyhow::Result<()> {
                    let target_message = ChatMessage::load(&mut *txn, target)
                        .await?
                        .with_context(|| format!("Can't find message with id {target:?}"))?;
                    let Some(target_mimi_id) = target_message.message().mimi_id().copied() else {
                        return Ok(());
                    };

                    let sender = self.user_id().clone();
                    let Some(reaction_mimi_id) =
                        Reaction::load_mimi_id(&mut *txn, &target_mimi_id, &sender, &emoji).await?
                    else {
                        return Ok(()); // we have no such reaction
                    };

                    Reaction::delete_by_mimi_id(&mut *txn, &reaction_mimi_id).await?;

                    let content = reaction_tombstone_content(&target_mimi_id, &reaction_mimi_id)?;
                    let bytes = content.serialize()?;
                    self.outbound_service()
                        .enqueue_reaction_in_transaction(txn, chat_id, None, &bytes)
                        .await?;

                    txn.notifier().update(target);
                    Ok(())
                }),
        )
        .await
    }
}
