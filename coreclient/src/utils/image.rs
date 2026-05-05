// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{self, File},
    io::{BufReader, Cursor},
    path::Path,
};

use image::{
    AnimationDecoder, Delay, DynamicImage, GenericImageView, ImageBuffer, ImageDecoder,
    ImageFormat, ImageReader, Rgba,
    codecs::{gif::GifDecoder, png::PngDecoder, webp::WebPDecoder},
    guess_format,
};
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
/// Floor for per-frame durations. Some animated images declare a 0 ms delay
/// expecting the renderer to clamp it, so we ensure each frame contributes a
/// non-zero duration to the resulting WebP timeline.
const MIN_FRAME_DURATION_MS: i32 = 20;

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
/// - Converts the image to WebP. Animated GIFs, animated WebPs, and APNGs are
///   re-encoded as animated WebP, preserving per-frame timing.
pub(crate) fn load_attachment_image(
    path: &Path,
) -> anyhow::Result<Option<ReencodedAttachmentImage>> {
    let file_size = fs::metadata(path)?.len();

    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let Some(format) = reader.format() else {
        return Ok(None);
    };

    let result = match format {
        ImageFormat::Gif => {
            let decoder = GifDecoder::new(open_buffered(path)?)?;
            load_animated_frames(decoder, file_size, format)?
        }
        ImageFormat::WebP => {
            let decoder = WebPDecoder::new(open_buffered(path)?)?;
            if decoder.has_animation() {
                load_animated_frames(decoder, file_size, format)?
            } else {
                load_still_image(decoder, file_size)?
            }
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::new(open_buffered(path)?)?;
            if decoder.is_apng()? {
                let apng = decoder.apng()?;
                load_animated_frames(apng, file_size, format)?
            } else {
                load_still_image(decoder, file_size)?
            }
        }
        _ => {
            let decoder = reader.into_decoder()?;
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

fn open_buffered(path: &Path) -> anyhow::Result<BufReader<File>> {
    Ok(BufReader::new(File::open(path)?))
}

/// Decodes a still image and re-encodes it as a single-frame WebP.
fn load_still_image<D: ImageDecoder>(
    mut decoder: D,
    file_size: u64,
) -> anyhow::Result<ReencodedAttachmentImage> {
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

    let image_rgba = image.to_rgba8();
    let (width, height) = image_rgba.dimensions();

    let mut encoder = webp_encoder(width, height)?;
    encoder
        .add_frame(&image_rgba, 0)
        .map_err(|err| anyhow::anyhow!("WebP add_frame failed: {err:?}"))?;
    let webp_data = encoder
        .finalize(MIN_FRAME_DURATION_MS)
        .map_err(|err| anyhow::anyhow!("WebP finalize failed: {err:?}"))?;

    // `blurhash::encode` can only fail if the components dimension is out of range
    // => We should never get an error here.
    let blurhash = blurhash::encode(4, 3, width, height, &image_rgba)?;

    info!(
        from_bytes = file_size,
        to_bytes = webp_data.len(),
        "Reencoded attachment image as WebP",
    );

    Ok(ReencodedAttachmentImage {
        webp_image: webp_data.to_vec(),
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

    let blurhash = blurhash::encode(4, 3, width, height, first_buffer.as_raw())?;

    let mut encoder = webp_encoder(width, height)?;

    let mut timestamp_ms: i32 = 0;
    encoder
        .add_frame(first_buffer.as_raw(), timestamp_ms)
        .map_err(|err| anyhow::anyhow!("WebP add_frame failed: {err:?}"))?;
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
            .add_frame(resized.as_raw(), timestamp_ms)
            .map_err(|err| anyhow::anyhow!("WebP add_frame failed: {err:?}"))?;
        timestamp_ms = timestamp_ms.saturating_add(delay_to_ms(frame_delay));
    }

    let webp_data = encoder
        .finalize(timestamp_ms)
        .map_err(|err| anyhow::anyhow!("WebP finalize failed: {err:?}"))?;

    info!(
        from_bytes = file_size,
        to_bytes = webp_data.len(),
        ?source,
        "Reencoded animated image as animated WebP",
    );

    Ok(ReencodedAttachmentImage {
        webp_image: webp_data.to_vec(),
        image_dimensions: (width, height),
        blurhash,
    })
}

/// Creates a new WebP encoder with the given dimensions and quality settings.
fn webp_encoder(width: u32, height: u32) -> anyhow::Result<webp_animation::Encoder> {
    webp_animation::Encoder::new_with_options(
        (width, height),
        webp_animation::EncoderOptions {
            minimize_size: true, // only for animated WebP
            allow_mixed: true,   // only for animated WebP
            encoding_config: Some(webp_animation::EncodingConfig {
                quality: ATTACHMENT_IMAGE_QUALITY_PERCENT,
                encoding_type: webp_animation::EncodingType::Lossy(
                    webp_animation::LossyEncodingConfig::default(),
                ),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .map_err(|err| anyhow::anyhow!("WebP encoder init failed: {err:?}"))
}

/// Converts a frame delay to milliseconds, applying a floor to avoid
/// zero-duration frames.
fn delay_to_ms(delay: Delay) -> i32 {
    let (n, d) = delay.numer_denom_ms();
    let ms = if d == 0 { 0 } else { n / d };
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
