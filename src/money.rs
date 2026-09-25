//! Money as integer minor units. The currency code lives in `settings`.

use diesel::deserialize::{self, FromSql, FromSqlRow};
use diesel::expression::AsExpression;
use diesel::serialize::{self, IsNull, Output, ToSql};
use diesel::sql_types::BigInt;
use diesel::sqlite::{Sqlite, SqliteValue};
use tracing::warn;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, AsExpression, FromSqlRow,
)]
#[diesel(sql_type = BigInt)]
pub struct Cents(pub i64);

impl Cents {
    /// Converts a major-unit float (Homebox stores `1099.99`) to cents, rounding
    /// half away from zero.
    pub fn from_major_f64(major: f64) -> Self {
        Self((major * 100.0).round() as i64)
    }

    /// GraphQL `Int` is 32-bit; anything larger saturates and is logged.
    pub fn as_graphql_int(self) -> i32 {
        i32::try_from(self.0).unwrap_or_else(|_| {
            warn!(
                cents = self.0,
                "cents value exceeds GraphQL Int; saturating"
            );
            if self.0 < 0 { i32::MIN } else { i32::MAX }
        })
    }
}

impl FromSql<BigInt, Sqlite> for Cents {
    fn from_sql(value: SqliteValue<'_, '_, '_>) -> deserialize::Result<Self> {
        Ok(Self(<i64 as FromSql<BigInt, Sqlite>>::from_sql(value)?))
    }
}

impl ToSql<BigInt, Sqlite> for Cents {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(self.0);
        Ok(IsNull::No)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_homebox_floats_to_cents() {
        assert_eq!(Cents::from_major_f64(1099.99), Cents(109_999));
        assert_eq!(Cents::from_major_f64(0.0), Cents(0));
        assert_eq!(Cents::from_major_f64(152.99), Cents(15_299));
        assert_eq!(Cents::from_major_f64(0.005), Cents(1));
        assert_eq!(Cents::from_major_f64(-2.5), Cents(-250));
    }

    #[test]
    fn saturates_for_graphql() {
        assert_eq!(Cents(293_294).as_graphql_int(), 293_294);
        assert_eq!(Cents(i64::MAX).as_graphql_int(), i32::MAX);
        assert_eq!(Cents(i64::MIN).as_graphql_int(), i32::MIN);
    }
}
