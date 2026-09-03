// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    io::{BufRead, Cursor, Seek},
    path::Path,
    sync::LazyLock,
};

use anyhow::Context;
use image::{
    AnimationDecoder, Delay, DynamicImage, GenericImageView, ImageBuffer, ImageDecoder,
    ImageFormat, ImageReader, Rgba,
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
    guess_format,
    metadata::Orientation,
};
use tracing::info;

/// Running blurhash on the full resolution picture is unnecessary (and
/// extremely slow)
const BLURHASH_MAX_EDGE: u32 = 64;
const BLURHASH_COMPONENTS_X: u32 = 4;
const BLURHASH_COMPONENTS_Y: u32 = 3;

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
/// Floor for per-frame durations. Some animated images declare a 0 ms delay
/// expecting the renderer to clamp it, so we ensure each frame contributes a
/// non-zero duration to the resulting WebP timeline.
const MIN_FRAME_DURATION_MS: i32 = 20;

pub(crate) struct ReencodedAttachmentImage {
    pub(crate) webp_image: Vec<u8>,
    pub(crate) image_dimensions: (u32, u32),
    pub(crate) blurhash: String,
}

/// Reads an image's displayed dimensions from its header, without decoding it.
pub(crate) fn probe_attachment_image<P: AsRef<Path>>(
    path: P,
) -> anyhow::Result<Option<(u32, u32)>> {
    probe_image_dimensions(ImageReader::open(path)?.with_guessed_format()?)
}

/// Mirrors [`reencode_image`]'s format dispatch, reading only dimensions.
fn probe_image_dimensions<R>(reader: ImageReader<R>) -> anyhow::Result<Option<(u32, u32)>>
where
    R: BufRead + Seek,
{
    let Some(format) = reader.format() else {
        return Ok(None);
    };

    let dimensions = match format {
        ImageFormat::Gif => oriented_dimensions(GifDecoder::new(reader.into_inner())?),
        ImageFormat::WebP => oriented_dimensions(WebPDecoder::new(reader.into_inner())?),
        ImageFormat::Png => oriented_dimensions(PngDecoder::new(reader.into_inner())?),
        _ => match reader.into_decoder() {
            Ok(decoder) => oriented_dimensions(decoder),
            // format support not compiled in, upload as a regular file
            Err(image::ImageError::Unsupported(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        },
    };

    Ok(Some(dimensions))
}

/// The dimensions the image is displayed at, with the EXIF orientation.
fn oriented_dimensions(mut decoder: impl ImageDecoder) -> (u32, u32) {
    let orientation = decoder.orientation().ok();
    let (width, height) = decoder.dimensions();
    match orientation {
        Some(
            Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH,
        ) => (height, width),
        _ => (width, height),
    }
}

/// Re-encodes an image to WEBP format but also:
/// - Rotates and flips the image according to the EXIF orientation
/// - Resizes the image to a maximum width and height
/// - Converts the image to WebP. Animated GIFs, animated WebPs, and APNGs are
///   re-encoded as animated WebP.
pub(crate) fn reencode_attachment_image(
    bytes: Vec<u8>,
) -> anyhow::Result<ReencodedAttachmentImage> {
    let file_size = bytes.len() as u64;
    // `Cursor<Vec<u8>>` rather than a borrow: the animated branch needs a
    // reader that outlives the frame iterator.
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    reencode_image(reader, file_size)?.context("not a supported image format")
}

fn reencode_image<R>(
    reader: ImageReader<R>,
    file_size: u64,
) -> anyhow::Result<Option<ReencodedAttachmentImage>>
where
    R: BufRead + Seek + 'static,
{
    let Some(format) = reader.format() else {
        return Ok(None);
    };

    let result = match format {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(reader.into_inner())?;
            load_animated_frames(decoder, file_size, format)?
        }
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(reader.into_inner())?;
            if decoder.has_animation() {
                load_animated_frames(decoder, file_size, format)?
            } else {
                load_still_image(decoder, file_size)?
            }
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::new(reader.into_inner())?;
            if decoder.is_apng()? {
                let apng = decoder.apng()?;
                load_animated_frames(apng, file_size, format)?
            } else {
                load_still_image(decoder, file_size)?
            }
        }
        _ => {
            let decoder = match reader.into_decoder() {
                Ok(decoder) => decoder,
                // format support not compiled in, upload as a regular file
                Err(image::ImageError::Unsupported(_)) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            load_still_image(decoder, file_size)?
        }
    };

    Ok(Some(result))
}

/// Classifies an attachment's encoded bytes as animated by reading only the
/// format-specific header chunks.
pub fn image_is_animated(bytes: &[u8]) -> bool {
    let Ok(format) = guess_format(bytes) else {
        return false;
    };
    match format {
        ImageFormat::Gif => true,
        ImageFormat::WebP => WebPDecoder::new(Cursor::new(bytes))
            .map(|decoder| decoder.has_animation())
            .unwrap_or(false),
        ImageFormat::Png => PngDecoder::new(Cursor::new(bytes))
            .ok()
            .and_then(|decoder| decoder.is_apng().ok())
            .unwrap_or(false),
        _ => false,
    }
}

/// Compute the blurhash on a (very) small thumbnail, which produces a very
/// similar result for photos and runs much faster.
fn compute_blurhash(image: &DynamicImage) -> anyhow::Result<String> {
    let thumbnail = image
        .thumbnail(BLURHASH_MAX_EDGE, BLURHASH_MAX_EDGE)
        .to_rgba8();
    let (width, height) = thumbnail.dimensions();
    Ok(blurhash::encode(
        BLURHASH_COMPONENTS_X,
        BLURHASH_COMPONENTS_Y,
        width,
        height,
        &thumbnail,
    )?)
}

/// A flat neutral blurhash, stored until the real one has been computed.
pub(crate) fn placeholder_blurhash() -> &'static str {
    static PLACEHOLDER: LazyLock<String> = LazyLock::new(|| {
        let gray = [0x80, 0x80, 0x80, 0xff];
        blurhash::encode(BLURHASH_COMPONENTS_X, BLURHASH_COMPONENTS_Y, 1, 1, &gray)
            .expect("component counts are in range")
    });
    &PLACEHOLDER
}

