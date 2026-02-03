// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{fs, io::Cursor, path::Path};

use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};
use tracing::info;

const MAX_PROFILE_IMAGE_WIDTH: u32 = 256;
const MAX_PROFILE_IMAGE_HEIGHT: u32 = 256;

pub(crate) fn resize_profile_image(image_bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut decoder = ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()?
        .into_decoder()?;

    let orientation = decoder.orientation().ok();

    // Decode, resize and rotate the image
    let image = DynamicImage::from_decoder(decoder)?;
    let mut image = resize(image, MAX_PROFILE_IMAGE_WIDTH, MAX_PROFILE_IMAGE_HEIGHT);
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }

    // Save the resized image
    let mut buf = Vec::new();
    let mut cursor = Cursor::new(&mut buf);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 90);
    encoder.encode_image(&image)?;
    info!(
        from_bytes = image_bytes.len(),
        to_bytes = buf.len(),
        "Resized profile image",
    );
    Ok(buf)
}

const ATTACHMENT_IMAGE_QUALITY_PERCENT: f32 = 90.0;
const MAX_ATTACHMENT_IMAGE_WIDTH: u32 = 4096;
const MAX_ATTACHMENT_IMAGE_HEIGHT: u32 = 4096;

pub(crate) struct ReencodedAttachmentImage {
    pub(crate) webp_image: Vec<u8>,
    pub(crate) image_dimensions: (u32, u32),
    pub(crate) blurhash: String,
}

/// Loads an image and re-encodes it to WEBP format.
///
/// If the path is not an image, returns `None`.
///
/// This does several things:
/// - Rotates and flips the image according to the EXIF orientation
/// - Resizes the image to a maximum width and height of 4096x4096
/// - Converts the image to WebP
pub(crate) fn load_attachment_image(
    path: &Path,
) -> anyhow::Result<Option<ReencodedAttachmentImage>> {
    let file_size = fs::metadata(path)?.len();

    let reader = ImageReader::open(path)?.with_guessed_format()?;
    if reader.format().is_none() {
        return Ok(None);
    }

    let mut decoder = reader.into_decoder()?;

    let orientation = decoder.orientation().ok();

    let image = DynamicImage::from_decoder(decoder)?;
    let mut image = resize(
        image,
        MAX_ATTACHMENT_IMAGE_WIDTH,
        MAX_ATTACHMENT_IMAGE_HEIGHT,
    );
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }

    // TODO: Preserve format instead of converting to WebP

    let image_rgba = image.to_rgba8();
    let (width, height) = image_rgba.dimensions();

    let webp_image = webp::Encoder::from_rgba(&image_rgba, width, height)
        .encode(ATTACHMENT_IMAGE_QUALITY_PERCENT);

    // `blurhash::encode` can only fail if the compoments dimension is out of range
    // => We should never get an error here.
    let blurhash = blurhash::encode(4, 3, width, height, &image_rgba)?;

    info!(
        from_bytes = file_size,
        to_bytes = webp_image.len(),
        "Reencoded attachment image as WebP",
    );

    // Note: We need to convert WebPMemory to Vec here, because the former is not Send.
    Ok(Some(ReencodedAttachmentImage {
        webp_image: webp_image.to_vec(),
        image_dimensions: (width, height),
        blurhash,
    }))
}

/// Resizes the image to fit within the given dimensions.
///
/// If the image is already smaller than the given dimensions, it is returned.
fn resize(image: DynamicImage, max_width: u32, max_height: u32) -> DynamicImage {
    let (width, height) = image.dimensions();
    if width <= max_width && height <= max_height {
        return image;
    }
    image.resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
}
