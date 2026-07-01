//! Application configuration, shared across every transport (CLI, HTTP API,
//! and future MCP).
//!
//! [`AppConfig`] is the full configuration including secret credentials;
//! [`FrontendConfig`] is the sanitized projection safe to expose to a browser
//! over HTTP. The sanitizer is a **security boundary** — no secret field may
//! ever cross it (enforced by `#[serde(skip_serializing)]` and covered by the
//! tests in this crate).
//!
//! Config is loaded from `global_config.yaml` (next to this crate by default,
//! or `APP_CONFIG_PATH`), layered with an optional `production_config.yaml` /
//! `.global_config.yaml`, then overridden by `APP__`-prefixed env vars.

use config::{Config, ConfigError, Environment, File};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AppConfig {
    pub model_name: String,
    pub dot_global_config_health_check: bool,
    #[serde(default = "default_dev_env")]
    pub dev_env: String,

    pub example_parent: ExampleParent,
    pub default_llm: DefaultLlm,
    pub llm_config: LlmConfig,
    pub logging: LoggingConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub features: HashMap<String, bool>,

    // Secret credentials (optional in config file, usually injected via env).
    // `#[serde(skip_serializing)]` is the security boundary: these never
    // serialize into any payload (see `FrontendConfig` and the sanitization
    // test). Prefer the accessor methods below for read access.
    #[serde(skip_serializing)]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing)]
    pub anthropic_api_key: Option<String>,
    #[serde(skip_serializing)]
    pub groq_api_key: Option<String>,
    #[serde(skip_serializing)]
    pub perplexity_api_key: Option<String>,
    #[serde(skip_serializing)]
    pub gemini_api_key: Option<String>,
}

impl AppConfig {
    pub fn openai_api_key(&self) -> Option<&str> {
        self.openai_api_key.as_deref()
    }
    pub fn anthropic_api_key(&self) -> Option<&str> {
        self.anthropic_api_key.as_deref()
    }
    pub fn groq_api_key(&self) -> Option<&str> {
        self.groq_api_key.as_deref()
    }
    pub fn perplexity_api_key(&self) -> Option<&str> {
        self.perplexity_api_key.as_deref()
    }
    pub fn gemini_api_key(&self) -> Option<&str> {
        self.gemini_api_key.as_deref()
    }
}

/// A sanitized version of the configuration intended for exposure to the frontend.
/// This strictly excludes sensitive information like API keys.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FrontendConfig {
    pub model_name: String,
    pub dot_global_config_health_check: bool,
    pub dev_env: String,
    pub example_parent: ExampleParent,
    pub default_llm: DefaultLlm,
    pub llm_config: LlmConfig,
    pub features: HashMap<String, bool>,
}

impl From<&AppConfig> for FrontendConfig {
    fn from(config: &AppConfig) -> Self {
        Self {
            model_name: config.model_name.clone(),
            dot_global_config_health_check: config.dot_global_config_health_check,
            dev_env: config.dev_env.clone(),
            example_parent: config.example_parent.clone(),
            default_llm: config.default_llm.clone(),
            llm_config: config.llm_config.clone(),
            features: config.features.clone(),
        }
    }
}