/// Decodes a still image and re-encodes it as a static WebP.
fn load_still_image<D: ImageDecoder>(
    mut decoder: D,
    file_size: u64,
) -> anyhow::Result<ReencodedAttachmentImage> {
    let orientation = decoder.orientation().ok();

    let image = DynamicImage::from_decoder(decoder)?;

    // TODO: use image crate to resize to thumbnail IF too big
    // and then surface it in UI

    let mut image = resize(
        image,
        MAX_ATTACHMENT_IMAGE_WIDTH,
        MAX_ATTACHMENT_IMAGE_HEIGHT,
    );
    if let Some(orientation) = orientation {
        image.apply_orientation(orientation);
    }

    let blurhash = compute_blurhash(&image)?;

    let image_rgba = image.to_rgba8();
    let (width, height) = image_rgba.dimensions();

    let webp_data = webpx::Encoder::new_rgba(&image_rgba, width, height)
        .quality(ATTACHMENT_IMAGE_QUALITY_PERCENT)
        .encode(webpx::Unstoppable)
        .context("WebP encode failed")?;

    info!(
        from_bytes = file_size,
        to_bytes = webp_data.len(),
        "Reencoded attachment image as WebP",
    );

    Ok(ReencodedAttachmentImage {
        webp_image: webp_data,
        image_dimensions: (width, height),
        blurhash,
    })
}

/// Decodes an animated image (GIF, animated WebP, APNG) and re-encodes it as
/// animated WebP. Also returns the dimensions of the first frame and a
/// blurhash generated from it.
fn load_animated_frames<'a, D: AnimationDecoder<'a>>(
    decoder: D,
    file_size: u64,
    source: ImageFormat,
) -> anyhow::Result<ReencodedAttachmentImage> {
    let mut frames = decoder.into_frames();

    let first = frames
        .next()
        .ok_or_else(|| anyhow::anyhow!("{source:?} has no frames"))??;
    let first_delay = first.delay();

    let first_buffer = fit_to_max(
        first.into_buffer(),
        MAX_ATTACHMENT_IMAGE_WIDTH,
        MAX_ATTACHMENT_IMAGE_HEIGHT,
    );
    let (width, height) = first_buffer.dimensions();
    let first_dynamic_image = DynamicImage::ImageRgba8(first_buffer);

    let blurhash = compute_blurhash(&first_dynamic_image)?;

    let mut encoder = webpx::AnimationEncoder::with_options(width, height, true, 0)
        .context("WebP encoder init failed")?;
    encoder.set_quality(ATTACHMENT_IMAGE_QUALITY_PERCENT);

    let mut timestamp_ms: i32 = 0;
    encoder
        .add_frame_rgba(first_dynamic_image.as_bytes(), timestamp_ms)
        .context("WebP add_frame failed")?;
    timestamp_ms = timestamp_ms.saturating_add(delay_to_ms(first_delay));

    for frame_result in frames {
        let frame = frame_result?;
        let frame_delay = frame.delay();
        let resized = fit_to_max(
            frame.into_buffer(),
            MAX_ATTACHMENT_IMAGE_WIDTH,
            MAX_ATTACHMENT_IMAGE_HEIGHT,
        );
        // The dimensions should never change mid-stream.
        if resized.dimensions() != (width, height) {
            anyhow::bail!("{source:?} frame dimensions changed mid-stream");
        }
        encoder
            .add_frame_rgba(resized.as_raw(), timestamp_ms)
            .context("WebP add_frame failed")?;
        timestamp_ms = timestamp_ms.saturating_add(delay_to_ms(frame_delay));
    }

    let webp_data = encoder
        .finish(timestamp_ms)
        .context("WebP finalize failed")?;

    info!(
        from_bytes = file_size,
        to_bytes = webp_data.len(),
        ?source,
        "Reencoded animated image as animated WebP",
    );

    Ok(ReencodedAttachmentImage {
        webp_image: webp_data,
        image_dimensions: (width, height),
        blurhash,
    })
}

/// Converts a frame delay to milliseconds, applying a floor to avoid
/// zero-duration frames.
fn delay_to_ms(delay: Delay) -> i32 {
    let (n, d) = delay.numer_denom_ms();
    let ms = n.checked_div(d).unwrap_or(0);
    let ms_i32: i32 = ms.try_into().unwrap_or(i32::MAX);
    ms_i32.max(MIN_FRAME_DURATION_MS)
}

/// Resizes the image to fit within the given dimensions, preserving aspect
/// ratio.
fn fit_to_max(
    buffer: ImageBuffer<Rgba<u8>, Vec<u8>>,
    max_width: u32,
    max_height: u32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (w, h) = buffer.dimensions();
    if w <= max_width && h <= max_height {
        return buffer;
    }
    DynamicImage::ImageRgba8(buffer)
        .resize(max_width, max_height, image::imageops::FilterType::Lanczos3)
        .to_rgba8()
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
