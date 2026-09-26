//! Closed string enums stored as lowercase TEXT.

use std::fmt;
use std::str::FromStr;

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::Text;
use diesel::sqlite::{Sqlite, SqliteValue};
use juniper::GraphQLEnum;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} {value:?}")]
pub struct UnknownKind {
    kind: &'static str,
    value: String,
}

macro_rules! text_enum {
    ($name:ident, $label:literal, { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, AsExpression, FromSqlRow, GraphQLEnum, Serialize,
        )]
        #[diesel(sql_type = Text)]
        // JSON spells a variant as GraphQL does, so one client type fits both.
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = UnknownKind;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.trim().to_ascii_lowercase().as_str() {
                    $($text => Ok(Self::$variant),)+
                    _ => Err(UnknownKind { kind: $label, value: s.to_owned() }),
                }
            }
        }

        impl FromSql<Text, Sqlite> for $name {
            fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
                let raw = <String as FromSql<Text, Sqlite>>::from_sql(value)?;
                Ok(raw.parse()?)
            }
        }

        impl ToSql<Text, Sqlite> for $name {
            fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
                out.set_value(self.as_str());
                Ok(IsNull::No)
            }
        }
    };
}

text_enum!(AttachmentKind, "attachment kind", {
    Photo => "photo",
    Manual => "manual",
    Warranty => "warranty",
    Attachment => "attachment",
    Receipt => "receipt",
});

text_enum!(FieldKind, "field kind", {
    Text => "text",
    Number => "number",
    Boolean => "boolean",
    Time => "time",
});

text_enum!(IngestBatchStatus, "ingest batch status", {
    Collecting => "collecting",
    Processing => "processing",
    Reviewing => "reviewing",
    Done => "done",
});

text_enum!(IngestItemStatus, "ingest item status", {
    Collecting => "collecting",
    Queued => "queued",
    Analysing => "analysing",
    Ready => "ready",
    Failed => "failed",
    Accepted => "accepted",
    Skipped => "skipped",
});

impl IngestItemStatus {
    /// Whether the item waits for the user to accept or skip it.
    pub fn is_reviewable(self) -> bool {
        matches!(self, Self::Ready | Self::Failed)
    }

    /// Whether the item is finished with: accepted or skipped.
    pub fn is_closed(self) -> bool {
        matches!(self, Self::Accepted | Self::Skipped)
    }
}

text_enum!(IngestPhotoStatus, "ingest photo status", {
    Pending => "pending",
    Described => "described",
    Failed => "failed",
});

text_enum!(SuggestedKind, "suggested kind", {
    Photo => "photo",
    Receipt => "receipt",
    Warranty => "warranty",
    Manual => "manual",
    Other => "other",
});

impl From<SuggestedKind> for AttachmentKind {
    /// The attachment kind a photo classified as `kind` is stored as;
    /// anything the model could not place becomes a plain attachment.
    fn from(kind: SuggestedKind) -> Self {
        match kind {
            SuggestedKind::Photo => Self::Photo,
            SuggestedKind::Receipt => Self::Receipt,
            SuggestedKind::Warranty => Self::Warranty,
            SuggestedKind::Manual => Self::Manual,
            SuggestedKind::Other => Self::Attachment,
        }
    }
}

#[cfg(test)]
mod tests {
    use juniper::ToInputValue;

    use super::*;

    /// Asserts `check` for every variant of every text enum.
    macro_rules! for_every_variant {
        ($check:ident) => {
            $check(AttachmentKind::ALL);
            $check(FieldKind::ALL);
            $check(IngestBatchStatus::ALL);
            $check(IngestItemStatus::ALL);
            $check(IngestPhotoStatus::ALL);
            $check(SuggestedKind::ALL);
        };
    }

    #[test]
    fn round_trips_every_variant_through_text() {
        fn check<K: FromStr + PartialEq + fmt::Debug + fmt::Display>(all: &[K])
        where
            K::Err: fmt::Debug,
        {
            for kind in all {
                assert_eq!(&kind.to_string().parse::<K>().unwrap(), kind);
            }
        }
        for_every_variant!(check);
    }

    #[test]
    fn parsing_is_case_insensitive_and_rejects_unknowns() {
        assert_eq!(
            "PHOTO".parse::<AttachmentKind>().unwrap(),
            AttachmentKind::Photo
        );
        assert_eq!(" Time ".parse::<FieldKind>().unwrap(), FieldKind::Time);
        assert!("thumbnail".parse::<AttachmentKind>().is_err());
        assert!("json".parse::<FieldKind>().is_err());
    }

    #[test]
    fn json_spells_every_variant_as_graphql_does() {
        fn check<K: Serialize + ToInputValue>(all: &[K]) {
            for kind in all {
                let json = serde_json::to_value(kind).unwrap();
                let graphql = serde_json::to_value(kind.to_input_value()).unwrap();
                assert_eq!(json, graphql);
            }
        }
        for_every_variant!(check);
    }

    #[test]
    fn other_is_stored_as_a_plain_attachment() {
        let stored: Vec<AttachmentKind> = SuggestedKind::ALL.iter().map(|&k| k.into()).collect();
        assert_eq!(
            stored,
            [
                AttachmentKind::Photo,
                AttachmentKind::Receipt,
                AttachmentKind::Warranty,
                AttachmentKind::Manual,
                AttachmentKind::Attachment,
            ]
        );
    }

    #[test]
    fn kinds_round_trip_through_sqlite() {
        use crate::db::{ITEM_TYPE_ID, TestDb};
        use crate::schema::{attachments, entities};
        use diesel::prelude::*;

        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        diesel::insert_into(entities::table)
            .values((
                entities::id.eq("e1"),
                entities::name.eq("Thing"),
                entities::entity_type_id.eq(ITEM_TYPE_ID),
            ))
            .execute(&mut conn)
            .unwrap();
        diesel::insert_into(attachments::table)
            .values((
                attachments::id.eq("a1"),
                attachments::entity_id.eq("e1"),
                attachments::kind.eq(AttachmentKind::Receipt),
                attachments::mime_type.eq("application/pdf"),
                attachments::sha256.eq("00"),
                attachments::size_bytes.eq(2_i64),
            ))
            .execute(&mut conn)
            .unwrap();
        let (stored_text, stored_kind): (String, AttachmentKind) = attachments::table
            .select((
                diesel::dsl::sql::<diesel::sql_types::Text>("kind"),
                attachments::kind,
            ))
            .first(&mut conn)
            .unwrap();
        assert_eq!(stored_text, "receipt");
        assert_eq!(stored_kind, AttachmentKind::Receipt);
    }
}
