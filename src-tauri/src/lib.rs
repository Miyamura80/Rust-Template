pub mod global_config;
pub mod logging;

pub use global_config as config;

use global_config::FrontendConfig;
// ---------------------------------------------------------------------------
// Engine integration
// ---------------------------------------------------------------------------

use engine::{AppContext, CommandRegistry};
use std::sync::OnceLock;

static ENGINE_CTX: OnceLock<AppContext> = OnceLock::new();
static ENGINE_REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();

fn engine_ctx() -> &'static AppContext {
    ENGINE_CTX.get_or_init(AppContext::default_platform)
}

fn engine_registry() -> &'static CommandRegistry {
    ENGINE_REGISTRY.get_or_init(CommandRegistry::new)
}

// ---------------------------------------------------------------------------
// Tauri commands – thin wrappers that delegate to engine
// ---------------------------------------------------------------------------

#[tauri::command]
fn greet(name: &str) -> String {
    // Simple greeting – delegates to engine ping to prove wiring
    let result = engine_registry().execute("ping", serde_json::json!({}), engine_ctx());
    format!(
        "Hello, {}! You've been greeted from Rust! (engine status: {:?})",
        name, result.status
    )
}

#[tauri::command]
fn get_app_config() -> &'static FrontendConfig {
    global_config::get_frontend_config()
}

/// Generic command invocation – call any engine command by name.
#[tauri::command]
fn engine_call(cmd: String, args: serde_json::Value) -> serde_json::Value {
    let result = engine_registry().execute(&cmd, args, engine_ctx());
    serde_json::to_value(&result).unwrap_or_default()
}

/// List all available engine commands.
#[tauri::command]
fn engine_list_commands() -> Vec<String> {
    engine_registry()
        .list()
        .into_iter()
        .map(String::from)
        .collect()
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Install ring as the rustls crypto provider (reqwest needs this with rustls-no-provider)
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Initialize logging
    logging::init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_app_config,
            engine_call,
            engine_list_commands,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
