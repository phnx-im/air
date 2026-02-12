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

pub async fn export_client_database(db_path: String, user_id: UiUserId) -> anyhow::Result<Vec<u8>> {
    aircoreclient::export_client_database(&db_path, &user_id.into()).await
}

pub async fn import_client_database(db_path: String, tar_gz_bytes: Vec<u8>) -> anyhow::Result<()> {
    aircoreclient::import_client_database(&db_path, &tar_gz_bytes).await
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
