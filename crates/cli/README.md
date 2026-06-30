# appctl – CLI Test Harness

Headless CLI for invoking the same engine logic that powers the Tauri GUI app.
Designed for VM-based compatibility testing on macOS and Linux.

## Build

```bash
cargo build -p appctl
# Binary at target/debug/appctl (or target/release/appctl with --release)
```

## Commands

### doctor

Collect environment facts (OS, kernel, headless detection, proxy vars).

```bash
# Human-readable
appctl doctor

# JSON output
appctl doctor --json

# Write result to file
appctl doctor --json --out /tmp/env.json
```

### call

Invoke a backend command by name with JSON arguments.

```bash
# Ping (prove wiring works)
appctl call ping --json

# Read a file
appctl call read_file --args '{"path": "/etc/hostname"}' --json

# Write a file
appctl call write_file --args '{"path": "/tmp/test.txt", "content": "hello"}' --json

# With artifacts directory
appctl call ping --json --artifacts /tmp/artifacts
```

### probe

Targeted capability checks.

```bash
# Filesystem probe (create/read/write/delete in temp dir)
appctl probe filesystem --json

# Network probe (DNS resolve + HTTPS GET)
appctl probe network --json

# Clipboard probe (returns SKIP if headless)
appctl probe clipboard --json
```

### run-scenario

Execute a scripted scenario from a YAML file.

```yaml
# scenario.yaml
name: basic smoke test
steps:
  - call: "ping"
    args: {}
    expect_status: "pass"
  - call: "write_file"
    args:
      path: "/tmp/scenario_test.txt"
      content: "written by scenario"
    expect_status: "pass"
  - call: "read_file"
    args:
      path: "/tmp/scenario_test.txt"
    expect_status: "pass"
  - probe: "filesystem"
```

```bash
appctl run-scenario scenario.yaml --json
appctl run-scenario scenario.yaml --artifacts /tmp/artifacts
```

### serve

Start a daemon over a Unix socket. Accepts newline-delimited JSON requests.

```bash
appctl serve --socket /tmp/appctl.sock
```

Protocol:

```json
// Request
{"id": "1", "method": "call", "params": {"cmd": "ping", "args": {}}}

// Response
{"id": "1", "result": {"run_id": "...", "status": "pass", ...}}
```

Supported methods: `call`, `probe`, `doctor`.

### emit

Desktop event simulation (skeleton -- returns UNIMPLEMENTED or UNSUPPORTED).

```bash
appctl emit tray-click --json
appctl emit deep-link --json
appctl emit file-drop --json
appctl emit app-focus --json
```

## Output Contract

Every command produces a result with this stable JSON schema:

```json
{
  "run_id": "uuid",
  "command": "call|probe|doctor|run-scenario|emit|serve",
  "target": "<cmd or probe name>",
  "status": "pass|fail|skip|error",
  "error": { "code": "ERROR_CODE", "message": "..." },
  "timing_ms": { "total": 1234, "steps": { "init": 10, "work": 1200 } },
  "artifacts": [],
  "env_summary": { "os": "linux|macos", "arch": "x86_64|aarch64", "headless": true },
  "data": {}
}
```

Error codes: `INVALID_INPUT`, `UNSUPPORTED`, `UNIMPLEMENTED`, `DEPENDENCY_MISSING`,
`PERMISSION_DENIED`, `NETWORK_ERROR`, `IO_ERROR`, `TIMEOUT`, `EXTERNAL_INTERFERENCE`,
`INTERNAL_ERROR`.

## Artifacts

When `--artifacts <dir>` is provided, the CLI writes:

```
<dir>/<run_id>/
  result.json      # Full result object
  events.jsonl     # JSON Lines log of events
```

## Exit Codes

- `0` -- pass or skip
- `1` -- fail
- `2` -- error
