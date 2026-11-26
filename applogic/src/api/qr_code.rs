// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

//! QR codes in camera frames

use std::{collections::HashSet, panic, thread};

use anyhow::{Context, Result, ensure};
use rxing::{
    BarcodeFormat, BinaryBitmap, DecodeHints, Exceptions, Luma8Source, LuminanceSource,
    MultiFormatReader, Reader, common::HybridBinarizer, multi::MultipleBarcodeReader,
    qrcode::cpp_port::QrReader,
};

/// One camera frame, luma plane only.
///
/// Rows start `bytes_per_row` bytes apart. The last row may end right after
/// its `width` bytes.
pub struct LumaFrame {
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
    pub bytes: Vec<u8>,
}

/// Decodes the QR codes in `frame` and returns the first payload that `parse`
/// accepts.
///
/// A frame without such a code is `Ok(None)`. Fails if the dimensions of the
/// frame do not match its buffer.
pub(crate) fn decode_qr_code<T: Send>(
    frame: &LumaFrame,
    parse: impl Fn(&str) -> Option<T> + Sync,
) -> Result<Option<T>> {
    let luma = packed_luma(frame)?;
    let source = Luma8Source::new_with_slice(&luma, frame.width, frame.height)?;
    // We invert the luminance and not the binarized bitmap. The binarizer
    // expects dark codes on a light background and loses blurred light codes
    // on a dark one.
    let mut inverted_source = source.clone();
    inverted_source.invert();
    let mut image = BinaryBitmap::new(HybridBinarizer::new(source));
    let mut inverted_image = BinaryBitmap::new(HybridBinarizer::new(inverted_source));
    // We try to decode both polarisations at once.
    let (payload, inverted_payload) = thread::scope(|scope| {
        let inverted = scope.spawn(|| decode_image(&mut inverted_image, &parse));
        let payload = decode_image(&mut image, &parse);
        let inverted_payload = inverted
            .join()
            .unwrap_or_else(|panic| panic::resume_unwind(panic));
        (payload, inverted_payload)
    });
    let payload = payload?;
    if payload.is_some() {
        return Ok(payload);
    }
    inverted_payload
}

/// Decodes the QR codes in `image` as is and returns the first payload that
/// `parse` accepts.
fn decode_image<T>(
    image: &mut BinaryBitmap<HybridBinarizer<Luma8Source<'_>>>,
    parse: &impl Fn(&str) -> Option<T>,
) -> Result<Option<T>> {
    let hints = DecodeHints {
        PossibleFormats: Some(HashSet::from([BarcodeFormat::QR_CODE])),
        TryHarder: Some(true),
        ..Default::default()
    };
    let decoded = MultiFormatReader::default().decode_with_hints(image, &hints);
    let first = match decoded {
        Ok(result) => result,
        Err(Exceptions::NotFoundException(_)) => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to decode a QR code in a {}x{} frame",
                    image.get_width(),
                    image.get_height()
                )
            });
        }
    };
    if let Some(payload) = parse(first.getText()) {
        return Ok(Some(payload));
    }
    // The reader stops at the first code it decodes, which may be another code
    // in view. We read all codes only as a fallback, since that is slower and
    // misses more of the hard frames.
    let results = QrReader.decode_multiple_with_hints(image, &hints)?;
    Ok(results.iter().find_map(|result| parse(result.getText())))
}

/// Returns the luma rows of `frame` without the row padding.
fn packed_luma(frame: &LumaFrame) -> Result<Vec<u8>> {
    let required_len = u64::from(frame.bytes_per_row) * u64::from(frame.height.saturating_sub(1))
        + u64::from(frame.width);
    ensure!(
        frame.width > 0
            && frame.height > 0
            && frame.bytes_per_row >= frame.width
            && frame.bytes.len() as u64 >= required_len,
        "invalid luma frame: width {}, height {}, bytes per row {}, length {}",
        frame.width,
        frame.height,
        frame.bytes_per_row,
        frame.bytes.len(),
    );

    let width = frame.width as usize;
    let height = frame.height as usize;
    let stride = frame.bytes_per_row as usize;
    let mut packed = Vec::with_capacity(width * height);
    for row in frame.bytes.chunks(stride).take(height) {
        packed.extend_from_slice(&row[..width]);
    }
    Ok(packed)
}

