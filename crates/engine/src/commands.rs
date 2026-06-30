//! Command registry and built-in example commands.
//!
//! Commands are registered by name and invoked with JSON input/output.

use crate::context::AppContext;
use crate::types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Instant;

/// Signature for all engine commands.
pub type CommandHandler = fn(Value, &AppContext) -> Result<Value, CommandError>;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("{0}")]
    Other(String),
}

impl CommandError {
    pub fn error_code(&self) -> ErrorCode {
        match self {
            CommandError::InvalidInput(_) => ErrorCode::InvalidInput,
            CommandError::Io(_) => ErrorCode::IoError,
            CommandError::PermissionDenied(_) => ErrorCode::PermissionDenied,
            CommandError::Other(_) => ErrorCode::InternalError,
        }
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub struct CommandRegistry {
    handlers: HashMap<String, CommandHandler>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            handlers: HashMap::new(),
        };
        // Register built-in commands
        reg.register("ping", cmd_ping);
        reg.register("read_file", cmd_read_file);
        reg.register("write_file", cmd_write_file);
        reg.register("system_info", cmd_system_info);
        reg.register("list_dir", cmd_list_dir);
        reg
    }

    pub fn register(&mut self, name: &str, handler: CommandHandler) {
        self.handlers.insert(name.to_string(), handler);
    }

    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.handlers.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Execute a command by name and return a full CommandResult.
    pub fn execute(&self, name: &str, args: Value, ctx: &AppContext) -> CommandResult {
        let run_id = new_run_id();
        let start = Instant::now();

        let handler = match self.handlers.get(name) {
            Some(h) => h,
            None => {
                return result_err(
                    "call",
                    name,
                    &run_id,
                    start.elapsed().as_millis() as u64,
                    ErrorCode::InvalidInput,
                    format!("unknown command: {}", name),
                );
            }
        };

        match handler(args, ctx) {
            Ok(data) => {
                let mut r = result_ok("call", name, &run_id, start.elapsed().as_millis() as u64);
                r.data = Some(data);
                r
            }
            Err(e) => result_err(
                "call",
                name,
                &run_id,
                start.elapsed().as_millis() as u64,
                e.error_code(),
                e.to_string(),
            ),
        }
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Built-in commands
// ===========================================================================

/// `ping` – returns { "pong": true }. Proves wiring works.
fn cmd_ping(_args: Value, _ctx: &AppContext) -> Result<Value, CommandError> {
    Ok(serde_json::json!({ "pong": true }))
}

/// `read_file` – read a file, return its contents as a UTF-8 string.
///
/// Args: `{ "path": "/absolute/path" }`
/// Returns: `{ "content": "...", "size_bytes": 123 }`
fn cmd_read_file(args: Value, ctx: &AppContext) -> Result<Value, CommandError> {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::InvalidInput("missing 'path' string field".into()))?;

    let path = std::path::Path::new(path_str);
    let data = ctx.fs().read_file(path).map_err(|e| match e {
        crate::traits::CapError::PermissionDenied(m) => CommandError::PermissionDenied(m),
        crate::traits::CapError::Io(io) => CommandError::Io(io),
        other => CommandError::Other(other.to_string()),
    })?;

    let content = String::from_utf8_lossy(&data);
    Ok(serde_json::json!({
        "content": content,
        "size_bytes": data.len(),
    }))
}

/// `write_file` – write string content to a file.
///
/// Args: `{ "path": "/absolute/path", "content": "hello" }`
/// Returns: `{ "bytes_written": 5 }`
fn cmd_write_file(args: Value, ctx: &AppContext) -> Result<Value, CommandError> {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::InvalidInput("missing 'path' string field".into()))?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::InvalidInput("missing 'content' string field".into()))?;

    let path = std::path::Path::new(path_str);
    let data = content.as_bytes();
    ctx.fs().write_file(path, data).map_err(|e| match e {
        crate::traits::CapError::PermissionDenied(m) => CommandError::PermissionDenied(m),
        crate::traits::CapError::Io(io) => CommandError::Io(io),
        other => CommandError::Other(other.to_string()),
    })?;

    Ok(serde_json::json!({ "bytes_written": data.len() }))
}

