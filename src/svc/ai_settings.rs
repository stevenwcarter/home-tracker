//! The AI settings: `ai.*` rows in `settings`, overlaid by the environment.
//!
//! The API key is write-only: it goes into the database (the SQLite file is
//! treated as secret) and comes back out only as [`AiConfig`] for the model
//! client. [`AiSettingsView`], which GraphQL returns, says whether a key
//! exists and never what it is, and no error message here includes it.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use axum::http::Uri;
use axum::http::uri::Scheme;
use diesel::prelude::*;

use crate::ai::REDACTED;
use crate::ai::client::{AiClient, ChatRequest, ContentPart, Message, Role};
use crate::ai::env::{API_KEY_VAR, AiEnv, BASE_URL_VAR};
use crate::schema::settings;
use crate::svc::{optional_text, required_text};

pub const BASE_URL_KEY: &str = "ai.base_url";
pub const API_KEY_KEY: &str = "ai.api_key";
pub const VISION_MODEL_KEY: &str = "ai.vision_model";
pub const SYNTHESIS_MODEL_KEY: &str = "ai.synthesis_model";
pub const EXTRA_INSTRUCTIONS_KEY: &str = "ai.extra_instructions";

/// The longest `extra_instructions` accepted, in characters.
pub const MAX_EXTRA_INSTRUCTIONS_CHARS: usize = 4000;

/// What the settings screen shows: the effective values, minus the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSettingsView {
    pub base_url: String,
    pub vision_model: String,
    pub synthesis_model: String,
    pub extra_instructions: Option<String>,
    pub has_api_key: bool,
    /// The environment variables overriding a stored value, e.g. `OPENAI_API_KEY`.
    pub from_environment: Vec<&'static str>,
}

/// A save from the settings screen. Every field but the key is replaced;
/// `api_key` is `None` to keep the stored key, `""` to clear it, and anything
/// else to replace it. No `Debug`: it carries the key.
pub struct AiSettingsUpdate {
    pub base_url: String,
    pub vision_model: String,
    pub synthesis_model: String,
    pub extra_instructions: Option<String>,
    pub api_key: Option<String>,
}

/// The resolved settings a model call needs; exists only when a key does.
/// `Debug` redacts the key.
#[derive(Clone, PartialEq, Eq)]
pub struct AiConfig {
    pub base_url: String,
    pub api_key: String,
    pub vision_model: String,
    pub synthesis_model: String,
    pub extra_instructions: Option<String>,
}

impl fmt::Debug for AiConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiConfig")
            .field("base_url", &self.base_url)
            .field("api_key", &REDACTED)
            .field("vision_model", &self.vision_model)
            .field("synthesis_model", &self.synthesis_model)
            .field("extra_instructions", &self.extra_instructions)
            .finish()
    }
}

/// `raw` trimmed and without trailing slashes, so `{base}/chat/completions`
/// has exactly one slash; refused unless it is an `http`/`https` URL with a
/// host and no `user:password@` part (credentials in the URL would be sent
/// and shown on the settings screen; the key has its own write-only field).
/// The message never echoes the input, which may hold credentials.
pub fn normalise_base_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let uri = Uri::from_str(trimmed).ok().filter(|uri| {
        uri.scheme()
            .is_some_and(|s| *s == Scheme::HTTP || *s == Scheme::HTTPS)
            && uri.host().is_some_and(|host| !host.is_empty())
    });
    let Some(uri) = uri else {
        bail!("base URL must be an http:// or https:// URL");
    };
    ensure!(
        !uri.authority().is_some_and(|a| a.as_str().contains('@')),
        "base URL must not contain a user name or password"
    );
    Ok(trimmed.to_owned())
}

