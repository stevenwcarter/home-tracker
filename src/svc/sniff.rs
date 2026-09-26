//! Image format detection from magic bytes. An upload's type is decided here,
//! never by the client's content type or filename, which are only advisory.

/// How many leading bytes [`sniff`] needs to tell every format apart.
pub const SNIFF_LEN: usize = 12;

const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const GIF_MAGICS: [&[u8]; 2] = [b"GIF87a", b"GIF89a"];

/// The image formats an upload may be. Each is one the thumbnailer decodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Gif,
    WebP,
}

impl ImageFormat {
    /// The MIME type stored for an attachment of this format.
    pub fn mime(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }

    /// The usual file extension, without the dot.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Gif => "gif",
            Self::WebP => "webp",
        }
    }
}

/// The format whose signature `head` (a file's first bytes) starts with, if
/// any: JPEG `FF D8 FF`, the 8-byte PNG signature, `GIF87a`/`GIF89a`, or a
/// RIFF container of type `WEBP`.
pub fn sniff(head: &[u8]) -> Option<ImageFormat> {
    if head.starts_with(JPEG_MAGIC) {
        Some(ImageFormat::Jpeg)
    } else if head.starts_with(PNG_MAGIC) {
        Some(ImageFormat::Png)
    } else if GIF_MAGICS.iter().any(|magic| head.starts_with(magic)) {
        Some(ImageFormat::Gif)
    } else if head.starts_with(b"RIFF") && head.get(8..SNIFF_LEN) == Some(b"WEBP") {
        Some(ImageFormat::WebP)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_four_formats() {
        let cases: [(&[u8], ImageFormat); 5] = [
            (&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10], ImageFormat::Jpeg),
            (b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR", ImageFormat::Png),
            (b"GIF87a\x01\0", ImageFormat::Gif),
            (b"GIF89a\x01\0", ImageFormat::Gif),
            (b"RIFF\x24\0\0\0WEBPVP8 ", ImageFormat::WebP),
        ];
        for (head, format) in cases {
            assert_eq!(sniff(head), Some(format), "{head:?}");
        }
    }

    #[test]
    fn rejects_everything_else() {
        let cases: [&[u8]; 8] = [
            b"<!doctype html><html>",
            b"<svg xmlns=",
            b"%PDF-1.4 mini",
            b"",
            &[0xFF, 0xD8],
            b"\x89PNG\r\n",
            b"RIFF\x24\0\0\0WAVEfmt ",
            b"RIFF\x24\0\0\0WEB",
        ];
        for head in cases {
            assert_eq!(sniff(head), None, "{head:?}");
        }
    }

    #[test]
    fn mime_and_extension_agree_with_the_format() {
        let table = [
            (ImageFormat::Jpeg, "image/jpeg", "jpg"),
            (ImageFormat::Png, "image/png", "png"),
            (ImageFormat::Gif, "image/gif", "gif"),
            (ImageFormat::WebP, "image/webp", "webp"),
        ];
        for (format, mime, extension) in table {
            assert_eq!(format.mime(), mime);
            assert_eq!(format.extension(), extension);
        }
    }
}
