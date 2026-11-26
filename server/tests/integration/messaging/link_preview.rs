// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Link previews in new messages and in edits, incl. failed image uploads.

use std::{assert_matches, time::Duration};

use aircoreclient::{
    AttachmentContent, AttachmentFile, AttachmentId, AttachmentStatus, ChatId, ChatMessage,
    ComposedLinkPreview, ComposedMessage, MessageId, MimiContentExt, UploadTaskError,
    clients::CoreUser,
    link_preview::{FetchedLinkPreview, LinkPreview},
};
use airprotos::client::link_preview::{LinkPreviewCard, LinkPreviewImage, LinkPreviewKind};
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams},
};
use mimi_content::MessageStatus;

use crate::messaging::attachments::test_picture_bytes;

const LINK_A: &str = "https://a.org";
const LINK_B: &str = "https://b.org";

const IMAGE_A: &[u8] = &[0xA; 64];
const IMAGE_B: &[u8] = &[0xB; 96];

/// Card of `url`, with an image if `image` is given.
fn fetched(url: &str, image: Option<&[u8]>) -> ComposedLinkPreview {
    let card_image = image.map(|_| LinkPreviewImage::new(3, 4, 3, None, None).unwrap());
    let card = LinkPreviewCard::new(
        url.to_owned(),
        "title",
        "",
        LinkPreviewKind::Default,
        card_image,
    )
    .unwrap();
    let preview = FetchedLinkPreview::new(card, image.map(<[u8]>::to_vec)).unwrap();
    ComposedLinkPreview::Fetched(preview)
}

/// Runs the uploads of a composed message and sends it.
async fn deliver(user: &CoreUser, composed: ComposedMessage) -> ChatMessage {
    let ComposedMessage { message, uploads } = composed;
    for upload in uploads {
        upload.task.await.expect("upload failed");
    }
    user.outbound_service().run_once().await;
    user.expect_message(message.id()).await
}

async fn text(user: &CoreUser, message_id: MessageId) -> String {
    let message = user.expect_message(message_id).await;
    let content = message.message().mimi_content().unwrap();
    content.markdown_text().unwrap().to_owned()
}

/// Url of the card of the message and the attachment of its image.
async fn card(user: &CoreUser, message_id: MessageId) -> Option<(String, Option<AttachmentId>)> {
    let message = user.expect_message(message_id).await;
    let preview = LinkPreview::from_content(message.message().mimi_content().unwrap())?;
    let image = match preview.image() {
        Some((_image, part_index)) => {
            let attachments = user.attachment_infos_for_message(message_id).await;
            let info = attachments
                .find(part_index, None)
                .expect("no attachment for the card image");
            Some(info.attachment_id)
        }
        None => None,
    };
    Some((preview.card().url().to_owned(), image))
}

async fn download(user: &CoreUser, attachment_id: AttachmentId) -> Vec<u8> {
    let (_progress, task) = user.download_attachment(attachment_id);
    task.await.expect("download failed");
    user.load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap()
}

