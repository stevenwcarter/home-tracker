//! Thumbnail policy: which MIME types can be thumbnailed and which sizes exist.

use std::io::Cursor;

use anyhow::{Context, Result};
use chrono::NaiveDateTime;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

use crate::models::Thumbnail;

/// The MIME type of every generated thumbnail.
pub const GENERATED_MIME: &str = "image/webp";

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

impl Generated {
    /// The thumbnail row storing this as the `size` thumbnail of the
    /// original whose content hash is `sha256`.
    pub fn into_row(
        self,
        sha256: String,
        size: ThumbSize,
        created_at: NaiveDateTime,
    ) -> Result<Thumbnail> {
        Ok(Thumbnail {
            sha256,
            size: i32::try_from(size.get()).context("thumbnail size out of range")?,
            mime_type: GENERATED_MIME.to_owned(),
            width: i32::try_from(self.width).context("thumbnail width out of range")?,
            height: i32::try_from(self.height).context("thumbnail height out of range")?,
            data: self.data,
            created_at,
        })
    }
}

/// The largest original edge, in pixels, that will be decoded (8192² ≈ 64 MP).
const MAX_DECODE_EDGE: u32 = 8192;

/// The most the decoder may allocate for one original. Uploads whose decoded
/// image would need more are refused.
pub(crate) const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

/// Decoder bounds. A small file can declare enormous dimensions, so the
/// header is checked against these before any pixel buffer is allocated.
/// Uploads are refused against the same bounds, so every stored photo can
/// be thumbnailed.
pub(crate) fn decode_limits() -> Limits {
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

/// One of the allowed thumbnail box sizes, in pixels. Only [`allowed_size`]
/// makes one, so a size that reaches the thumbnail service is always valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThumbSize(u32);

impl ThumbSize {
    /// The smallest size, generated with every upload.
    pub const SMALLEST: Self = THUMB_SIZES[0];

    /// The box edge in pixels.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Allowed thumbnail box sizes, ascending. A request rounds up to the first
/// size that is at least as large; larger requests clamp to the last.
const THUMB_SIZES: [ThumbSize; 3] = [ThumbSize(300), ThumbSize(500), ThumbSize(1200)];

/// Whether `mime` is a type this service can generate a thumbnail for.
pub fn is_thumbnailable(mime: &str) -> bool {
    matches!(
        mime.trim().to_ascii_lowercase().as_str(),
        "image/jpeg" | "image/png" | "image/gif" | "image/webp"
    )
}

/// The [`ThumbSize`] to serve for a requested size, or `None` when
/// `requested` is not positive.
pub fn allowed_size(requested: i64) -> Option<ThumbSize> {
    if requested <= 0 {
        return None;
    }
    let wanted = u32::try_from(requested).unwrap_or(u32::MAX);
    Some(
        THUMB_SIZES
            .into_iter()
            .find(|s| s.0 >= wanted)
            .unwrap_or(THUMB_SIZES[THUMB_SIZES.len() - 1]),
    )
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, ImageError};

    use super::*;
    use crate::svc::fixtures::{jpeg, png, png_header};

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
        let px = |requested| allowed_size(requested).map(ThumbSize::get);
        assert_eq!(px(1), Some(300));
        assert_eq!(px(300), Some(300));
        assert_eq!(px(301), Some(500));
        assert_eq!(px(500), Some(500));
        assert_eq!(px(501), Some(1200));
        assert_eq!(px(1200), Some(1200));
        assert_eq!(px(9999), Some(1200));
        assert_eq!(px(i64::MAX), Some(1200));
        assert_eq!(px(0), None);
        assert_eq!(px(-5), None);
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