/// The settings as the screen shows them, environment overrides applied.
pub fn view(conn: &mut SqliteConnection, env: &AiEnv) -> Result<AiSettingsView> {
    let stored = Stored::load(conn)?;
    Ok(AiSettingsView {
        base_url: effective_base_url(env, stored.base_url)?,
        vision_model: stored.vision_model,
        synthesis_model: stored.synthesis_model,
        extra_instructions: stored.extra_instructions,
        has_api_key: env.api_key.is_some() || stored.api_key.is_some(),
        from_environment: env.overridden(),
    })
}

/// Validates `update` whole, then saves it in one transaction. A field the
/// environment overrides is refused, naming the variable, if the update
/// would change it; the base URL shown on screen may be sent back as is.
pub fn update(
    conn: &mut SqliteConnection,
    env: &AiEnv,
    update: AiSettingsUpdate,
) -> Result<AiSettingsView> {
    let key = KeyChange::parse(update.api_key);
    if env.api_key.is_some() && !matches!(key, KeyChange::Keep) {
        bail!("the API key cannot be changed here: {API_KEY_VAR} is set in the environment");
    }
    let base_url = match env_base_url(env)? {
        None => Some(normalise_base_url(&update.base_url)?),
        Some(from_env) => {
            ensure!(
                normalise_base_url(&update.base_url).is_ok_and(|url| url == from_env),
                "the base URL cannot be changed here: {BASE_URL_VAR} is set in the environment"
            );
            None
        }
    };
    let vision_model = required_text(&update.vision_model, "vision model")?;
    let synthesis_model = required_text(&update.synthesis_model, "synthesis model")?;
    let extra_instructions = optional_text(update.extra_instructions);
    if let Some(text) = &extra_instructions {
        ensure!(
            text.chars().count() <= MAX_EXTRA_INSTRUCTIONS_CHARS,
            "extra instructions must be at most {MAX_EXTRA_INSTRUCTIONS_CHARS} characters"
        );
    }

    conn.transaction::<_, anyhow::Error, _>(|conn| {
        if let Some(url) = &base_url {
            put(conn, BASE_URL_KEY, Some(url))?;
        }
        put(conn, VISION_MODEL_KEY, Some(&vision_model))?;
        put(conn, SYNTHESIS_MODEL_KEY, Some(&synthesis_model))?;
        put(conn, EXTRA_INSTRUCTIONS_KEY, extra_instructions.as_deref())?;
        match &key {
            KeyChange::Keep => {}
            KeyChange::Clear => put(conn, API_KEY_KEY, None)?,
            KeyChange::Replace(k) => put(conn, API_KEY_KEY, Some(k))?,
        }
        Ok(())
    })?;
    view(conn, env)
}

/// The resolved settings for a model call, environment first; `None` when
/// there is no key anywhere (AI is not configured).
pub fn config(conn: &mut SqliteConnection, env: &AiEnv) -> Result<Option<AiConfig>> {
    let stored = Stored::load(conn)?;
    let Some(api_key) = env.api_key.clone().or(stored.api_key) else {
        return Ok(None);
    };
    Ok(Some(AiConfig {
        base_url: effective_base_url(env, stored.base_url)?,
        api_key,
        vision_model: stored.vision_model,
        synthesis_model: stored.synthesis_model,
        extra_instructions: stored.extra_instructions,
    }))
}

/// The outcome of [`test_connection`], as the settings screen shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiTestResult {
    pub ok: bool,
    /// Which model answered and how fast, or why the call failed.
    pub message: String,
    pub latency_ms: i32,
}

/// Sends the synthesis model the smallest possible chat completion and
/// reports whether an answer came back. Takes no connection, so the caller
/// releases its database connection before the network call.
pub async fn test_connection(config: &AiConfig, client: &dyn AiClient) -> AiTestResult {
    let request = ChatRequest {
        model: config.synthesis_model.clone(),
        messages: vec![Message {
            role: Role::User,
            content: vec![ContentPart::Text(
                "Reply with the single word OK.".to_owned(),
            )],
        }],
        response_format: None,
        max_tokens: Some(8),
    };
    let started = Instant::now();
    let answer = client.chat(config, request).await;
    let latency_ms = i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX);
    match answer {
        Ok(response) => AiTestResult {
            ok: true,
            message: format!("Connected: {} answered in {latency_ms} ms", response.model),
            latency_ms,
        },
        Err(err) => AiTestResult {
            ok: false,
            message: err.to_string(),
            latency_ms,
        },
    }
}

