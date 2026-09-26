//! Per-request GraphQL context and the authorization seam.

use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use diesel::SqliteConnection;
use diesel::r2d2::{ConnectionManager, PooledConnection};
use juniper::{FieldError, FieldResult};

use crate::db::SqlitePool;

/// What a user may do. v1 has no users; the enum exists so mutations gate on it now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    ReadOnly,
    Write,
}

/// Who is making the request. Auth will replace `Anonymous` with `User`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// No authentication configured. Has full access in v1.
    Anonymous,
    User {
        id: String,
        role: Role,
    },
}

impl Actor {
    pub fn can_write(&self) -> bool {
        match self {
            Self::Anonymous => true,
            Self::User { role, .. } => *role == Role::Write,
        }
    }
}

#[derive(Clone)]
pub struct GraphQLContext {
    pub pool: SqlitePool,
    pub actor: Actor,
    /// Where originals live, so deleting an attachment can remove its file.
    pub data_dir: Arc<Path>,
}

impl juniper::Context for GraphQLContext {}

impl GraphQLContext {
    pub fn new(pool: SqlitePool, actor: Actor, data_dir: impl Into<Arc<Path>>) -> Self {
        Self {
            pool,
            actor,
            data_dir: data_dir.into(),
        }
    }

    /// One pooled connection for the duration of a resolver.
    pub fn conn(&self) -> anyhow::Result<PooledConnection<ConnectionManager<SqliteConnection>>> {
        self.pool
            .get()
            .context("could not get a database connection")
    }

    /// The single gate every mutation calls first.
    pub fn require_write(&self) -> FieldResult<()> {
        if self.actor.can_write() {
            Ok(())
        } else {
            Err(FieldError::new(
                "Forbidden: write access required",
                juniper::Value::null(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;

    #[test]
    fn anonymous_can_write_in_v1() {
        assert!(Actor::Anonymous.can_write());
    }

    #[test]
    fn read_only_users_are_refused() {
        let db = TestDb::new();
        let data = tempfile::tempdir().unwrap();
        let ctx = GraphQLContext::new(
            db.pool.clone(),
            Actor::User {
                id: "u1".to_owned(),
                role: Role::ReadOnly,
            },
            data.path(),
        );
        assert!(ctx.require_write().is_err());
        let ctx = GraphQLContext::new(
            db.pool,
            Actor::User {
                id: "u1".to_owned(),
                role: Role::Write,
            },
            data.path(),
        );
        assert!(ctx.require_write().is_ok());
    }
}
