// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    fs::{self},
    io::Cursor,
    path::Path,
};

use anyhow::Context;
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
const BLURHASH_THUMBNAIL_SIZE: u32 = 32;
/// Floor for per-frame durations. Some animated images declare a 0 ms delay
/// expecting the renderer to clamp it, so we ensure each frame contributes a
/// non-zero duration to the resulting WebP timeline.
const MIN_FRAME_DURATION_MS: i32 = 20;

pub(crate) struct ReencodedAttachmentImage {
    pub(crate) webp_image: Vec<u8>,
    pub(crate) image_dimensions: (u32, u32),
    pub(crate) blurhash: String,
}

/// How much memory the re-encode of an attachment image may use.
///
/// The iOS share extension runs within a budget of roughly 120 MB shared
/// with the Flutter engine, which the re-encode of a 12 MP photo only fits
/// when libwebp trades speed for memory. The main app has no such limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageMemoryBudget {
    #[default]
    Unconstrained,
    Constrained,
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
    memory_budget: ImageMemoryBudget,
) -> anyhow::Result<Option<ReencodedAttachmentImage>> {
    let file_size = fs::metadata(path)?.len();

    let reader = ImageReader::open(path)?.with_guessed_format()?;
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
                load_still_image(decoder, file_size, memory_budget)?
            }
        }
        ImageFormat::Png => {
            let decoder = PngDecoder::new(reader.into_inner())?;
            if decoder.is_apng()? {
                let apng = decoder.apng()?;
                load_animated_frames(apng, file_size, format)?
            } else {
                load_still_image(decoder, file_size, memory_budget)?
            }
        }
        _ => {
            let decoder = match reader.into_decoder() {
                Ok(decoder) => decoder,
                // format support not compiled in, upload as a regular file
                Err(image::ImageError::Unsupported(_)) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            load_still_image(decoder, file_size, memory_budget)?
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

/// Decodes a still image and re-encodes it as a static WebP.
///
/// Peak memory matters here (see [`ImageMemoryBudget`]): nothing may hold a
/// second full-size copy of the decoded image. The pixels go to libwebp as
/// YUV planes and the RGB buffer is dropped before encoding, since libwebp's
/// RGB import would keep an ARGB copy alive for the whole encode.
fn load_still_image<D: ImageDecoder>(
    mut decoder: D,
    file_size: u64,
    memory_budget: ImageMemoryBudget,
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
    let (width, height) = image.dimensions();

    let blurhash = compute_blurhash(&image)?;

    // `low_memory` makes libwebp emit the bitstream as it goes instead of
    // buffering the tokens of the whole image, at the cost of encoding speed.
    let config = webpx::EncoderConfig::default()
        .quality(ATTACHMENT_IMAGE_QUALITY_PERCENT)
        .low_memory(memory_budget == ImageMemoryBudget::Constrained);

    let webp_data = if image.color().has_alpha() {
        let image_rgba = image.into_rgba8();
        webpx::Encoder::new_rgba(&image_rgba, width, height)
            .config(config)
            .encode(webpx::Unstoppable)
    } else {
        let planes = {
            let image_rgb = image.into_rgb8();
            YuvPlanes::from_rgb(&image_rgb)?
            // the RGB buffer is dropped here, before libwebp allocates
        };
        webpx::Encoder::new_yuv(planes.as_ref())
            .config(config)
            .encode(webpx::Unstoppable)
    }
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

    let blurhash = blurhash::encode(4, 3, width, height, first_buffer.as_raw())?;

    let mut encoder = webpx::AnimationEncoder::with_options(width, height, true, 0)
        .context("WebP encoder init failed")?;
    encoder.set_quality(ATTACHMENT_IMAGE_QUALITY_PERCENT);

    let mut timestamp_ms: i32 = 0;
    encoder
        .add_frame_rgba(first_buffer.as_raw(), timestamp_ms)
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

/// Computes the blurhash of an image from a small thumbnail of it.
fn compute_blurhash(image: &DynamicImage) -> anyhow::Result<String> {
    let thumbnail = image
        .thumbnail(BLURHASH_THUMBNAIL_SIZE, BLURHASH_THUMBNAIL_SIZE)
        .into_rgba8();
    // `blurhash::encode` can only fail if the components dimension is out of range
    // => We should never get an error here.
    Ok(blurhash::encode(
        4,
        3,
        thumbnail.width(),
        thumbnail.height(),
        &thumbnail,
    )?)
}

/// An image as planar YUV 4:2:0, the layout libwebp encodes from.
struct YuvPlanes {
    planes: yuv::YuvPlanarImageMut<'static, u8>,
}

impl YuvPlanes {
    /// Converts an RGB image to YUV 4:2:0.
    ///
    /// BT.601 limited range is what libwebp's own RGB import produces, so the
    /// result matches what libwebp would have computed from the RGB pixels.
    fn from_rgb(image: &image::RgbImage) -> anyhow::Result<Self> {
        let (width, height) = image.dimensions();
        let mut planes =
            yuv::YuvPlanarImageMut::alloc(width, height, yuv::YuvChromaSubsampling::Yuv420);
        yuv::rgb_to_yuv420(
            &mut planes,
            image.as_raw(),
            width * 3,
            yuv::YuvRange::Limited,
            yuv::YuvStandardMatrix::Bt601,
            yuv::YuvConversionMode::Balanced,
        )
        .context("RGB to YUV conversion failed")?;
        Ok(Self { planes })
    }

    fn as_ref(&self) -> webpx::YuvPlanesRef<'_> {
        webpx::YuvPlanesRef {
            y: self.planes.y_plane.borrow(),
            y_stride: self.planes.y_stride as usize,
            u: self.planes.u_plane.borrow(),
            u_stride: self.planes.u_stride as usize,
            v: self.planes.v_plane.borrow(),
            v_stride: self.planes.v_stride as usize,
            a: None,
            a_stride: 0,
            width: self.planes.width,
            height: self.planes.height,
        }
    }
}
