//! Thumbnail policy: which MIME types can be thumbnailed and which sizes exist.

use std::io::Cursor;

use anyhow::{Context, Result};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

/// Quality passed to the WebP encoder for generated thumbnails (0-100).
pub const WEBP_QUALITY: f32 = 80.0;

/// A generated thumbnail: WebP-encoded bytes plus the pixel dimensions they
/// were encoded at (the box-fitted size, not the caller's requested size).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generated {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// The largest original edge, in pixels, that will be decoded (8192² ≈ 64 MP).
const MAX_DECODE_EDGE: u32 = 8192;

/// The most the decoder may allocate for one original.
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

/// Decoder bounds. A small file can declare enormous dimensions, so the
/// header is checked against these before any pixel buffer is allocated.
fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_EDGE);
    limits.max_image_height = Some(MAX_DECODE_EDGE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

/// Decodes `original`, applies its EXIF orientation, fits it inside a
/// `size × size` box without upscaling, and encodes WebP at [`WEBP_QUALITY`].
/// Pure and blocking: call it from `spawn_blocking`.
pub fn generate_bytes(original: &[u8], size: u32) -> Result<Generated> {
    let mut reader = ImageReader::new(Cursor::new(original))
        .with_guessed_format()
        .context("sniffing the image format")?;
    reader.limits(decode_limits());
    let mut decoder = reader.into_decoder().context("opening the image")?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).context("decoding the image")?;
    img.apply_orientation(orientation);
    if img.width() > size || img.height() > size {
        img = img.resize(size, size, image::imageops::FilterType::Lanczos3);
    }
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    let encoded = webp::Encoder::from_rgba(rgba.as_raw(), width, height).encode(WEBP_QUALITY);
    Ok(Generated {
        data: encoded.to_vec(),
        width,
        height,
    })
}

/// Allowed thumbnail box sizes, ascending. A request rounds up to the first
/// size that is at least as large; larger requests clamp to the last.
pub const THUMB_SIZES: [u32; 3] = [300, 500, 1200];

/// Whether `mime` is a type this service can generate a thumbnail for.
pub fn is_thumbnailable(mime: &str) -> bool {
    matches!(
        mime.trim().to_ascii_lowercase().as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    )
}