/// Alice sends a message with the card of [`LINK_A`] and its image to Bob.
///
/// Returns the message as stored by Alice and by Bob.
async fn send_with_card(
    alice: &CoreUser,
    bob: &CoreUser,
    chat_id: ChatId,
) -> (ChatMessage, MessageId) {
    alice.drain().await;
    bob.drain().await;

    let composed = alice
        .compose(chat_id)
        .text(format!("see {LINK_A}"))
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();
    let message = deliver(alice, composed).await;
    assert!(message.is_sent());

    bob.drain().await;
    let received = bob.expect_last_message(chat_id).await;
    (message, received.id())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn message_with_card_and_image_is_received() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let (_message, received_id) = send_with_card(&alice, &bob, chat_id).await;

    assert_eq!(text(&bob, received_id).await, format!("see {LINK_A}"));
    let (url, image) = card(&bob, received_id).await.expect("no card");
    assert_eq!(url, LINK_A);
    assert_eq!(download(&bob, image.expect("no image")).await, IMAGE_A);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn card_and_attachment_are_rejected() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    alice.drain().await;
    bob.drain().await;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("picture.png");
    std::fs::write(&path, test_picture_bytes()).unwrap();
    let staged = alice
        .stage_attachment(AttachmentFile::borrowed(path))
        .await
        .unwrap()
        .unwrap();

    let composed = alice
        .compose(chat_id)
        .text(format!("see {LINK_A}"))
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .attachment(staged)
        .send();
    assert!(Box::pin(composed).await.is_err());

    alice.outbound_service().run_once().await;
    assert!(bob.drain().await.new_messages.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_with_another_link_replaces_card_and_image() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let (message, received_id) = send_with_card(&alice, &bob, chat_id).await;
    let (_url, old_image) = card(&bob, received_id).await.expect("no card");
    let old_image = old_image.expect("no image");
    download(&bob, old_image).await;

    let composed = alice
        .compose(chat_id)
        .text(format!("see {LINK_B}"))
        .replaces(message)
        .link_preview(fetched(LINK_B, Some(IMAGE_B)))
        .send()
        .await
        .unwrap();
    deliver(&alice, composed).await;
    bob.drain().await;

    assert_eq!(text(&bob, received_id).await, format!("see {LINK_B}"));
    let (url, image) = card(&bob, received_id).await.expect("no card");
    assert_eq!(url, LINK_B);
    let image = image.expect("no image");
    assert_ne!(image, old_image);
    assert_eq!(download(&bob, image).await, IMAGE_B);

    // The image of the replaced card is gone
    assert_eq!(bob.attachment_infos_for_message(received_id).await.len(), 1);
    assert_matches!(
        bob.load_attachment(old_image).await.unwrap(),
        AttachmentContent::None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_adds_card_and_image_to_plain_message() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;
    let message = alice.send_text(chat_id, "hello").await;
    bob.drain().await;
    let received_id = bob.expect_last_message(chat_id).await.id();
    assert!(card(&bob, received_id).await.is_none());

    let composed = alice
        .compose(chat_id)
        .text(format!("hello {LINK_A}"))
        .replaces(message)
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();
    deliver(&alice, composed).await;
    bob.drain().await;

    let (url, image) = card(&bob, received_id).await.expect("no card");
    assert_eq!(url, LINK_A);
    assert_eq!(download(&bob, image.expect("no image")).await, IMAGE_A);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_keeping_the_card_keeps_its_image() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let (message, received_id) = send_with_card(&alice, &bob, chat_id).await;
    let (_url, image) = card(&bob, received_id).await.expect("no card");
    let image = image.expect("no image");
    download(&bob, image).await;

    let composed = alice
        .compose(chat_id)
        .text(format!("look at {LINK_A}"))
        .replaces(message)
        .link_preview(ComposedLinkPreview::Original)
        .send()
        .await
        .unwrap();
    assert!(composed.uploads.is_empty());
    deliver(&alice, composed).await;
    bob.drain().await;

    assert_eq!(text(&bob, received_id).await, format!("look at {LINK_A}"));
    let (url, kept_image) = card(&bob, received_id).await.expect("no card");
    assert_eq!(url, LINK_A);
    assert_eq!(kept_image, Some(image));

    // Still downloaded
    let content = bob.load_attachment(image).await.unwrap();
    assert_eq!(content.into_bytes().as_deref(), Some(IMAGE_A));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_without_card_drops_card_and_image() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let (message, received_id) = send_with_card(&alice, &bob, chat_id).await;
    let (_url, image) = card(&bob, received_id).await.expect("no card");
    let image = image.expect("no image");

    let composed = alice
        .compose(chat_id)
        .text(format!("just {LINK_A}"))
        .replaces(message)
        .send()
        .await
        .unwrap();
    deliver(&alice, composed).await;
    bob.drain().await;

    assert_eq!(text(&bob, received_id).await, format!("just {LINK_A}"));
    assert!(card(&bob, received_id).await.is_none());
    assert_eq!(bob.attachment_infos_for_message(received_id).await.len(), 0);
    assert_matches!(
        bob.load_attachment(image).await.unwrap(),
        AttachmentContent::None
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn edit_survives_a_rejected_image_upload() {
    // Smaller than the image of the new card
    const MAX_ATTACHMENT_SIZE: u64 = 32;

    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_attachment_size: MAX_ATTACHMENT_SIZE,
        ..Default::default()
    })
    .await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;
    let message = alice.send_text(chat_id, "hello").await;
    bob.drain().await;
    let received_id = bob.expect_last_message(chat_id).await.id();

    let ComposedMessage {
        message: edited,
        uploads,
    } = alice
        .compose(chat_id)
        .text(format!("hello {LINK_A}"))
        .replaces(message)
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();
    let [upload] = uploads.try_into().ok().expect("one upload");
    assert_matches!(upload.task.await, Err(UploadTaskError::Provision(_)));

    // The edit is kept and marked as failed
    assert_eq!(text(&alice, edited.id()).await, format!("hello {LINK_A}"));
    let stored = alice.expect_message(edited.id()).await;
    assert_eq!(stored.status(), MessageStatus::Error);
    assert!(!stored.is_sent());

    // Nothing reaches Bob
    alice.outbound_service().run_once().await;
    bob.drain().await;
    assert_eq!(text(&bob, received_id).await, "hello");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn held_edit_is_delivered_after_a_retry() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;
    let message = alice.send_text(chat_id, "hello").await;
    bob.drain().await;
    let received_id = bob.expect_last_message(chat_id).await.id();

    let ComposedMessage {
        message: edited,
        uploads,
    } = alice
        .compose(chat_id)
        .text(format!("hello {LINK_A}"))
        .replaces(message)
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();

    // The upload is cancelled before it ran
    let [upload] = uploads.try_into().ok().expect("one upload");
    let attachment_id = upload.attachment_id;
    drop(upload);
    alice.fail_cancelled_upload(attachment_id).await.unwrap();

    let stored = alice.expect_message(edited.id()).await;
    assert_eq!(stored.status(), MessageStatus::Error);
    alice.outbound_service().run_once().await;
    bob.drain().await;
    assert_eq!(text(&bob, received_id).await, "hello");

    let uploads = alice.retry_message(edited.id()).await.unwrap();
    assert_eq!(uploads.len(), 1);
    for upload in uploads {
        upload.task.await.expect("upload failed");
    }
    alice.outbound_service().run_once().await;
    assert!(alice.expect_message(edited.id()).await.is_sent());

    bob.drain().await;
    assert_eq!(text(&bob, received_id).await, format!("hello {LINK_A}"));
    let (url, image) = card(&bob, received_id).await.expect("no card");
    assert_eq!(url, LINK_A);
    assert_eq!(download(&bob, image.expect("no image")).await, IMAGE_A);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn failed_upload_releases_later_messages_of_its_chat() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;
    let ComposedMessage {
        message: first,
        uploads,
    } = alice
        .compose(chat_id)
        .text(format!("first {LINK_A}"))
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();
    let second = alice
        .compose(chat_id)
        .text("second".to_owned())
        .send()
        .await
        .unwrap()
        .message;

    // The service stays started and idle, so only a wake-up sends a message
    alice.outbound_service().start_and_wait().await;
    assert!(!alice.expect_message(second.id()).await.is_sent());

    // The upload is cancelled before it ran
    let [upload] = uploads.try_into().ok().expect("one upload");
    let attachment_id = upload.attachment_id;
    drop(upload);
    alice.fail_cancelled_upload(attachment_id).await.unwrap();

    let second_is_sent = async {
        while !alice.expect_message(second.id()).await.is_sent() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(10), second_is_sent)
        .await
        .expect("the second message is not sent");

    let first = alice.expect_message(first.id()).await;
    assert_eq!(first.status(), MessageStatus::Error);
    assert!(!first.is_sent());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn message_with_failed_upload_does_not_fail_other_chats() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, carol] = setup.add_users().await;
    let chat_bob = setup.connect_users(&alice, &bob).await;
    let chat_carol = setup.connect_users(&alice, &carol).await;
    alice.drain().await;

    let ComposedMessage {
        message: with_card,
        uploads,
    } = alice
        .compose(chat_bob)
        .text(format!("see {LINK_A}"))
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();
    let plain = alice
        .compose(chat_carol)
        .text("hello".to_owned())
        .send()
        .await
        .unwrap()
        .message;

    // The upload is failed without holding the message, so it stays queued
    let [upload] = uploads.try_into().ok().expect("one upload");
    let attachment_id = upload.attachment_id;
    drop(upload);
    alice
        .set_attachment_status(attachment_id, AttachmentStatus::UploadFailed)
        .await
        .unwrap();

    alice.outbound_service().run_once().await;

    let with_card = alice.expect_message(with_card.id()).await;
    assert_eq!(with_card.status(), MessageStatus::Error);
    assert!(!with_card.is_sent());
    assert!(alice.expect_message(plain.id()).await.is_sent());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn interrupted_upload_is_resumed() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;
    let texts_before = bob.text_messages(chat_id).await.len();

    let ComposedMessage { message, uploads } = alice
        .compose(chat_id)
        .text(format!("see {LINK_A}"))
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();

    // A killed app never runs the upload and leaves it as running
    drop(uploads);
    alice.outbound_service().run_once().await;
    bob.drain().await;
    assert_eq!(bob.text_messages(chat_id).await.len(), texts_before);

    let uploads = alice.resume_interrupted_uploads().await.unwrap();
    assert_eq!(uploads.len(), 1);
    for upload in uploads {
        upload.task.await.expect("upload failed");
    }
    alice.outbound_service().run_once().await;
    assert!(alice.expect_message(message.id()).await.is_sent());

    // Nothing is left to resume
    assert!(alice.resume_interrupted_uploads().await.unwrap().is_empty());

    bob.drain().await;
    let received_id = bob.expect_last_message(chat_id).await.id();
    let (url, image) = card(&bob, received_id).await.expect("no card");
    assert_eq!(url, LINK_A);
    assert_eq!(download(&bob, image.expect("no image")).await, IMAGE_A);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn message_waiting_for_its_upload_holds_back_its_chat() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    alice.drain().await;
    bob.drain().await;
    let texts_before = bob.text_messages(chat_id).await.len();

    let ComposedMessage {
        message: first,
        uploads,
    } = alice
        .compose(chat_id)
        .text(format!("first {LINK_A}"))
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await
        .unwrap();
    let second = alice.send_text(chat_id, "second").await;
    assert!(!second.is_sent());

    bob.drain().await;
    assert_eq!(bob.text_messages(chat_id).await.len(), texts_before);

    for upload in uploads {
        upload.task.await.expect("upload failed");
    }
    alice.outbound_service().run_once().await;
    assert!(alice.expect_message(first.id()).await.is_sent());
    assert!(alice.expect_message(second.id()).await.is_sent());

    bob.drain().await;
    let messages = bob.messages(chat_id, 2).await.unwrap();
    let [received_first, received_second] = &messages[..] else {
        panic!("expected two messages");
    };
    assert_eq!(
        text(&bob, received_first.id()).await,
        format!("first {LINK_A}")
    );
    assert_eq!(text(&bob, received_second.id()).await, "second");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn badly_composed_message_is_refused() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;
    alice.drain().await;
    let messages_before = alice.messages(chat_id, 10).await.unwrap().len();

    // Nothing to send
    assert!(alice.compose(chat_id).send().await.is_err());

    // A card without a text
    let composed = alice
        .compose(chat_id)
        .link_preview(fetched(LINK_A, Some(IMAGE_A)))
        .send()
        .await;
    assert!(composed.is_err());

    // The card of a replaced message without a replaced message
    let composed = alice
        .compose(chat_id)
        .text(format!("see {LINK_A}"))
        .link_preview(ComposedLinkPreview::Original)
        .send()
        .await;
    assert!(composed.is_err());

    let messages_after = alice.messages(chat_id, 10).await.unwrap().len();
    assert_eq!(messages_after, messages_before);
}
