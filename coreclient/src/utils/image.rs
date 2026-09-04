// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    io::{BufRead, Cursor, Seek},
    path::Path,
    sync::LazyLock,
};

use anyhow::{Context, ensure};
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

const THUMBNAIL_MAX_EDGE: u32 = 1024;
const THUMBNAIL_QUALITY_PERCENT: f32 = 80.0;

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
    pub(crate) is_animated: bool,
    /// WebP encoded thumbnail, or `None` if the original fits as thumbnail.
    /// Always set for animated sources (static first frame).
    pub(crate) thumbnail: Option<Vec<u8>>,
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

    let thumbnail = if width.max(height) <= THUMBNAIL_MAX_EDGE {
        None
    } else {
        let thumbnail = image
            .resize(
                THUMBNAIL_MAX_EDGE,
                THUMBNAIL_MAX_EDGE,
                image::imageops::FilterType::Lanczos3,
            )
            .into_rgba8();
        let (width, height) = thumbnail.dimensions();
        Some(encode_thumbnail_webp(thumbnail.as_raw(), width, height)?)
    };

    info!(
        from_bytes = file_size,
        to_bytes = webp_data.len(),
        "Reencoded attachment image as WebP",
    );

    Ok(ReencodedAttachmentImage {
        webp_image: webp_data,
        image_dimensions: (width, height),
        blurhash,
        is_animated: false,
        thumbnail,
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

    // Animated sources always store a static first-frame thumbnail, so the
    // thumbnail path never hands animated bytes to a static surface.
    let thumbnail = {
        let buffer = fit_to_max(
            first_dynamic_image.into_rgba8(),
            THUMBNAIL_MAX_EDGE,
            THUMBNAIL_MAX_EDGE,
        );
        let (width, height) = buffer.dimensions();
        encode_thumbnail_webp(buffer.as_raw(), width, height)?
    };

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
        is_animated: true,
        thumbnail: Some(thumbnail),
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

pub(crate) enum ThumbnailImage {
    Encoded {
        /// WebP encoded thumbnail
        bytes: Vec<u8>,
        /// Whether the source is animated
        is_animated: bool,
    },
    /// Long edge is already within bounds and the image is *not* animated.
    OriginalFits,
}

/// Produces a static thumbnail from an attachment's stored WebP bytes.
///
/// Animated sources yield their first frame, and never `OriginalFits`, so the thumbnail path never
/// hands animated bytes to a static surface.
pub(crate) fn encode_thumbnail(original: &[u8]) -> anyhow::Result<ThumbnailImage> {
    let decoder = webpx::Decoder::new(original).context("WebP decode failed")?;
    let (width, height, has_animation) = {
        let info = decoder.info();
        (info.width, info.height, info.has_animation)
    };

    if has_animation {
        return encode_animated_thumbnail(original);
    }

    ensure_within_supported_cap(width, height)?;

    let Some((target_width, target_height)) =
        thumbnail_dimensions(width, height, THUMBNAIL_MAX_EDGE)
    else {
        return Ok(ThumbnailImage::OriginalFits);
    };

    let (rgba, width, height) = decoder
        .scale(target_width, target_height)
        .decode_rgba_raw()
        .context("WebP scaled decode failed")?;
    Ok(ThumbnailImage::Encoded {
        bytes: encode_thumbnail_webp(&rgba, width, height)?,
        is_animated: false,
    })
}

fn encode_animated_thumbnail(original: &[u8]) -> anyhow::Result<ThumbnailImage> {
    let mut decoder = webpx::AnimationDecoder::new(original)?;
    let (width, height) = {
        let info = decoder.info();
        (info.width, info.height)
    };
    ensure_within_supported_cap(width, height)?;

    let frame = decoder
        .next_frame()?
        .context("animated WebP has no frames")?;
    let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(frame.width, frame.height, frame.data)
        .context("frame does not match its buffer size")?;
    let buffer = fit_to_max(buffer, THUMBNAIL_MAX_EDGE, THUMBNAIL_MAX_EDGE);
    let (width, height) = buffer.dimensions();

    Ok(ThumbnailImage::Encoded {
        bytes: encode_thumbnail_webp(buffer.as_raw(), width, height)?,
        is_animated: true,
    })
}

fn ensure_within_supported_cap(width: u32, height: u32) -> anyhow::Result<()> {
    ensure!(
        width <= MAX_ATTACHMENT_IMAGE_WIDTH && height <= MAX_ATTACHMENT_IMAGE_HEIGHT,
        "image exceeded the supported size: {width}x{height}",
    );
    Ok(())
}

fn encode_thumbnail_webp(rgba: &[u8], width: u32, height: u32) -> anyhow::Result<Vec<u8>> {
    webpx::Encoder::new_rgba(rgba, width, height)
        .quality(THUMBNAIL_QUALITY_PERCENT)
        .encode(webpx::Unstoppable)
        .context("WebP encode failed")
}

fn thumbnail_dimensions(width: u32, height: u32, max_edge: u32) -> Option<(u32, u32)> {
    let long_edge = width.max(height);
    if long_edge <= max_edge {
        return None;
    }
    let scale = f64::from(max_edge) / f64::from(long_edge);
    let edge = |e: u32| ((f64::from(e) * scale).round() as u32).max(1);
    Some((edge(width), edge(height)))
}

#[cfg(test)]
mod test {
    use super::*;

    fn encode_static_webp(width: u32, height: u32) -> Vec<u8> {
        let rgba = vec![127u8; (width * height * 4) as usize];
        webpx::Encoder::new_rgba(&rgba, width, height)
            .quality(80.0)
            .encode(webpx::Unstoppable)
            .unwrap()
    }

    fn encode_animated_webp(width: u32, height: u32, frames: u32) -> Vec<u8> {
        let mut encoder = webpx::AnimationEncoder::with_options(width, height, true, 0).unwrap();
        encoder.set_quality(80.0);
        let mut timestamp_ms = 0;
        for frame in 0..frames {
            // Frames must differ, otherwise they are merged and a single-frame
            // animation is assembled as a still image.
            let rgba = vec![(frame * 60) as u8; (width * height * 4) as usize];
            encoder.add_frame_rgba(&rgba, timestamp_ms).unwrap();
            timestamp_ms += 100;
        }
        encoder.finish(timestamp_ms).unwrap()
    }

    fn decoded_info(bytes: &[u8]) -> (u32, u32, bool) {
        let decoder = webpx::Decoder::new(bytes).unwrap();
        let info = decoder.info();
        (info.width, info.height, info.has_animation)
    }

    #[test]
    fn thumbnail_dimensions_scales_long_edge() {
        assert_eq!(thumbnail_dimensions(1024, 1024, 1024), None);
        assert_eq!(thumbnail_dimensions(500, 300, 1024), None);
        assert_eq!(thumbnail_dimensions(2048, 1024, 1024), Some((1024, 512)));
        assert_eq!(thumbnail_dimensions(1000, 4000, 1024), Some((256, 1024)));
        // The short edge never rounds down to zero
        assert_eq!(thumbnail_dimensions(10000, 1, 1024), Some((1024, 1)));
    }

    #[test]
    fn thumbnail_of_large_still_image_is_downscaled() {
        let original = encode_static_webp(2048, 1024);
        let ThumbnailImage::Encoded { bytes, is_animated } = encode_thumbnail(&original).unwrap()
        else {
            panic!("expected an encoded thumbnail");
        };
        assert!(!is_animated);
        assert_eq!(decoded_info(&bytes), (1024, 512, false));
    }

    #[test]
    fn thumbnail_of_small_still_image_is_the_original() {
        let original = encode_static_webp(800, 600);
        assert!(matches!(
            encode_thumbnail(&original).unwrap(),
            ThumbnailImage::OriginalFits
        ));
    }

    #[test]
    fn thumbnail_of_animated_image_is_a_static_frame() {
        // Small animated images are re-encoded and never serve the original.
        let original = encode_animated_webp(200, 100, 3);
        let ThumbnailImage::Encoded { bytes, is_animated } = encode_thumbnail(&original).unwrap()
        else {
            panic!("expected an encoded thumbnail");
        };
        assert!(is_animated);
        assert_eq!(decoded_info(&bytes), (200, 100, false));
    }

    #[test]
    fn thumbnail_of_large_animated_image_is_downscaled() {
        let original = encode_animated_webp(2048, 512, 2);
        let ThumbnailImage::Encoded { bytes, is_animated } = encode_thumbnail(&original).unwrap()
        else {
            panic!("expected an encoded thumbnail");
        };
        assert!(is_animated);
        assert_eq!(decoded_info(&bytes), (1024, 256, false));
    }

    #[test]
    fn thumbnail_of_undecodable_bytes_fails() {
        assert!(encode_thumbnail(b"not a webp").is_err());
    }

    #[test]
    fn thumbnail_of_oversized_image_fails() {
        let original = encode_static_webp(MAX_ATTACHMENT_IMAGE_WIDTH + 1, 8);
        assert!(encode_thumbnail(&original).is_err());
    }
}
