// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Sending attachments and the attachment size limit.

use std::{assert_matches, io::Cursor};

use aircoreclient::{
    AttachmentProgressEvent, AttachmentStatus, MimiContentExt, ProvisionAttachmentError,
    VisitedPart, clients::MarkChatAsRead,
};
use airserver_test_harness::utils::{
    core_user_ext::CoreUserExt,
    setup::{TestBackend, TestBackendParams},
};
use base64::{Engine, prelude::BASE64_STANDARD};
use image::{ImageBuffer, Rgba};
use mimi_content::{
    Disposition, MimiContent,
    content_container::{NestedPart, PartSemantics},
};
use png::Encoder;
use sha2::{Digest, Sha256};
use tokio_stream::StreamExt;

pub(crate) fn test_picture_bytes() -> Vec<u8> {
    // Create a new ImgBuf with width: 1px and height: 1px
    let mut img = ImageBuffer::new(200, 200);

    // Put a single pixel in the image
    img.put_pixel(0, 0, Rgba([0u8, 0u8, 255u8, 255u8])); // Blue pixel

    // A Cursor for in-memory writing of bytes
    let mut buffer = Cursor::new(Vec::new());

    {
        // Create a new PNG encoder
        let mut encoder = Encoder::new(&mut buffer, 200, 200);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();

        // Encode the image data.
        writer.write_image_data(&img).unwrap();
    }

    // Get the PNG data bytes
    buffer.into_inner()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn attachment_is_downloadable_by_recipient() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let attachment = vec![0x00, 0x01, 0x02, 0x03];
    let (_message_id, external_part) = setup
        .send_attachment(chat_id, &alice, &[&bob], &attachment, "test.bin")
        .await
        .unwrap();

    match &external_part {
        NestedPart::ExternalPart {
            content_type,
            filename,
            size,
            content_hash,
            ..
        } => {
            assert_eq!(content_type, "application/octet-stream");
            assert_eq!(filename, "test.bin");
            assert_eq!(*size, attachment.len() as u64);

            let sha256sum = Sha256::digest(&attachment);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());
        }
        _ => panic!("unexpected attachment type"),
    };

    let pending_attachments = bob.pending_attachments().await.unwrap();
    assert_eq!(pending_attachments.len(), 1);
    let attachment_id = pending_attachments[0];

    let (progress, download_task) = bob.download_attachment(attachment_id);

    let progress_events = progress.stream().collect::<Vec<_>>();

    let (progress_events, res) = tokio::join!(progress_events, download_task);
    res.expect("Download task failed");

    assert_matches!(
        progress_events.first().unwrap(),
        AttachmentProgressEvent::Init
    );
    assert_matches!(
        progress_events.last().unwrap(),
        AttachmentProgressEvent::Completed
    );

    let content = bob
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap();
    match external_part {
        NestedPart::ExternalPart {
            size, content_hash, ..
        } => {
            assert_eq!(content.len() as u64, size);
            let sha256sum = Sha256::digest(&content);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());
        }
        _ => panic!("unexpected attachment type"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn interrupted_attachment_download_is_resumed() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let attachment = vec![0x00, 0x01, 0x02, 0x03];
    setup
        .send_attachment(chat_id, &alice, &[&bob], &attachment, "test.bin")
        .await
        .unwrap();

    let [attachment_id] = bob.pending_attachments().await.unwrap()[..] else {
        panic!("expected exactly one pending attachment");
    };

    // A process killed during the download leaves the attachment like this
    bob.set_attachment_status(attachment_id, AttachmentStatus::Downloading)
        .await
        .unwrap();
    assert!(bob.pending_attachments().await.unwrap().is_empty());

    bob.reset_interrupted_attachment_downloads().await.unwrap();
    assert_eq!(bob.pending_attachments().await.unwrap(), [attachment_id]);

    let (_progress, download_task) = bob.download_attachment(attachment_id);
    download_task.await.unwrap();

    let content = bob
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes();
    assert_eq!(content, Some(attachment));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn nested_attachments_are_paired_by_part_index() {
    let mut setup = TestBackend::single().await;
    let [alice, bob, carol] = setup.add_users().await;
    let upload_chat_id = setup.connect_users(&alice, &carol).await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // Uploaded in a chat without Bob, since a client stores each remote
    // attachment id only once.
    let first = b"first attachment";
    let second = b"second attachment";
    let (_, first_part) = setup
        .send_attachment(upload_chat_id, &alice, &[&carol], first, "first.bin")
        .await
        .unwrap();
    let (_, second_part) = setup
        .send_attachment(upload_chat_id, &alice, &[&carol], second, "second.bin")
        .await
        .unwrap();

    let multi_part = |parts| NestedPart::MultiPart {
        disposition: Disposition::Render,
        language: String::new(),
        part_semantics: PartSemantics::ProcessAll,
        parts,
    };
    let text = NestedPart::SinglePart {
        disposition: Disposition::Render,
        language: String::new(),
        content_type: "text/markdown".to_owned(),
        content: b"two files".to_vec(),
    };
    // Visit order: second (depth 2), then first (depth 1)
    let content = MimiContent {
        salt: uuid::Uuid::new_v4().as_bytes().to_vec(),
        nested_part: multi_part(vec![
            text,
            multi_part(vec![multi_part(vec![second_part])]),
            first_part,
        ]),
        ..Default::default()
    };

    alice.drain().await;
    alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let messages = bob.drain().await;
    let [message] = &messages.new_messages[..] else {
        panic!("expected exactly one new message");
    };

    let infos = bob.attachment_infos_for_message(message.id()).await;
    let [info_0, info_1] = &infos[..] else {
        panic!("expected exactly two attachments, got {infos:?}");
    };
    assert_eq!(info_0.part_index, 0);
    assert_eq!(info_1.part_index, 1);

    for (info, expected) in [(info_0, &second[..]), (info_1, &first[..])] {
        let (_progress, download_task) = bob.download_attachment(info.attachment_id);
        download_task.await.unwrap();
        let content = bob
            .load_attachment(info.attachment_id)
            .await
            .unwrap()
            .into_bytes();
        assert_eq!(content.as_deref(), Some(expected));
    }

    // Bob's stored copy has no secrets left
    let stored = bob.message(message.id()).await.unwrap().unwrap();
    let mut external_parts = 0;
    stored
        .message()
        .mimi_content()
        .unwrap()
        .visit_parts(|part| {
            let VisitedPart::Attachment { part, .. } = part else {
                return Ok(());
            };
            let NestedPart::ExternalPart {
                key,
                nonce,
                aad,
                content_hash,
                ..
            } = part
            else {
                panic!("unexpected attachment type");
            };
            assert!(key.is_empty());
            assert!(nonce.is_empty());
            assert!(aad.is_empty());
            assert!(content_hash.is_empty());
            external_parts += 1;
            Ok(())
        })
        .unwrap();
    assert_eq!(external_parts, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn reused_remote_attachment_is_skipped() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let (_, external_part) = setup
        .send_attachment(chat_id, &alice, &[&bob], b"attachment", "file.bin")
        .await
        .unwrap();
    let messages = bob.drain().await;
    let [first] = &messages.new_messages[..] else {
        panic!("expected exactly one new message");
    };
    assert_eq!(bob.attachment_infos_for_message(first.id()).await.len(), 1);

    // Same remote attachment id again
    let content = MimiContent {
        salt: uuid::Uuid::new_v4().as_bytes().to_vec(),
        nested_part: NestedPart::MultiPart {
            disposition: Disposition::Render,
            language: String::new(),
            part_semantics: PartSemantics::ProcessAll,
            parts: vec![
                NestedPart::SinglePart {
                    disposition: Disposition::Render,
                    language: String::new(),
                    content_type: "text/markdown".to_owned(),
                    content: b"again".to_vec(),
                },
                external_part,
            ],
        },
        ..Default::default()
    };
    alice.drain().await;
    alice
        .send_message(chat_id, content, None, MarkChatAsRead::Yes)
        .await
        .unwrap();
    alice.outbound_service().run_once().await;

    let messages = bob.drain().await;
    let [second] = &messages.new_messages[..] else {
        panic!("expected exactly one new message");
    };
    assert_ne!(second.id(), first.id());
    let stored = bob.message(second.id()).await.unwrap().unwrap();
    let text = stored
        .message()
        .mimi_content()
        .unwrap()
        .string_rendering()
        .unwrap();
    assert!(text.contains("again"), "unexpected text: {text}");
    assert!(bob.attachment_infos_for_message(second.id()).await.is_empty());
    assert_eq!(bob.attachment_infos_for_message(first.id()).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn image_attachment_is_converted_to_webp() {
    let mut setup = TestBackend::single().await;
    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // A base64 encoded blue PNG image 100x75 pixels.
    // spellchecker:off
    const SAMPLE_PNG_BASE64: &str = "\
    iVBORw0KGgoAAAANSUhEUgAAAGQAAABLAQMAAAC81rD0AAAABGdBTUEAALGPC/xhBQAAACBjSFJN\
    AAB6JgAAgIQAAPoAAACA6AAAdTAAAOpgAAA6mAAAF3CculE8AAAABlBMVEUAAP7////DYP5JAAAA\
    AWJLR0QB/wIt3gAAAAlwSFlzAAALEgAACxIB0t1+/AAAAAd0SU1FB+QIGBcKN7/nP/UAAAASSURB\
    VDjLY2AYBaNgFIwCdAAABBoAAaNglfsAAAAZdEVYdGNvbW1lbnQAQ3JlYXRlZCB3aXRoIEdJTVDn\
    r0DLAAAAJXRFWHRkYXRlOmNyZWF0ZQAyMDIwLTA4LTI0VDIzOjEwOjU1KzAzOjAwkHdeuQAAACV0\
    RVh0ZGF0ZTptb2RpZnkAMjAyMC0wOC0yNFQyMzoxMDo1NSswMzowMOEq5gUAAAAASUVORK5CYII=";
    // spellchecker:on

    let attachment = BASE64_STANDARD.decode(SAMPLE_PNG_BASE64).unwrap();
    let (_message_id, external_part) = setup
        .send_attachment(chat_id, &alice, &[&bob], &attachment, "test.png")
        .await
        .unwrap();

    alice.outbound_service().run_once().await;

    match &external_part {
        NestedPart::ExternalPart {
            content_type,
            filename,
            size,
            content_hash,
            ..
        } => {
            assert_eq!(content_type, "image/webp");
            assert!(
                filename.starts_with("Air--") && filename.ends_with(".webp"),
                "unexpected filename: {filename}"
            );
            assert_eq!(*size, 100);
            assert_eq!(
                content_hash.as_slice(),
                hex::decode("c8cb184c4242c38c3bc8fb26c521377778d9038b9d7dd03f31b9be701269a673")
                    .unwrap()
                    .as_slice()
            );
        }
        _ => panic!("unexpected attachment type"),
    };

    let pending_attachments = bob.pending_attachments().await.unwrap();
    assert_eq!(pending_attachments.len(), 1);
    let attachment_id = pending_attachments[0];

    let (progress, download_task) = bob.download_attachment(attachment_id);

    let progress_events = progress.stream().collect::<Vec<_>>();

    let (progress_events, res) = tokio::join!(progress_events, download_task);
    res.expect("Download task failed");

    assert_matches!(
        progress_events.first().unwrap(),
        AttachmentProgressEvent::Init
    );
    assert_matches!(
        progress_events.last().unwrap(),
        AttachmentProgressEvent::Completed
    );

    let content = bob
        .load_attachment(attachment_id)
        .await
        .unwrap()
        .into_bytes()
        .unwrap();
    match external_part {
        NestedPart::ExternalPart {
            size, content_hash, ..
        } => {
            assert_eq!(content.len() as u64, size);
            let sha256sum = Sha256::digest(&content);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());
        }
        _ => panic!("unexpected attachment type"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn oversized_attachment_is_rejected() {
    const MAX_ATTACHMENT_SIZE: u64 = 1023;

    let mut setup = TestBackend::single_with_params(TestBackendParams {
        max_attachment_size: MAX_ATTACHMENT_SIZE,
        ..Default::default()
    })
    .await;

    let [alice, bob] = setup.add_users().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let attachment = vec![0; MAX_ATTACHMENT_SIZE as usize + 1];

    // 2 bytes TLS size tag, 16 bytes AEAD tag
    let encrypted_size = attachment.len() as u64 + 2 + 16;

    let result = setup
        .send_attachment(chat_id, &alice, &[&bob], &attachment, "test.bin")
        .await;
    let error = result.unwrap_err();
    match error {
        ProvisionAttachmentError::TooLarge(detail) => {
            assert_eq!(detail.max_size_bytes, MAX_ATTACHMENT_SIZE);
            assert_eq!(detail.actual_size_bytes, encrypted_size);
        }
        ProvisionAttachmentError::DecodingError => {
            panic!("wrong error");
        }
    }
}
