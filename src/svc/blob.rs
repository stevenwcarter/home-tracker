//! A stored original as the thumbnailer sees it: content hash and type only.

use crate::models::Attachment;

/// An original addressed by its content hash. Attachments are one kind of
/// blob; anything else stored under `originals/` can be one too, so the
/// thumbnail service never needs to know which row owns the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub sha256: String,
    pub mime_type: String,
}

impl From<&Attachment> for Blob {
    fn from(att: &Attachment) -> Self {
        Self {
            sha256: att.sha256.clone(),
            mime_type: att.mime_type.clone(),
        }
    }
}
