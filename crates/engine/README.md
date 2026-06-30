# engine – Shared Backend Logic

Platform-agnostic engine crate that contains all real backend logic for the
Tauri template app. Used by both the GUI (`src-tauri`) and the headless CLI
test harness (`crates/cli`).

## Design Principles

- **No Tauri dependency** – the engine never imports Tauri types, so it can run
  in any Rust context (CLI, tests, WASM, etc.).
- **Trait-based OS access** – filesystem, network, and clipboard operations are
  behind traits (`FilesystemOps`, `NetworkOps`, `ClipboardOps`). Callers inject
  the implementation they need (real platform vs. headless stubs).
- **Structured results** – every operation returns a `CommandResult` with a
  stable JSON schema including `run_id`, `status`, `error`, `timing_ms`, and
  `env_summary`.
- **No panics on missing capabilities** – headless environments get `SKIP` or
  `UNSUPPORTED` error codes instead of crashes.

## Modules

| Module | Purpose |
|--------|---------|
| `types` | Output contract: `CommandResult`, `Status`, `ErrorCode`, `EnvSummary`, scenario/daemon types |
| `traits` | OS capability traits: `FilesystemOps`, `NetworkOps`, `ClipboardOps` |
| `platform` | Real implementations (`StdFilesystem`, `ReqwestNetwork`, `SystemClipboard`) + `HeadlessClipboard` |
| `context` | `AppContext` – holds trait objects and config; constructors for platform/headless |
| `commands` | `CommandRegistry` with built-in commands: `ping`, `read_file`, `write_file` |
| `probes` | Capability probes: `filesystem`, `network`, `clipboard` |
| `doctor` | Environment diagnostics (OS, kernel, headless detection, proxy vars) |
| `scenario` | YAML scenario parser and async runner |

## Usage

```rust
use engine::{AppContext, CommandRegistry};

// Create context with real platform capabilities
let ctx = AppContext::default_platform();
let registry = CommandRegistry::new();

// Execute a command
let result = registry.execute("ping", serde_json::json!({}), &ctx);
assert_eq!(result.status, engine::Status::Pass);

// Run a probe
let probe_result = engine::probes::run_probe("filesystem", &ctx).await;
```

## Adding Commands

Register new commands in `CommandRegistry::new()`:

```rust
impl CommandRegistry {
    pub fn new() -> Self {
        let mut reg = Self { handlers: HashMap::new() };
        reg.register("ping", cmd_ping);
        reg.register("my_command", cmd_my_command);
        reg
    }
}

fn cmd_my_command(args: Value, ctx: &AppContext) -> Result<Value, CommandError> {
    let input = args.get("key").and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::InvalidInput("missing 'key'".into()))?;
    Ok(serde_json::json!({ "result": input }))
}
```

## OS Traits

Implement custom capability providers by implementing the traits:

```rust
use engine::traits::{ClipboardOps, CapResult, CapError};

struct MyClipboard;

impl ClipboardOps for MyClipboard {
    fn read_text(&self) -> CapResult<String> {
        Err(CapError::Unsupported("not available".into()))
    }
    fn write_text(&self, _text: &str) -> CapResult<()> {
        Err(CapError::Unsupported("not available".into()))
    }
}
```
