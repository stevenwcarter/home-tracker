use crate::graphql::context::GraphQLContext;
use crate::svc::ai_settings::AiSettingsView;

/// The AI settings; says whether an API key exists, never what it is.
#[juniper::graphql_object(context = GraphQLContext, name = "AiSettings")]
impl AiSettingsView {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn vision_model(&self) -> &str {
        &self.vision_model
    }
    fn synthesis_model(&self) -> &str {
        &self.synthesis_model
    }
    /// Appended to both system prompts; null when unset.
    fn extra_instructions(&self) -> Option<&str> {
        self.extra_instructions.as_deref()
    }
    fn has_api_key(&self) -> bool {
        self.has_api_key
    }
    /// The environment variables overriding a setting, e.g. `OPENAI_API_KEY`;
    /// the screen shows those fields as set from the environment. (A Rust
    /// `from_*` method taking `&self` reads as a constructor, hence the rename.)
    #[graphql(name = "fromEnvironment")]
    fn environment_overrides(&self) -> &[&'static str] {
        &self.from_environment
    }
}
