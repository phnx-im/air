// SPDX-FileCopyrightText: 2024 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Misc. functions

use super::types::UiUserId;

pub async fn delete_databases(db_path: String) -> anyhow::Result<()> {
    aircoreclient::delete_databases(&db_path).await
}

pub async fn delete_client_database(db_path: String, user_id: UiUserId) -> anyhow::Result<()> {
    aircoreclient::delete_client_database(&db_path, &user_id.into()).await
}

/// Returns whether the file at the given path is a recognized image format.
/// Uses the same detection as `load_attachment_image()`.
pub fn is_image_file(path: String) -> bool {
    image::ImageReader::open(&path)
        .ok()
        .and_then(|r| r.with_guessed_format().ok())
        .and_then(|r| r.format())
        .is_some()
}

/// Reads file paths from the system clipboard. Only supported on desktop
/// platforms (Linux, Windows, macOS).
///
/// Returns `None` if the clipboard does not contain file paths, or when called
/// on unsupported platforms.
pub fn read_clipboard_file_paths() -> Option<Vec<String>> {
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    {
        let mut clipboard = arboard::Clipboard::new().ok()?;
        let paths = clipboard.get().file_list().ok()?;
        if paths.is_empty() {
            return None;
        }
        Some(
            paths
                .into_iter()
                .filter_map(|p| p.to_str().map(String::from))
                .collect(),
        )
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

/// Reads an image from the system clipboard and returns it as JPEG bytes. Only
/// supported on desktop platforms (Linux, Windows, macOS).
///
/// Returns `None` if the clipboard does not contain image data, or when called
/// on unsupported platforms.
pub fn read_clipboard_image() -> Option<Vec<u8>> {
    // arboard is only supported on desktop platforms
    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    {
        use image::codecs::jpeg::JpegEncoder;
        use std::io::Cursor;

        let mut clipboard = arboard::Clipboard::new().ok()?;
        let img_data = clipboard.get_image().ok()?;

        let rgba = image::RgbaImage::from_raw(
            img_data.width as u32,
            img_data.height as u32,
            img_data.bytes.into_owned(),
        )?;

        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);
        let mut encoder = JpegEncoder::new_with_quality(&mut cursor, 99);
        encoder.encode_image(&rgba).ok()?;

        Some(buf)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        None
    }
}