/// `system_info` – return OS, architecture, and hostname.
///
/// Args: `{}` (none required)
/// Returns: `{ "os": "macos", "arch": "aarch64", "hostname": "...", "headless": false }`
fn cmd_system_info(_args: Value, _ctx: &AppContext) -> Result<Value, CommandError> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string());

    Ok(serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "hostname": hostname,
        "headless": crate::types::detect_headless(),
    }))
}

/// `list_dir` – list entries in a directory.
///
/// Args: `{ "path": "/some/dir" }`
/// Returns: `{ "entries": [{ "name": "foo.txt", "is_dir": false, "size_bytes": 42 }, ...] }`
fn cmd_list_dir(args: Value, ctx: &AppContext) -> Result<Value, CommandError> {
    let path_str = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::InvalidInput("missing 'path' string field".into()))?;

    let path = std::path::Path::new(path_str);
    let dir_entries = ctx.fs().list_dir(path).map_err(|e| match e {
        crate::traits::CapError::PermissionDenied(m) => CommandError::PermissionDenied(m),
        crate::traits::CapError::Io(io) => CommandError::Io(io),
        other => CommandError::Other(other.to_string()),
    })?;

    let entries: Vec<Value> = dir_entries
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "name": e.name,
                "is_dir": e.is_dir,
                "size_bytes": e.size_bytes,
            })
        })
        .collect();

    Ok(serde_json::json!({ "entries": entries }))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::AppContext;

    #[test]
    fn test_ping_command() {
        let ctx = AppContext::default_headless();
        let reg = CommandRegistry::new();
        let result = reg.execute("ping", serde_json::json!({}), &ctx);
        assert_eq!(result.status, Status::Pass);
        assert_eq!(result.data.unwrap()["pong"], true);
    }

    #[test]
    fn test_unknown_command() {
        let ctx = AppContext::default_headless();
        let reg = CommandRegistry::new();
        let result = reg.execute("nonexistent", serde_json::json!({}), &ctx);
        assert_eq!(result.status, Status::Error);
        assert_eq!(result.error.unwrap().code, ErrorCode::InvalidInput);
    }

    #[test]
    fn test_read_write_file() {
        let ctx = AppContext::default_headless();
        let reg = CommandRegistry::new();

        let tmp = std::env::temp_dir().join("engine_test_rw.txt");
        let path_str = tmp.to_str().unwrap();

        // Write
        let w = reg.execute(
            "write_file",
            serde_json::json!({ "path": path_str, "content": "hello engine" }),
            &ctx,
        );
        assert_eq!(w.status, Status::Pass);

        // Read back
        let r = reg.execute("read_file", serde_json::json!({ "path": path_str }), &ctx);
        assert_eq!(r.status, Status::Pass);
        assert_eq!(r.data.unwrap()["content"], "hello engine");

        // Cleanup
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_list_commands() {
        let reg = CommandRegistry::new();
        let names = reg.list();
        assert!(names.contains(&"ping"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
        assert!(names.contains(&"system_info"));
        assert!(names.contains(&"list_dir"));
    }

    #[test]
    fn test_system_info_command() {
        let ctx = AppContext::default_headless();
        let reg = CommandRegistry::new();
        let result = reg.execute("system_info", serde_json::json!({}), &ctx);
        assert_eq!(result.status, Status::Pass);
        let data = result.data.unwrap();
        assert!(data["os"].is_string());
        assert!(data["arch"].is_string());
        assert!(data["hostname"].is_string());
    }

    #[test]
    fn test_list_dir_command() {
        let ctx = AppContext::default_headless();
        let reg = CommandRegistry::new();

        let tmp = std::env::temp_dir();
        let path_str = tmp.to_str().unwrap();
        let result = reg.execute("list_dir", serde_json::json!({ "path": path_str }), &ctx);
        assert_eq!(result.status, Status::Pass);
        let data = result.data.unwrap();
        assert!(data["entries"].is_array());
    }

    #[test]
    fn test_list_dir_not_a_directory() {
        let ctx = AppContext::default_headless();
        let reg = CommandRegistry::new();
        let result = reg.execute(
            "list_dir",
            serde_json::json!({ "path": "/nonexistent_dir_12345" }),
            &ctx,
        );
        assert_eq!(result.status, Status::Error);
    }
}