/// What an update does to the stored key.
enum KeyChange {
    Keep,
    Clear,
    Replace(String),
}

impl KeyChange {
    /// `None` keeps, blank clears, anything else replaces (trimmed).
    fn parse(input: Option<String>) -> Self {
        match input.as_deref().map(str::trim) {
            None => Self::Keep,
            Some("") => Self::Clear,
            Some(key) => Self::Replace(key.to_owned()),
        }
    }
}

/// The `ai.*` rows as stored. The seeded rows must exist; the key and the
/// instructions are optional.
struct Stored {
    base_url: String,
    api_key: Option<String>,
    vision_model: String,
    synthesis_model: String,
    extra_instructions: Option<String>,
}

impl Stored {
    fn load(conn: &mut SqliteConnection) -> Result<Self> {
        let mut rows: HashMap<String, String> = settings::table
            .filter(settings::key.eq_any([
                BASE_URL_KEY,
                API_KEY_KEY,
                VISION_MODEL_KEY,
                SYNTHESIS_MODEL_KEY,
                EXTRA_INSTRUCTIONS_KEY,
            ]))
            .select((settings::key, settings::value))
            .load::<(String, String)>(conn)
            .context("reading the AI settings")?
            .into_iter()
            .collect();
        Ok(Self {
            base_url: seeded(&mut rows, BASE_URL_KEY)?,
            vision_model: seeded(&mut rows, VISION_MODEL_KEY)?,
            synthesis_model: seeded(&mut rows, SYNTHESIS_MODEL_KEY)?,
            // An empty key row (never written by `update`) is no key.
            api_key: rows.remove(API_KEY_KEY).filter(|key| !key.is_empty()),
            extra_instructions: rows.remove(EXTRA_INSTRUCTIONS_KEY),
        })
    }
}

/// A row the migration seeds; absence means a damaged database, not a default.
fn seeded(rows: &mut HashMap<String, String>, key: &str) -> Result<String> {
    rows.remove(key)
        .with_context(|| format!("settings row {key:?} is missing"))
}

/// `OPENAI_BASE_URL`, normalised, when it is set.
fn env_base_url(env: &AiEnv) -> Result<Option<String>> {
    env.base_url
        .as_deref()
        .map(|raw| normalise_base_url(raw).with_context(|| format!("{BASE_URL_VAR} is invalid")))
        .transpose()
}

/// The environment's base URL when set, else the stored one.
fn effective_base_url(env: &AiEnv, stored: String) -> Result<String> {
    Ok(env_base_url(env)?.unwrap_or(stored))
}

