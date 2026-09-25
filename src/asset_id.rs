//! Homebox-style asset numbers, shown as `000-007`. Zero means "no asset id".

use std::fmt;
use std::str::FromStr;

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::BigInt;
use diesel::sqlite::{Sqlite, SqliteValue};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, AsExpression, FromSqlRow,
)]
#[diesel(sql_type = BigInt)]
pub struct AssetId(pub i64);

impl AssetId {
    pub const NONE: Self = Self(0);

    pub fn is_none(self) -> bool {
        self.0 <= 0
    }

    /// `Some("000-007")`, or `None` when there is no asset id.
    pub fn display_option(self) -> Option<String> {
        (!self.is_none()).then(|| self.to_string())
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_none() {
            return Ok(());
        }
        let digits = format!("{:06}", self.0);
        let (head, tail) = digits.split_at(digits.len() - 3);
        write!(f, "{head}-{tail}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid asset id {0:?}: expected digits, optionally as 000-000")]
pub struct ParseAssetIdError(String);

impl FromStr for AssetId {
    type Err = ParseAssetIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let cleaned: String = s.trim().chars().filter(|c| *c != '-').collect();
        if cleaned.is_empty() {
            return Ok(Self::NONE);
        }
        cleaned
            .parse::<i64>()
            .map(Self)
            .map_err(|_| ParseAssetIdError(s.to_owned()))
    }
}

impl FromSql<BigInt, Sqlite> for AssetId {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(Self(<i64 as FromSql<BigInt, Sqlite>>::from_sql(value)?))
    }
}

impl ToSql<BigInt, Sqlite> for AssetId {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0);
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_homebox() {
        assert_eq!(AssetId(7).to_string(), "000-007");
        assert_eq!(AssetId(123456).to_string(), "123-456");
        assert_eq!(AssetId(1234567).to_string(), "1234-567");
        assert_eq!(AssetId::NONE.to_string(), "");
        assert_eq!(AssetId(-3).display_option(), None);
        assert_eq!(AssetId(7).display_option().as_deref(), Some("000-007"));
    }

    #[test]
    fn parses_dashed_plain_and_empty() {
        assert_eq!("000-007".parse::<AssetId>().unwrap(), AssetId(7));
        assert_eq!("7".parse::<AssetId>().unwrap(), AssetId(7));
        assert_eq!(" 000007 ".parse::<AssetId>().unwrap(), AssetId(7));
        assert_eq!("".parse::<AssetId>().unwrap(), AssetId::NONE);
        assert!("abc".parse::<AssetId>().is_err());
    }
}
