// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::Cursor;

use aircommon::assert_matches;
use aircoreclient::{AttachmentProgressEvent, store::Store};
use airserver_test_harness::utils::setup::TestBackend;
use base64::{Engine, prelude::BASE64_STANDARD};
use image::{ImageBuffer, Rgba};
use mimi_content::content_container::NestedPartContent;
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
#[tracing::instrument(name = "Send attachment test", skip_all)]
async fn send_attachment() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    let attachment = vec![0x00, 0x01, 0x02, 0x03];
    let (_message_id, external_part) = setup
        .send_attachment(chat_id, &alice, vec![&bob], &attachment, "test.bin")
        .await;

    let attachment_id = match &external_part {
        NestedPartContent::ExternalPart {
            content_type,
            url,
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

            url.parse().unwrap()
        }
        _ => panic!("unexpected attachment type"),
    };

    let bob_test_user = setup.get_user(&bob);
    let bob = &bob_test_user.user;

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
        NestedPartContent::ExternalPart {
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
#[tracing::instrument(name = "Send image attachment test", skip_all)]
async fn send_image_attachment() {
    let mut setup = TestBackend::single().await;
    let alice = setup.add_user().await;
    let bob = setup.add_user().await;
    let chat_id = setup.connect_users(&alice, &bob).await;

    // A base64 encoded blue PNG image 100x75 pixels.
    const SAMPLE_PNG_BASE64: &str = "\
    iVBORw0KGgoAAAANSUhEUgAAAGQAAABLAQMAAAC81rD0AAAABGdBTUEAALGPC/xhBQAAACBjSFJN\
    AAB6JgAAgIQAAPoAAACA6AAAdTAAAOpgAAA6mAAAF3CculE8AAAABlBMVEUAAP7////DYP5JAAAA\
    AWJLR0QB/wIt3gAAAAlwSFlzAAALEgAACxIB0t1+/AAAAAd0SU1FB+QIGBcKN7/nP/UAAAASSURB\
    VDjLY2AYBaNgFIwCdAAABBoAAaNglfsAAAAZdEVYdGNvbW1lbnQAQ3JlYXRlZCB3aXRoIEdJTVDn\
    r0DLAAAAJXRFWHRkYXRlOmNyZWF0ZQAyMDIwLTA4LTI0VDIzOjEwOjU1KzAzOjAwkHdeuQAAACV0\
    RVh0ZGF0ZTptb2RpZnkAMjAyMC0wOC0yNFQyMzoxMDo1NSswMzowMOEq5gUAAAAASUVORK5CYII=";

    let attachment = BASE64_STANDARD.decode(SAMPLE_PNG_BASE64).unwrap();
    let (_message_id, external_part) = setup
        .send_attachment(chat_id, &alice, vec![&bob], &attachment, "test.png")
        .await;

    let alice = setup.get_user(&alice);
    alice.user.outbound_service().run_once().await;

    let attachment_id = match &external_part {
        NestedPartContent::ExternalPart {
            content_type,
            url,
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

            url.parse().unwrap()
        }
        _ => panic!("unexpected attachment type"),
    };

    let bob_test_user = setup.get_user(&bob);
    let bob = &bob_test_user.user;

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
        NestedPartContent::ExternalPart {
            size, content_hash, ..
        } => {
            assert_eq!(content.len() as u64, size);
            let sha256sum = Sha256::digest(&content);
            assert_eq!(sha256sum.as_slice(), content_hash.as_slice());
        }
        _ => panic!("unexpected attachment type"),
    }
}