/// Upserts setting `key`, or deletes its row when `value` is `None`. The
/// error names the key, never the value.
fn put(conn: &mut SqliteConnection, key: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(value) => diesel::insert_into(settings::table)
            .values((settings::key.eq(key), settings::value.eq(value)))
            .on_conflict(settings::key)
            .do_update()
            .set(settings::value.eq(value))
            .execute(conn),
        None => diesel::delete(settings::table.filter(settings::key.eq(key))).execute(conn),
    }
    .with_context(|| format!("saving setting {key:?}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::TestDb;

    const ROW_KEY: &str = "sk-row-secret";

    fn save(conn: &mut SqliteConnection, api_key: Option<&str>) -> AiSettingsView {
        update(
            conn,
            &AiEnv::none(),
            AiSettingsUpdate {
                base_url: "https://row.example/v1".to_owned(),
                vision_model: "vision".to_owned(),
                synthesis_model: "synthesis".to_owned(),
                extra_instructions: Some("Prefer metric units.".to_owned()),
                api_key: api_key.map(str::to_owned),
            },
        )
        .unwrap()
    }

    #[test]
    fn config_is_none_without_a_key_anywhere() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        assert_eq!(config(&mut conn, &AiEnv::none()).unwrap(), None);
        save(&mut conn, None);
        assert_eq!(config(&mut conn, &AiEnv::none()).unwrap(), None);
    }

    #[test]
    fn config_resolves_the_environment_over_the_rows() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        save(&mut conn, Some(ROW_KEY));

        let from_rows = config(&mut conn, &AiEnv::none()).unwrap().unwrap();
        assert_eq!(
            from_rows,
            AiConfig {
                base_url: "https://row.example/v1".to_owned(),
                api_key: ROW_KEY.to_owned(),
                vision_model: "vision".to_owned(),
                synthesis_model: "synthesis".to_owned(),
                extra_instructions: Some("Prefer metric units.".to_owned()),
            }
        );

        let env = AiEnv {
            api_key: Some("sk-env".to_owned()),
            base_url: Some("http://llm.lan:8080/v1/".to_owned()),
        };
        let from_env = config(&mut conn, &env).unwrap().unwrap();
        assert_eq!(from_env.api_key, "sk-env");
        assert_eq!(from_env.base_url, "http://llm.lan:8080/v1");
        assert_eq!(
            from_env.vision_model, "vision",
            "models always come from rows"
        );

        // The environment alone is enough to configure AI.
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let only_env = AiEnv {
            api_key: Some("sk-env".to_owned()),
            base_url: None,
        };
        let config = config(&mut conn, &only_env).unwrap().unwrap();
        assert_eq!(config.api_key, "sk-env");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn an_invalid_environment_base_url_names_the_variable() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        let env = AiEnv {
            api_key: None,
            base_url: Some("ftp://nope".to_owned()),
        };
        let err = view(&mut conn, &env).unwrap_err();
        assert!(format!("{err:#}").contains(BASE_URL_VAR), "{err:#}");
    }

    #[test]
    fn config_debug_redacts_the_key() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        save(&mut conn, Some(ROW_KEY));
        let config = config(&mut conn, &AiEnv::none()).unwrap().unwrap();
        let shown = format!("{config:?}");
        assert!(!shown.contains(ROW_KEY), "{shown}");
        assert!(
            shown.contains(REDACTED) && shown.contains("row.example"),
            "{shown}"
        );
    }

    #[test]
    fn normalise_base_url_refuses_credentials_without_echoing_them() {
        for bad in [
            "https://user:sk-in-url@host/v1",
            "http://user@127.0.0.1:8080/v1",
        ] {
            let err = normalise_base_url(bad).unwrap_err().to_string();
            assert!(err.contains("user name or password"), "{bad:?}: {err}");
            assert!(
                !err.contains("sk-in-url") && !err.contains("user@"),
                "{err}"
            );
        }
    }

    #[test]
    fn normalise_base_url_trims_and_strips_trailing_slashes() {
        assert_eq!(
            normalise_base_url("  https://host/v1//  ").unwrap(),
            "https://host/v1"
        );
        assert_eq!(
            normalise_base_url("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        for bad in [
            "ftp://x",
            "not a url",
            "",
            "   ",
            "https://",
            "host/v1",
            "/v1",
        ] {
            assert!(normalise_base_url(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn a_missing_seeded_row_is_an_error_not_a_default() {
        let db = TestDb::new();
        let mut conn = db.pool.get().unwrap();
        put(&mut conn, VISION_MODEL_KEY, None).unwrap();
        let err = view(&mut conn, &AiEnv::none()).unwrap_err();
        assert!(err.to_string().contains(VISION_MODEL_KEY), "{err}");
    }
}