fn default_dev_env() -> String {
    "dev".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExampleParent {
    pub example_child: String,
}

/// HTTP server bind settings for `appctl serve`. Overridable via
/// `APP__SERVER__HOST` / `APP__SERVER__PORT` or CLI flags.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DefaultLlm {
    pub default_model: String,
    pub fallback_model: Option<String>,
    pub default_temperature: f32,
    pub default_max_tokens: i32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LlmConfig {
    pub cache_enabled: bool,
    pub retry: RetryConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RetryConfig {
    pub max_attempts: i32,
    pub min_wait_seconds: i32,
    pub max_wait_seconds: i32,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingConfig {
    pub verbose: bool,
    pub format: LoggingFormatConfig,
    pub levels: LoggingLevelsConfig,
    #[serde(default)]
    pub redaction: RedactionConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingFormatConfig {
    pub show_time: bool,
    pub show_session_id: bool,
    pub location: LoggingLocationConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingLocationConfig {
    pub enabled: bool,
    pub show_file: bool,
    pub show_function: bool,
    pub show_line: bool,
    pub show_for_info: bool,
    pub show_for_debug: bool,
    pub show_for_warning: bool,
    pub show_for_error: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingLevelsConfig {
    pub debug: bool,
    pub info: bool,
    pub warning: bool,
    pub error: bool,
    pub critical: bool,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct RedactionConfig {
    #[serde(default = "true_default")]
    pub enabled: bool,
    #[serde(default = "true_default")]
    pub use_default_pii: bool,
    #[serde(default)]
    pub patterns: Vec<RedactionPattern>,
}

fn true_default() -> bool {
    true
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RedactionPattern {
    pub name: String,
    pub regex: String,
    pub placeholder: String,
}

#[derive(Clone, Copy)]
struct ConfigStorage {
    app: &'static AppConfig,
    frontend: &'static FrontendConfig,
}

static CONFIG_STORAGE: RwLock<Option<ConfigStorage>> = RwLock::new(None);

fn get_config_storage() -> ConfigStorage {
    if let Some(storage) = *CONFIG_STORAGE.read().unwrap() {
        return storage;
    }

    let mut write = CONFIG_STORAGE.write().unwrap();
    if let Some(storage) = *write {
        return storage;
    }

    let app_config =
        load_config().unwrap_or_else(|e| panic!("Failed to load configuration: {}", e));
    let frontend_config = FrontendConfig::from(&app_config);

    let app_cfg = Box::leak(Box::new(app_config));
    let frontend_cfg = Box::leak(Box::new(frontend_config));

    let storage = ConfigStorage {
        app: app_cfg,
        frontend: frontend_cfg,
    };
    *write = Some(storage);
    storage
}

pub fn get_config() -> &'static AppConfig {
    get_config_storage().app
}

pub fn get_frontend_config() -> &'static FrontendConfig {
    get_config_storage().frontend
}

#[cfg(test)]
pub fn reset_config() {
    let mut write = CONFIG_STORAGE.write().unwrap();
    *write = None;
}

/// Directory that config files are resolved against.
///
/// Defaults to this crate's directory (baked in at compile time via
/// `CARGO_MANIFEST_DIR`), which is correct for `cargo run`/`cargo test`. A
/// deployed binary should point `APP_CONFIG_PATH` at its config file instead;
/// sibling files (`production_config.yaml`, `.global_config.yaml`) are then
/// resolved next to that file.
fn config_base_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn load_config() -> Result<AppConfig, ConfigError> {
    // `APP_CONFIG_PATH` (full path to the primary YAML) overrides the default
    // location; production/local overrides are resolved next to it.
    let config_path = std::env::var("APP_CONFIG_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| config_base_dir().join("global_config.yaml"));

    let config_dir = config_path
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let prod_config_path = config_dir.join("production_config.yaml");
    let local_config_path = config_dir.join(".global_config.yaml");

    let builder = Config::builder()
        // Load default config (mandatory)
        .add_source(File::from(config_path).required(true))
        // Load production config if in prod
        .add_source(File::from(prod_config_path).required(false))
        // Load local override
        .add_source(File::from(local_config_path).required(false))
        // Load environment variables
        // Map nested env vars like APP__LOGGING__VERBOSE=true
        .add_source(Environment::with_prefix("APP").separator("__"));

    builder.build()?.try_deserialize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;

    struct EnvGuard(&'static str);
    impl EnvGuard {
        fn new(key: &'static str, val: &str) -> Self {
            reset_config();
            env::set_var(key, val);
            Self(key)
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            env::remove_var(self.0);
            reset_config();
        }
    }

    #[test]
    #[serial]
    fn test_load_config() {
        // Ensure the config loads without error
        let config = load_config();
        assert!(config.is_ok(), "Failed to load config: {:?}", config.err());

        let config = config.unwrap();
        // Verify some default values from global_config.yaml
        assert_eq!(
            config.default_llm.default_model,
            "gemini/gemini-3-flash-preview"
        );
        assert_eq!(config.llm_config.retry.max_attempts, 3);
    }

    #[test]
    #[serial]
    fn test_env_var_override_precedence() {
        // YAML value is "gemini/gemini-3-flash-preview"
        let _guard = EnvGuard::new("APP__MODEL_NAME", "override-model");

        let config = load_config().expect("Should load config");
        assert_eq!(config.model_name, "override-model");
    }

    #[test]
    #[serial]
    fn test_type_coercion_boolean() {
        {
            let _guard = EnvGuard::new("APP__LLM_CONFIG__CACHE_ENABLED", "true");
            let config = load_config().expect("Should load config");
            assert!(config.llm_config.cache_enabled);
        }

        {
            let _guard = EnvGuard::new("APP__LLM_CONFIG__CACHE_ENABLED", "false");
            let config = load_config().expect("Should load config");
            assert!(!config.llm_config.cache_enabled);
        }

        // Test boolean coercion from '1' and '0' (porting from Python tests)
        {
            let _guard = EnvGuard::new("APP__LOGGING__FORMAT__LOCATION__ENABLED", "1");
            let config = load_config().expect("Should load config");
            assert!(config.logging.format.location.enabled);
        }

        {
            let _guard = EnvGuard::new("APP__LOGGING__FORMAT__LOCATION__ENABLED", "0");
            let config = load_config().expect("Should load config");
            assert!(!config.logging.format.location.enabled);
        }
    }

    #[test]
    #[serial]
    fn test_type_coercion_numeric() {
        let _guard1 = EnvGuard::new("APP__DEFAULT_LLM__DEFAULT_TEMPERATURE", "0.95");
        let _guard2 = EnvGuard::new("APP__LLM_CONFIG__RETRY__MAX_ATTEMPTS", "10");

        let config = load_config().expect("Should load config");
        assert_eq!(config.default_llm.default_temperature, 0.95);
        assert_eq!(config.llm_config.retry.max_attempts, 10);

        // Test float-to-int coercion if applicable (usually handled by serde)
        // Here we test string-to-numeric coercion specifically.
        let _guard3 = EnvGuard::new("APP__LLM_CONFIG__RETRY__MAX_ATTEMPTS", "5");
        let config = load_config().expect("Should load config");
        assert_eq!(config.llm_config.retry.max_attempts, 5);
    }

    #[test]
    #[serial]
    fn test_dev_env_override() {
        // Field name is lowercase `dev_env`
        let _guard = EnvGuard::new("APP__DEV_ENV", "production");
        let config = load_config().expect("Should load config");
        assert_eq!(config.dev_env, "production");
    }

    #[test]
    #[serial]
    fn test_api_key_loading() {
        // Test that API keys are loaded from environment variables (APP__ prefix)
        let _guard1 = EnvGuard::new("APP__OPENAI_API_KEY", "test-openai-key");
        let _guard2 = EnvGuard::new("APP__ANTHROPIC_API_KEY", "test-anthropic-key");

        let config = load_config().expect("Should load config");
        assert_eq!(config.openai_api_key(), Some("test-openai-key"));
        assert_eq!(config.anthropic_api_key(), Some("test-anthropic-key"));
    }

    #[test]
    #[serial]
    fn test_frontend_config_sanitization() {
        let config = AppConfig {
            model_name: "gpt-4".to_string(),
            dot_global_config_health_check: true,
            dev_env: "dev".to_string(),
            example_parent: ExampleParent {
                example_child: "val".to_string(),
            },
            default_llm: DefaultLlm {
                default_model: "gpt-4".to_string(),
                fallback_model: None,
                default_temperature: 0.7,
                default_max_tokens: 100,
            },
            llm_config: LlmConfig {
                cache_enabled: true,
                retry: RetryConfig {
                    max_attempts: 1,
                    min_wait_seconds: 1,
                    max_wait_seconds: 1,
                },
            },
            logging: LoggingConfig {
                verbose: true,
                format: LoggingFormatConfig {
                    show_time: true,
                    show_session_id: true,
                    location: LoggingLocationConfig {
                        enabled: true,
                        show_file: true,
                        show_function: true,
                        show_line: true,
                        show_for_info: true,
                        show_for_debug: true,
                        show_for_warning: true,
                        show_for_error: true,
                    },
                },
                levels: LoggingLevelsConfig {
                    debug: true,
                    info: true,
                    warning: true,
                    error: true,
                    critical: true,
                },
                redaction: RedactionConfig::default(),
            },
            server: ServerConfig::default(),
            features: HashMap::new(),
            openai_api_key: Some("secret-key".to_string()),
            anthropic_api_key: None,
            groq_api_key: None,
            perplexity_api_key: None,
            gemini_api_key: None,
        };

        // The sanitized projection must not leak the secret value or its key.
        let frontend_config = FrontendConfig::from(&config);
        let json = serde_json::to_string(&frontend_config).unwrap();

        assert!(!json.contains("secret-key"));
        assert!(!json.contains("openai_api_key"));

        // Security boundary: even serializing the *full* AppConfig (e.g. by
        // accident in a log line or debug endpoint) must never emit a secret.
        let mut config = config;
        config.openai_api_key = Some("secret-openai".into());
        config.anthropic_api_key = Some("secret-anthropic".into());
        config.groq_api_key = Some("secret-groq".into());
        config.perplexity_api_key = Some("secret-perplexity".into());
        config.gemini_api_key = Some("secret-gemini".into());

        let full_json = serde_json::to_string(&config).unwrap();
        for leaked in [
            "secret-openai",
            "secret-anthropic",
            "secret-groq",
            "secret-perplexity",
            "secret-gemini",
            "api_key",
        ] {
            assert!(
                !full_json.contains(leaked),
                "AppConfig serialization leaked `{leaked}`: {full_json}"
            );
        }
    }

    #[test]
    #[serial]
    fn test_logging_verbose_default_is_false() {
        let config = load_config().expect("Should load config");
        assert!(
            !config.logging.verbose,
            "Logging verbose should be false by default"
        );
    }
}