#[cfg(test)]
mod tests {
    use qrcode::{Color, QrCode};

    use super::*;

    /// A payload as long as a multi-device linking URL.
    const PAYLOAD: &str = "air://air.ms/multiDeviceLinkingCode?code=482915637204";

    /// Quiet zone around the code, in modules.
    const QUIET_ZONE: f32 = 2.5;

    /// Gray levels of the modules, of the card under the code and of the
    /// area around the card.
    #[derive(Debug, Clone, Copy)]
    struct Style {
        dark: u8,
        surface: u8,
        background: u8,
    }

    const LIGHT: Style = Style {
        dark: 20,
        surface: 240,
        background: 200,
    };

    /// The inverted code shown in dark mode.
    const DARK: Style = Style {
        dark: 235,
        surface: 30,
        background: 15,
    };

    /// A QR code as a camera sees it.
    #[derive(Debug, Clone)]
    struct Scene {
        width: usize,
        height: usize,
        bytes_per_row: usize,
        /// Position of the center of the code in the frame.
        center: (f32, f32),
        module_px: f32,
        style: Style,
    }

    impl Scene {
        /// A 640x480 frame with row padding and the code in its center.
        fn centered(module_px: f32, style: Style) -> Self {
            Self {
                width: 640,
                height: 480,
                bytes_per_row: 704,
                center: (320.0, 240.0),
                module_px,
                style,
            }
        }
    }

    /// Renders `content` as a QR code into a frame.
    fn render(content: &str, scene: &Scene) -> LumaFrame {
        let mut frame = LumaFrame {
            width: scene.width as u32,
            height: scene.height as u32,
            bytes_per_row: scene.bytes_per_row as u32,
            bytes: vec![scene.style.background; scene.bytes_per_row * scene.height],
        };
        draw(&mut frame, content, scene);
        frame
    }

    /// Draws `content` as a QR code on its card into `frame`.
    fn draw(frame: &mut LumaFrame, content: &str, scene: &Scene) {
        let code = QrCode::new(content).unwrap();
        let colors = code.to_colors();
        let modules = code.width() as f32;
        let (center_x, center_y) = scene.center;
        for y in 0..scene.height {
            for x in 0..scene.width {
                let u = (x as f32 - center_x) / scene.module_px + modules / 2.0;
                let v = (y as f32 - center_y) / scene.module_px + modules / 2.0;
                let in_code = (0.0..modules).contains(&u) && (0.0..modules).contains(&v);
                let on_card = (-QUIET_ZONE..modules + QUIET_ZONE).contains(&u)
                    && (-QUIET_ZONE..modules + QUIET_ZONE).contains(&v);
                let pixel = &mut frame.bytes[y * scene.bytes_per_row + x];
                if in_code {
                    *pixel = match colors[v as usize * code.width() + u as usize] {
                        Color::Dark => scene.style.dark,
                        Color::Light => scene.style.surface,
                    };
                } else if on_card {
                    *pixel = scene.style.surface;
                }
            }
        }
    }

    /// Decodes `frame` with a parser that accepts only [`PAYLOAD`].
    fn decode(frame: &LumaFrame) -> Option<String> {
        decode_qr_code(frame, |text| (text == PAYLOAD).then(|| text.to_owned())).unwrap()
    }

    /// Box-blurs `frame` over a square of `2 * radius + 1` pixels, like an
    /// out-of-focus camera.
    fn blur(frame: &LumaFrame, radius: usize) -> LumaFrame {
        let width = frame.width as usize;
        let height = frame.height as usize;
        let stride = frame.bytes_per_row as usize;
        let mut bytes = frame.bytes.clone();
        for y in 0..height {
            for x in 0..width {
                let rows = y.saturating_sub(radius)..=(y + radius).min(height - 1);
                let columns = x.saturating_sub(radius)..=(x + radius).min(width - 1);
                let mut sum = 0;
                let mut count = 0;
                for row in rows {
                    for column in columns.clone() {
                        sum += u32::from(frame.bytes[row * stride + column]);
                        count += 1;
                    }
                }
                bytes[y * stride + x] = (sum / count) as u8;
            }
        }
        LumaFrame { bytes, ..*frame }
    }