/// The [`THUMB_SIZES`] entry to serve for a requested size, or `None` when
/// `requested` is not positive.
pub fn allowed_size(requested: i64) -> Option<u32> {
    if requested <= 0 {
        return None;
    }
    let wanted = u32::try_from(requested).unwrap_or(u32::MAX);
    Some(
        THUMB_SIZES
            .iter()
            .copied()
            .find(|s| *s >= wanted)
            .unwrap_or(THUMB_SIZES[THUMB_SIZES.len() - 1]),
    )
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageEncoder, ImageError, RgbImage};
    use std::io::Cursor;

    use super::*;

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_fn(w, h, |x, _| image::Rgb([(x % 256) as u8, 40, 200]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 90)
            .write_image(&img, w, h, image::ExtendedColorType::Rgb8)
            .unwrap();
        out
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbImage::from_pixel(w, h, image::Rgb([10, 200, 30]));
        let mut out = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// Inserts an APP1 Exif segment carrying only Orientation = `orientation`
    /// right after the SOI marker. Layout: FF E1, length, "Exif\0\0", TIFF header
    /// (little endian, IFD at offset 8), one IFD entry (tag 0x0112, SHORT, count 1).
    fn with_exif_orientation(jpeg: &[u8], orientation: u16) -> Vec<u8> {
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let mut tiff = vec![b'I', b'I', 0x2A, 0x00, 8, 0, 0, 0];
        tiff.extend_from_slice(&1u16.to_le_bytes()); // one entry
        tiff.extend_from_slice(&0x0112u16.to_le_bytes()); // Orientation
        tiff.extend_from_slice(&3u16.to_le_bytes()); // SHORT
        tiff.extend_from_slice(&1u32.to_le_bytes()); // count
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0]); // value padding to 4 bytes
        tiff.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        let mut payload = b"Exif\0\0".to_vec();
        payload.extend_from_slice(&tiff);
        let len = (payload.len() + 2) as u16;
        let mut out = jpeg[..2].to_vec();
        out.extend_from_slice(&[0xFF, 0xE1]);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&payload);
        out.extend_from_slice(&jpeg[2..]);
        out
    }

    fn decode(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).unwrap()
    }

    #[test]
    fn fits_inside_the_box_preserving_aspect_ratio_and_encodes_webp() {
        let g = generate_bytes(&jpeg(1600, 1200), 500).unwrap();
        assert_eq!((g.width, g.height), (500, 375));
        assert_eq!(&g.data[..4], b"RIFF");
        assert_eq!(&g.data[8..12], b"WEBP");
        let decoded = decode(&g.data);
        assert_eq!((decoded.width(), decoded.height()), (500, 375));
    }

    #[test]
    fn portrait_images_are_bounded_by_height() {
        let g = generate_bytes(&jpeg(600, 1800), 300).unwrap();
        assert_eq!((g.width, g.height), (100, 300));
    }

    #[test]
    fn never_upscales() {
        let g = generate_bytes(&png(120, 80), 1200).unwrap();
        assert_eq!((g.width, g.height), (120, 80));
    }

    #[test]
    fn applies_exif_orientation() {
        // Orientation 6 = rotate 90° clockwise: a 400×200 source becomes 200×400.
        let rotated = with_exif_orientation(&jpeg(400, 200), 6);
        let g = generate_bytes(&rotated, 1200).unwrap();
        assert_eq!((g.width, g.height), (200, 400));
        // And without the tag the same bytes stay landscape.
        let plain = generate_bytes(&jpeg(400, 200), 1200).unwrap();
        assert_eq!((plain.width, plain.height), (400, 200));
    }

    /// The CRC-32 (IEEE) of `bytes`, bit by bit: enough to frame a PNG chunk.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    /// A well-framed PNG whose IHDR claims `w`×`h` 8-bit greyscale pixels but
    /// which carries no pixel data.
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&w.to_be_bytes());
        ihdr.extend_from_slice(&h.to_be_bytes());
        // Bit depth 8, greyscale, deflate, adaptive filtering, no interlace.
        ihdr.extend_from_slice(&[8, 0, 0, 0, 0]);
        png_chunk(&mut out, b"IHDR", &ihdr);
        png_chunk(&mut out, b"IDAT", &[]);
        png_chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn oversized_dimensions_are_rejected_by_limits() {
        // 20000×20000 greyscale is 400 MB: under the crate's 512 MiB default
        // allocation cap, so only the explicit dimension limit stops it before
        // the decoder allocates the buffer.
        let err = generate_bytes(&png_header(20_000, 20_000), 300).unwrap_err();
        assert!(
            err.chain()
                .any(|e| matches!(e.downcast_ref(), Some(ImageError::Limits(_)))),
            "expected a limits error, got {err:#}"
        );
    }

    #[test]
    fn rejects_garbage_and_non_images() {
        assert!(generate_bytes(b"not an image", 300).is_err());
        assert!(generate_bytes(b"%PDF-1.4 mini", 300).is_err());
    }

    #[test]
    fn rounds_up_and_clamps() {
        assert_eq!(allowed_size(1), Some(300));
        assert_eq!(allowed_size(300), Some(300));
        assert_eq!(allowed_size(301), Some(500));
        assert_eq!(allowed_size(500), Some(500));
        assert_eq!(allowed_size(501), Some(1200));
        assert_eq!(allowed_size(1200), Some(1200));
        assert_eq!(allowed_size(9999), Some(1200));
        assert_eq!(allowed_size(i64::MAX), Some(1200));
        assert_eq!(allowed_size(0), None);
        assert_eq!(allowed_size(-5), None);
    }

    #[test]
    fn mime_check_is_case_insensitive_and_closed() {
        // Review focus 3.
        for ok in [
            "image/jpeg",
            "image/JPEG",
            " Image/Png ",
            "image/gif",
            "image/webp",
        ] {
            assert!(is_thumbnailable(ok), "{ok}");
        }
        for no in [
            "image/heic",
            "image/svg+xml",
            "image/avif",
            "application/pdf",
            "",
        ] {
            assert!(!is_thumbnailable(no), "{no}");
        }
    }
}
