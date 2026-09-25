use juniper::ID;

use super::saturating_int;
use crate::graphql::context::GraphQLContext;
use crate::kinds::AttachmentKind;
use crate::models::Attachment;

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
    /// Where the original file is served.
    fn url(&self) -> String {
        format!("/attachments/{}", self.id)
    }
    /// A thumbnail of at most `size` pixels; null for non-images. The size is
    /// echoed as requested: the HTTP handler rounds it to an allowed size.
    fn thumbnail_url(&self, #[graphql(default = 500)] size: i32) -> Option<String> {
        self.mime_type
            .starts_with("image/")
            .then(|| format!("/attachments/{}/thumb/{size}", self.id))
    }
}