    #[test]
    fn reads_light_code() {
        let frame = render(PAYLOAD, &Scene::centered(6.0, LIGHT));
        assert_eq!(decode(&frame).as_deref(), Some(PAYLOAD));
    }

    #[test]
    fn reads_inverted_code() {
        let frame = render(PAYLOAD, &Scene::centered(6.0, DARK));
        assert_eq!(decode(&frame).as_deref(), Some(PAYLOAD));
    }

    #[test]
    fn reads_blurred_inverted_code() {
        let frame = blur(&render(PAYLOAD, &Scene::centered(4.0, DARK)), 2);
        assert_eq!(decode(&frame).as_deref(), Some(PAYLOAD));
    }

    #[test]
    fn reads_frame_without_row_padding() {
        let scene = Scene {
            bytes_per_row: 640,
            ..Scene::centered(6.0, LIGHT)
        };
        let frame = render(PAYLOAD, &scene);
        assert_eq!(decode(&frame).as_deref(), Some(PAYLOAD));
    }

    #[test]
    fn reads_frame_whose_last_row_has_no_padding() {
        let mut frame = render(PAYLOAD, &Scene::centered(6.0, LIGHT));
        frame.bytes.truncate(704 * 479 + 640);
        assert_eq!(decode(&frame).as_deref(), Some(PAYLOAD));
    }

    #[test]
    fn frame_without_code_is_none() {
        let frame = LumaFrame {
            width: 640,
            height: 480,
            bytes_per_row: 704,
            bytes: vec![128; 704 * 480],
        };
        assert_eq!(decode(&frame), None);
    }

    #[test]
    fn code_the_parser_rejects_is_none() {
        let frame = render("hello world", &Scene::centered(6.0, LIGHT));
        assert_eq!(decode(&frame), None);
    }

    #[test]
    fn reads_code_next_to_another_code() {
        let cases = [
            ("light", LIGHT, LIGHT),
            ("dark", DARK, DARK),
            ("dark next to light", LIGHT, DARK),
            ("light next to dark", DARK, LIGHT),
        ];
        for (name, other_style, style) in cases {
            let other = Scene {
                center: (160.0, 240.0),
                ..Scene::centered(4.0, other_style)
            };
            let mut frame = render("hello world", &other);
            let scene = Scene {
                center: (480.0, 240.0),
                ..Scene::centered(4.0, style)
            };
            draw(&mut frame, PAYLOAD, &scene);
            assert_eq!(decode(&frame).as_deref(), Some(PAYLOAD), "{name}");
        }
    }

    #[test]
    fn rejects_invalid_frames() {
        let frame = |width, height, bytes_per_row, len| LumaFrame {
            width,
            height,
            bytes_per_row,
            bytes: vec![0; len],
        };
        let cases = [
            ("stride below width", frame(640, 480, 639, 640 * 480)),
            ("zero width", frame(0, 480, 640, 640 * 480)),
            ("zero height", frame(640, 0, 640, 640 * 480)),
            ("short buffer", frame(640, 480, 704, 704 * 479 + 639)),
            ("huge dimensions", frame(u32::MAX, u32::MAX, u32::MAX, 16)),
        ];
        for (name, frame) in cases {
            let error = decode_qr_code(&frame, |_| Some(())).unwrap_err();
            let message = error.to_string();
            let expected = format!(
                "width {}, height {}, bytes per row {}, length {}",
                frame.width,
                frame.height,
                frame.bytes_per_row,
                frame.bytes.len(),
            );
            assert!(message.contains(&expected), "{name}: {message}");
        }
    }
}
