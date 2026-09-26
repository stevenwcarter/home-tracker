//! Environment overrides for the AI settings, read once at startup.

use std::env;
use std::fmt;

use super::REDACTED;

/// The variable that overrides the stored API key.
pub const API_KEY_VAR: &str = "OPENAI_API_KEY";
/// The variable that overrides the stored base URL.
pub const BASE_URL_VAR: &str = "OPENAI_BASE_URL";

/// `OPENAI_API_KEY` and `OPENAI_BASE_URL` as the process saw them at
/// startup; each wins over its settings row when set. `Debug` redacts the key.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct AiEnv {
    pub api_key: Option<String>,
    /// Raw; `svc::ai_settings` normalises it wherever it is used.
    pub base_url: Option<String>,
}

impl AiEnv {
    /// Reads both variables from the process environment; blank is unset.
    pub fn from_env() -> Self {
        Self::from_lookup(|name| env::var(name).ok())
    }

    /// Builds the overrides from any lookup, so tests never touch the real
    /// environment.
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let read = |name| {
            lookup(name).and_then(|value| {
                let trimmed = value.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_owned())
            })
        };
        Self {
            api_key: read(API_KEY_VAR),
            base_url: read(BASE_URL_VAR),
        }
    }

    /// No overrides: every AI setting comes from the database.
    pub fn none() -> Self {
        Self::default()
    }

    /// The names of the variables that are set, for the settings screen.
    pub fn overridden(&self) -> Vec<&'static str> {
        [
            (API_KEY_VAR, self.api_key.is_some()),
            (BASE_URL_VAR, self.base_url.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect()
    }
}

impl fmt::Debug for AiEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AiEnv")
            .field("api_key", &self.api_key.as_ref().map(|_| REDACTED))
            .field("base_url", &self.base_url)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn blank_variables_are_unset_and_values_are_trimmed() {
        let env = AiEnv::from_lookup(lookup(&[
            (API_KEY_VAR, "  sk-env  "),
            (BASE_URL_VAR, "   "),
        ]));
        assert_eq!(env.api_key.as_deref(), Some("sk-env"));
        assert_eq!(env.base_url, None);
        assert_eq!(env.overridden(), [API_KEY_VAR]);
        assert_eq!(AiEnv::from_lookup(lookup(&[])), AiEnv::none());
    }

    #[test]
    fn overridden_lists_both_in_a_stable_order() {
        let env = AiEnv::from_lookup(lookup(&[(BASE_URL_VAR, "http://x"), (API_KEY_VAR, "k")]));
        assert_eq!(env.overridden(), [API_KEY_VAR, BASE_URL_VAR]);
        assert!(AiEnv::none().overridden().is_empty());
    }

    #[test]
    fn debug_redacts_the_key() {
        let env = AiEnv {
            api_key: Some("sk-secret-env".to_owned()),
            base_url: Some("http://llm.lan/v1".to_owned()),
        };
        let shown = format!("{env:?}");
        assert!(!shown.contains("sk-secret-env"), "{shown}");
        assert!(
            shown.contains(REDACTED) && shown.contains("llm.lan"),
            "{shown}"
        );
    }
}
