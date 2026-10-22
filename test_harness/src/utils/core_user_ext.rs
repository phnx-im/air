// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::Duration;

use aircoreclient::{
    Chat, ChatId, ChatMessage, EventMessage, MarkChatAsRead, Message, MessageId, SystemMessage,
    UsernameRecord,
    clients::{CoreUser, listen_response::Event, process::process_qs::ProcessedQsMessages},
};
use mimi_content::MimiContent;
use tokio::time::timeout;
use tokio_stream::StreamExt;

const MAX_MESSAGES: usize = 100;

/// Add convenience methods to [`CoreUser`] to be used in tests.
#[allow(async_fn_in_trait)]
pub trait CoreUserExt {
    /// Fetch and process the queue, asserting that every message was processes without errors.
    async fn drain(&self) -> ProcessedQsMessages;
    /// Same as [`drain`], but asserts nothing.
    async fn drain_unchecked(&self) -> ProcessedQsMessages;
    /// Ack the queue without processing it.
    async fn discard_queue(&self);

    /// Process the username queue of `record` and return the pending chat a
    /// connection offer in it created, if any.
    async fn drain_username_queue(&self, record: &UsernameRecord) -> Option<ChatId>;
    /// Same as [`drain_username_queue`], but expects a connection offer.
    async fn receive_connection_offer(&self, record: &UsernameRecord) -> ChatId;

    /// Link a new ephemeral device to this user and return it.
    async fn add_linked_device(&self) -> CoreUser;
    /// Same as [`add_linked_device`], but also onboard the new device into this
    /// user's groups and process all follow-up traffic on both devices.
    async fn add_synced_device(&self) -> CoreUser;

    /// Send a text message and return the sent message.
    async fn send_text(&self, chat_id: ChatId, text: &str) -> ChatMessage;
    /// Edit a message and return the edited message.
    async fn send_edit(&self, original: &ChatMessage, text: &str) -> ChatMessage;

    async fn text_messages(&self, chat_id: ChatId) -> Vec<String>;
    async fn system_messages(&self, chat_id: ChatId) -> Vec<SystemMessage>;

    async fn expect_chat(&self, chat_id: ChatId) -> Chat;
    async fn expect_last_message(&self, chat_id: ChatId) -> ChatMessage;
    async fn expect_message(&self, message_id: MessageId) -> ChatMessage;
}

impl CoreUserExt for CoreUser {
    async fn drain(&self) -> ProcessedQsMessages {
        let messages = self.qs_fetch_messages().await.unwrap();
        let expected = messages.len();
        let processed = self.fully_process_qs_messages(messages).await;
        assert!(
            processed.errors.is_empty(),
            "queue processing failed: {:?}",
            processed.errors
        );
        assert_eq!(processed.processed, expected, "not all messages processed");
        processed
    }

    async fn drain_unchecked(&self) -> ProcessedQsMessages {
        let messages = self.qs_fetch_messages().await.unwrap();
        self.fully_process_qs_messages(messages).await
    }

    async fn discard_queue(&self) {
        let (stream, responder) = self.listen_queue().await.unwrap();
        let sequence_number = stream
            .map_while(|message| match message.ok()?.event {
                Some(Event::Message(queue_message)) => Some(queue_message.sequence_number),
                Some(
                    Event::Empty(_)
                    | Event::Payload(_)
                    | Event::VersionStatus(_)
                    | Event::SiblingClientState(_),
                )
                | None => None,
            })
            .fold(0, |_, sequence_number| sequence_number)
            .await;
        responder.ack(sequence_number + 1).await;
    }

    async fn send_text(&self, chat_id: ChatId, text: &str) -> ChatMessage {
        let random_salt = rand::random();
        let content = MimiContent::simple_markdown_message(text.to_owned(), random_salt);
        let message = self
            .send_message(chat_id, content, None, MarkChatAsRead::Yes)
            .await
            .unwrap();
        self.outbound_service().run_once().await;
        self.expect_message(message.id()).await
    }

    async fn send_edit(&self, original: &ChatMessage, text: &str) -> ChatMessage {
        let random_salt = rand::random();
        let content = MimiContent::simple_markdown_message(text.to_owned(), random_salt);
        let message = self
            .send_message(
                original.chat_id(),
                content,
                Some(original.clone()),
                MarkChatAsRead::Yes,
            )
            .await
            .unwrap();
        self.outbound_service().run_once().await;
        self.expect_message(message.id()).await
    }

    async fn system_messages(&self, chat_id: ChatId) -> Vec<SystemMessage> {
        self.messages(chat_id, MAX_MESSAGES)
            .await
            .unwrap()
            .into_iter()
            .filter_map(|message| match message.message() {
                Message::Event(EventMessage::System(system)) => Some(system.clone()),
                _ => None,
            })
            .collect()
    }

    async fn text_messages(&self, chat_id: ChatId) -> Vec<String> {
        self.messages(chat_id, MAX_MESSAGES)
            .await
            .unwrap()
            .iter()
            .filter_map(|message| {
                let content = message.message().mimi_content()?;
                content.string_rendering().ok()
            })
            .collect()
    }

    async fn expect_chat(&self, chat_id: ChatId) -> Chat {
        self.chat(&chat_id)
            .await
            .unwrap_or_else(|| panic!("chat {chat_id:?} not found"))
    }

    async fn expect_last_message(&self, chat_id: ChatId) -> ChatMessage {
        self.last_message(chat_id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("no last message in {chat_id:?}"))
    }

    async fn expect_message(&self, message_id: MessageId) -> ChatMessage {
        self.message(message_id)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("message {message_id:?} not found"))
    }

    async fn drain_username_queue(&self, record: &UsernameRecord) -> Option<ChatId> {
        let (mut stream, responder) = self.listen_username(record).await.unwrap();
        let mut chat_id = None;
        while let Some(Some(message)) = timeout(Duration::from_millis(500), stream.next())
            .await
            .unwrap()
        {
            let message_id = message.message_id.unwrap();
            chat_id = self
                .process_username_queue_message(record.username.clone(), message)
                .await
                .unwrap()
                .map(|stored| stored.chat_id)
                .or(chat_id);
            responder.ack(message_id.into()).await;
        }
        // The acks are only applied once the server closes the stream after we
        // half-closed it. Otherwise the next listen redelivers the messages.
        drop(responder);
        while let Ok(Some(_)) = timeout(Duration::from_secs(5), stream.next()).await {}
        chat_id
    }

    async fn receive_connection_offer(&self, record: &UsernameRecord) -> ChatId {
        self.drain_username_queue(record)
            .await
            .expect("the connection offer should have created a pending chat")
    }

    async fn add_linked_device(&self) -> CoreUser {
        self.link_ephemeral_device().await.unwrap()
    }

    async fn add_synced_device(&self) -> CoreUser {
        let device = self.add_linked_device().await;
        // Onboarding into the pre-existing groups runs in the background.
        device.outbound_service().run_once().await;
        self.drain().await;
        device.drain().await;
        // Linking makes this device upload key packages, which completes only
        // once it fetches its own commit.
        self.outbound_service().run_once().await;
        self.drain().await;
        device.drain().await;
        device
    }
}
