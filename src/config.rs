//! Process configuration read from environment variables.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

pub const DEFAULT_PORT: u16 = 7008;
pub const DEFAULT_LISTEN_ADDRESS: &str = "::";
pub const DEFAULT_DATABASE_URL: &str = "data/db.sqlite";
pub const DEFAULT_DATA_DIR: &str = "data";

/// Everything the server needs from its environment, parsed once at startup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    pub listen_address: String,
    pub database_url: String,
    pub data_dir: PathBuf,
}

impl Config {
    /// Reads configuration from the process environment (after loading `.env` if present).
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Builds a config from any key lookup, so tests do not touch the real environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let port = match lookup("PORT") {
            None => DEFAULT_PORT,
            Some(raw) => raw.trim().parse::<u16>().with_context(|| {
                format!("PORT must be a number between 1 and 65535, got {raw:?}")
            })?,
        };
        if port == 0 {
            bail!("PORT must be between 1 and 65535, got 0");
        }
        let listen_address = non_empty(
            lookup("LISTEN_ADDRESS"),
            "LISTEN_ADDRESS",
            DEFAULT_LISTEN_ADDRESS,
        )?;
        let database_url = non_empty(lookup("DATABASE_URL"), "DATABASE_URL", DEFAULT_DATABASE_URL)?;
        let data_dir = non_empty(lookup("DATA_DIR"), "DATA_DIR", DEFAULT_DATA_DIR)?;
        Ok(Self {
            port,
            listen_address,
            database_url,
            data_dir: PathBuf::from(data_dir),
        })
    }
}

fn non_empty(value: Option<String>, name: &str, default: &str) -> Result<String> {
    match value {
        None => Ok(default.to_owned()),
        Some(v) if v.trim().is_empty() => bail!("{name} is set but empty"),
        Some(v) => Ok(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        let map: HashMap<&str, &str> = vars.iter().copied().collect();
        move |key| map.get(key).map(|v| (*v).to_owned())
    }

    #[test]
    fn defaults_when_nothing_is_set() {
        let cfg = Config::from_lookup(lookup(&[])).unwrap();
        assert_eq!(
            cfg,
            Config {
                port: 7008,
                listen_address: "::".to_owned(),
                database_url: "data/db.sqlite".to_owned(),
                data_dir: PathBuf::from("data"),
            }
        );
    }

    #[test]
    fn reads_every_variable() {
        let cfg = Config::from_lookup(lookup(&[
            ("PORT", "9000"),
            ("LISTEN_ADDRESS", "127.0.0.1"),
            ("DATABASE_URL", "/data/db.sqlite"),
            ("DATA_DIR", "/data"),
        ]))
        .unwrap();
        assert_eq!(cfg.port, 9000);
        assert_eq!(cfg.listen_address, "127.0.0.1");
        assert_eq!(cfg.database_url, "/data/db.sqlite");
        assert_eq!(cfg.data_dir, PathBuf::from("/data"));
    }

    #[test]
    fn rejects_an_unparseable_port() {
        // Review focus 1: a typo must not silently become the default port.
        for bad in ["abc", "70000", "", "-1"] {
            let err = Config::from_lookup(lookup(&[("PORT", bad)])).unwrap_err();
            assert!(err.to_string().contains("PORT"), "{bad:?}: {err}");
        }
    }

    #[test]
    fn empty_string_for_a_path_is_rejected() {
        let err = Config::from_lookup(lookup(&[("DATABASE_URL", "")])).unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
        let err = Config::from_lookup(lookup(&[("DATA_DIR", "")])).unwrap_err();
        assert!(err.to_string().contains("DATA_DIR"));
    }
}
