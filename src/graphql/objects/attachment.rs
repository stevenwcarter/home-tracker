use juniper::ID;

use super::saturating_int;
use crate::graphql::context::GraphQLContext;
use crate::kinds::AttachmentKind;
use crate::models::Attachment;
use crate::svc::thumbnail;

/// A file attached to an entity. The URLs are the HTTP contract served by the
/// attachment handlers; this layer only formats them.
#[juniper::graphql_object(context = GraphQLContext, name = "Attachment")]
impl Attachment {
    fn id(&self) -> ID {
        ID::new(&self.id)
    }
    fn kind(&self) -> AttachmentKind {
        self.kind
    }
    fn primary(&self) -> bool {
        self.is_primary
    }
    fn title(&self) -> &str {
        &self.title
    }
    fn mime_type(&self) -> &str {
        &self.mime_type
    }
    fn size_bytes(&self) -> i32 {
        saturating_int(self.size_bytes)
    }
    /// Where the original file is served. The `?v=` tag changes whenever the
    /// bytes do, so clients can cache the URL forever instead of the id.
    fn url(&self) -> String {
        format!("/attachments/{}?v={}", self.id, version_tag(&self.sha256))
    }
    /// A thumbnail of at most `size` pixels; null for non-images. The size is
    /// echoed as requested: the HTTP handler rounds it to an allowed size.
    fn thumbnail_url(&self, #[graphql(default = 500)] size: i32) -> Option<String> {
        thumbnail::is_thumbnailable(&self.mime_type).then(|| {
            format!(
                "/attachments/{}/thumb/{size}?v={}",
                self.id,
                version_tag(&self.sha256)
            )
        })
    }
}

/// A short, stable cache-busting tag for `sha256`: its first 12 hex characters
/// (the whole string if shorter), which is already plenty of entropy per
/// attachment id to change whenever the underlying bytes do.
fn version_tag(sha256: &str) -> &str {
    &sha256[..sha256.len().min(12)]
}
