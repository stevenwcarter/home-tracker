//! Thumbnail policy: which MIME types can be thumbnailed and which sizes exist.

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
    use super::*;

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
