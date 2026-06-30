---
name: update-backend
description: Guide for making changes to the Rust backend of the Tauri template, covering the engine crate, CLI harness, Tauri commands, and testing patterns.
---

# Update Backend Skill

Use this skill whenever you are modifying Rust backend logic — adding commands, probes, traits, or configuration in `crates/engine`, `crates/cli`, or `src-tauri`.

## Architecture

The backend is split into three layers:

| Layer | Path | Role |
|-------|------|------|
| **engine** | `crates/engine/` | All real backend logic. No Tauri dependency — runs in CLI, tests, and WASM. |
| **appctl CLI** | `crates/cli/` | Headless test harness that drives `engine` for VM/CI compatibility testing. |
| **src-tauri** | `src-tauri/` | Tauri host: wraps `engine` commands as Tauri `#[tauri::command]` handlers. |

### Design Principles (engine)

- **No Tauri dependency** — never import Tauri types inside `crates/engine`.
- **Trait-based OS access** — filesystem, network, and clipboard go through `FilesystemOps`, `NetworkOps`, `ClipboardOps`. Inject real platform or headless stubs via `AppContext`.
- **Structured results** — every operation returns `CommandResult` with `run_id`, `status`, `error`, `timing_ms`, and `env_summary`.
- **No panics on missing capabilities** — headless environments get `SKIP` or `UNSUPPORTED` error codes.

## Code Style (Rust)

- `snake_case` for functions/modules/variables
- `PascalCase` for structs/enums
- Run `cargo fmt` before committing; pass `cargo clippy` with no warnings

## Adding a Backend Command

1. Implement the handler in `crates/engine/src/commands/`:

```rust
fn cmd_my_command(args: Value, ctx: &AppContext) -> Result<Value, CommandError> {
    let input = args.get("key").and_then(|v| v.as_str())
        .ok_or_else(|| CommandError::InvalidInput("missing 'key'".into()))?;
    Ok(serde_json::json!({ "result": input }))
}
```

2. Register it in `CommandRegistry::new()`:

```rust
reg.register("my_command", cmd_my_command);
```

3. Expose it in `src-tauri` as a Tauri command (if the GUI needs it):

```rust
#[tauri::command]
fn my_command(args: serde_json::Value, ctx: tauri::State<AppContext>) -> CommandResult {
    ctx.registry.execute("my_command", args, &ctx)
}
```

4. Smoke-test headlessly with `appctl`:

```bash
cargo build -p appctl
appctl call my_command --args '{"key": "value"}' --json
```

## Adding an OS Capability (Trait)

Implement the relevant trait from `crates/engine/src/traits.rs`:

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

Inject via `AppContext` — real platform in `src-tauri`, headless stubs in tests and `appctl`.

## Configuration

Source of truth: `src-tauri/global_config.yaml` (`.env` overrides).
Access in Rust:

```rust
let config = crate::global_config::get_config();
println!("Model: {}", config.default_llm.default_model);
```

Config is loaded in `src-tauri/src/global_config.rs` and **not** imported by `crates/engine` (keep engine config-agnostic unless needed).

## Testing with appctl

The `appctl` CLI drives `engine` without a running Tauri process:

```bash
# Build
cargo build -p appctl

# Diagnostics
appctl doctor --json

# Call a command
appctl call ping --json
appctl call read_file --args '{"path": "/etc/hostname"}' --json

# Run a capability probe
appctl probe filesystem --json
appctl probe network --json

# Run a YAML scenario
appctl run-scenario scenario.yaml --json
```

Write scenario files for regression tests:

```yaml
name: my feature smoke test
steps:
  - call: "my_command"
    args:
      key: "value"
    expect_status: "pass"
```

## Output Contract

Every command result has this stable JSON schema:

```json
{
  "run_id": "uuid",
  "command": "call|probe|doctor|run-scenario",
  "target": "<cmd or probe name>",
  "status": "pass|fail|skip|error",
  "error": { "code": "ERROR_CODE", "message": "..." },
  "timing_ms": { "total": 1234 },
  "env_summary": { "os": "linux|macos", "arch": "x86_64|aarch64", "headless": true },
  "data": {}
}
```

Error codes: `INVALID_INPUT`, `UNSUPPORTED`, `UNIMPLEMENTED`, `DEPENDENCY_MISSING`,
`PERMISSION_DENIED`, `NETWORK_ERROR`, `IO_ERROR`, `TIMEOUT`, `EXTERNAL_INTERFERENCE`, `INTERNAL_ERROR`.

## Checklist Before Committing

- [ ] `cargo fmt` applied
- [ ] `cargo clippy` passes (no warnings)
- [ ] `cargo test` passes
- [ ] New command smoke-tested with `appctl call <cmd> --json`
- [ ] `engine` crate has no Tauri imports
