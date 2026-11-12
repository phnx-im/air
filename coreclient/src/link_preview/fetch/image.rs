// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::io::Cursor;

use anyhow::Context;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits};
use tokio::task::spawn_blocking;
use url::Url;

use crate::utils::image::{ImageProcessingCancellation, compute_blurhash};

use super::{ContentKind, LinkPreviewFetcher};

/// Image types a card image may have, as media type and decoder format.
pub(super) const IMAGE_TYPES: [(&str, ImageFormat); 4] = [
    ("image/webp", ImageFormat::WebP),
    ("image/png", ImageFormat::Png),
    ("image/jpeg", ImageFormat::Jpeg),
    ("image/gif", ImageFormat::Gif),
];

impl LinkPreviewFetcher {
    /// Fetches and processes the card image at `url`.
    ///
    /// `None` if it is not an allowed image or over a cap.
    pub(super) async fn fetch_image(&self, url: Url) -> anyhow::Result<Option<PreviewImage>> {
        let accept = IMAGE_TYPES.map(|(ty, _)| ty).join(",");
        let Some(response) = self.get(url, &accept).await? else {
            return Ok(None);
        };
        let Some(ContentKind::Image) = ContentKind::of(&response) else {
            return Ok(None);
        };
        PreviewImage::read(response).await
    }
}

pub(crate) struct PreviewImage {
    pub(crate) webp: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) blurhash: String,
}

impl PreviewImage {
    const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024;
    /// Must stay below the server attachment limit.
    const MAX_WEBP_BYTES: usize = 512 * 1024;

    const MAX_SOURCE_WIDTH: u32 = 8_192;
    const MAX_SOURCE_HEIGHT: u32 = 8_192;

    const WEBP_MAX_EDGE: u32 = 1024;
    const WEBP_QUALITY_PERCENT: f32 = 80.0;

    const MAX_ALLOC_BYTES: u64 = 64 * 1024 * 1024;

    pub(super) async fn read(mut response: reqwest::Response) -> anyhow::Result<Option<Self>> {
        /// Longest magic among IMAGE_TYPES (WebP)
        const MAGIC_LEN: usize = 12;

        let content_length = match response.content_length() {
            Some(len) if (PreviewImage::MAX_SOURCE_BYTES as u64) < len => return Ok(None),
            Some(len) => usize::try_from(len).unwrap_or(0),
            None => 0,
        };

        let mut bytes = Vec::with_capacity(content_length);
        let mut format = None;
        while let Some(chunk) = response.chunk().await? {
            bytes.extend(chunk);
            if Self::MAX_SOURCE_BYTES < bytes.len() {
                return Ok(None);
            }
            if format.is_none() && MAGIC_LEN <= bytes.len() {
                let Some(allowed) = allowed_format(&bytes) else {
                    return Ok(None);
                };
                format = Some(allowed);
            }
        }
        let Some(format) = format else {
            return Ok(None);
        };

        let cancel = ImageProcessingCancellation::new();
        // Dropping this future (cancel, timeout) stops the blocking work.
        let _cancel_on_drop = cancel.cancel_on_drop();
        spawn_blocking(move || Self::process(&bytes, format, &cancel)).await?
    }

    fn process(
        bytes: &[u8],
        format: ImageFormat,
        cancel: &ImageProcessingCancellation,
    ) -> anyhow::Result<Option<PreviewImage>> {
        let mut limits = Limits::default();
        limits.max_image_width = Some(Self::MAX_SOURCE_WIDTH);
        limits.max_image_height = Some(Self::MAX_SOURCE_HEIGHT);
        limits.max_alloc = Some(Self::MAX_ALLOC_BYTES);
        let image = Self::reencode(bytes, format, limits, cancel)?;
        if Self::MAX_WEBP_BYTES < image.webp.len() {
            return Ok(None);
        }
        Ok(Some(image))
    }

    /// Re-encodes an untrusted web image as a static WebP preview.
    fn reencode(
        bytes: &[u8],
        format: ImageFormat,
        limits: Limits,
        cancel: &ImageProcessingCancellation,
    ) -> anyhow::Result<PreviewImage> {
        let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
        reader.limits(limits);
        let mut decoder = reader.into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut image = DynamicImage::from_decoder(decoder)?;

        cancel.ensure_running()?;
        image.apply_orientation(orientation);

        // Fast area average, only when the image has to shrink
        let (width, height) = image.dimensions();
        let image = if Self::WEBP_MAX_EDGE < width.max(height) {
            image.thumbnail(Self::WEBP_MAX_EDGE, Self::WEBP_MAX_EDGE)
        } else {
            image
        };

        let buffer = image.into_rgba8();
        cancel.ensure_running()?;
        let (width, height) = buffer.dimensions();

        let webp = webpx::Encoder::new_rgba(buffer.as_raw(), width, height)
            .quality(Self::WEBP_QUALITY_PERCENT)
            .encode(cancel.clone())
            .context("WebP encode failed")?;

        Ok(PreviewImage {
            webp,
            width,
            height,
            blurhash: compute_blurhash(&DynamicImage::ImageRgba8(buffer))?,
        })
    }
}

fn allowed_format(bytes: &[u8]) -> Option<ImageFormat> {
    let format = image::guess_format(bytes).ok();
    format.filter(|format| IMAGE_TYPES.iter().any(|(_, f)| f == format))
}
