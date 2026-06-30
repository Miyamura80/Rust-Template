# PRD: Tauri → Rust Server + CLI/API Template

## 1. Overview

Convert this Tauri desktop template into a **Rust application-server template**
with a unified CLI & HTTP API interface over a single shared core, plus an
**optional** Bun/React frontend for visualization. Inspired by
[`Miyamura80/MCP-Template`](https://github.com/Miyamura80/MCP-Template):
*write business logic once, ship it as a CLI subcommand and an HTTP route* —
with an MCP transport designed-for but **not built** in this iteration.

All Tauri/desktop scaffolding is removed.

## 2. Goals / Non-Goals

### Goals
- One service core (`engine`) exposed through multiple transports.
- A typed command contract (schemars JSON Schema) shared across CLI + API (+ future MCP).
- An `axum`-based HTTP API.
- A single CLI binary with subcommands (`call`, `serve`, `doctor`, `probe`, `run-scenario`).
- Optional React/Vite frontend that talks to the HTTP API via `fetch`.
- Config relocated out of the (deleted) Tauri crate into a standalone crate.
- The core compiles and tests pass **standalone at every phase** (no broken intermediate states).

### Non-Goals (this iteration)
- Building the MCP server (designed-for only; left as a documented stub).
- Authentication/authorization beyond a pluggable middleware seam.
- Database / persistence layer.
- Desktop features (tray, deep links, auto-updater, native bundling).

## 3. Target Architecture

```
        ┌──────────────────────────────────────────────────────────┐
        │  TRANSPORTS  (crates/cli — one binary, subcommands)        │
        │                                                            │
        │   appctl call <cmd> --args '{...}'   one-shot JSON I/O     │
        │   appctl serve --http :8080          axum HTTP API         │
        │   appctl doctor | probe | run-scenario                     │
        │   appctl mcp                         (LATER — stub)        │
        └───────────────┬─────────────────────────┬──────────────────┘
                        │                          │
          optional ─────┘                          │  same registry
          bun/React frontend ──HTTP/fetch──▶ serve  │  + typed contract
                                                    │
        ┌───────────────────────────────────────────▼───────────────┐
        │  crates/engine  — the service core (no transport deps)      │
        │                                                             │
        │    Command trait:  Input: JsonSchema + Deserialize          │
        │                    Output: JsonSchema + Serialize           │
        │                    fn run(input, &AppContext) -> Result     │
        │    CommandRegistry: name → boxed typed command              │
        │                     + schema(name) introspection            │
        │    AppContext:      fs / network / clipboard (traits)       │
        │    types:           CommandResult (stable JSON contract)    │
        └───────────────────────────┬─────────────────────────────────┘
                                     │
        ┌────────────────────────────▼────────────────────────────────┐
        │  crates/config  — AppConfig / FrontendConfig (moved here)     │
        │                   YAML + APP__ env overrides + sanitizer       │
        └────────────────────────────────────────────────────────────────┘
```

### 3.1 Crate layout (after)

```
Cargo.toml                 # workspace: engine, config, cli
crates/
  engine/                  # service core + typed Command registry
  config/                  # AppConfig/FrontendConfig + loader (moved from src-tauri)
  cli/                     # `appctl` binary: call / serve / doctor / probe / scenario / (mcp stub)
    src/
      main.rs              # clap entrypoint
      serve_http.rs        # axum app (replaces serve.rs UDS daemon)
      ...
frontend/   (optional)     # React/Vite app, fetch()-based API client (moved from src/)
docs/                      # docs site + this PRD
```

`src-tauri/`, `src/` (Tauri frontend), and `crates/onboard/` (orphan, no
Cargo.toml) are removed.

## 4. The Typed Command Contract (core decision)

Replace the untyped `fn(Value, &AppContext) -> Result<Value, CommandError>`
with a typed trait so every transport gets schemas for free.

```
trait Command {
    type Input:  DeserializeOwned + JsonSchema;
    type Output: Serialize + JsonSchema;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn run(&self, input: Self::Input, ctx: &AppContext)
        -> Result<Self::Output, CommandError>;
}
```

- The registry stores **type-erased** entries (an object-safe inner trait that
  takes/returns `serde_json::Value`, with deserialize→run→serialize wrapped
  inside) **plus** the input/output `schemars::schema_for!` outputs.
- `CommandResult` (the stable envelope: run_id, status, timing, error, data)
  is unchanged — typed output is serialized into its `data` field.
- New introspection: `registry.schema(name) -> { input_schema, output_schema }`,
  consumed by the API (`GET /commands`, OpenAPI) and the future MCP `tools/list`.

New deps: `schemars` (engine), `clap` already present.

## 5. HTTP API (axum)

Replace the Unix-socket daemon (`crates/cli/src/serve.rs`) with axum routes
reusing the same registry and `CommandResult` envelope:

```
GET  /healthz                  → liveness
GET  /commands                 → list + JSON Schemas (introspection)
POST /commands/:name           → run command; body = Input JSON; resp = CommandResult
POST /probe/:target            → run probe        (filesystem|network|clipboard)
GET  /doctor                   → env report
```

- tower middleware: CORS (for the frontend), tracing, request-id, timeout.
- Keep the daemon's request/response shapes available if a non-HTTP transport
  is still wanted; otherwise retire `DaemonRequest`/`DaemonResponse`.
- Error mapping: `CommandError::error_code()` → HTTP status (InvalidInput→400,
  PermissionDenied→403, IoError→500, etc.), body still a `CommandResult`.

New deps (cli): `axum`, `tower`, `tower-http` (cors, trace).

## 6. Config relocation

Move `src-tauri/src/global_config.rs` + `global_config.yaml` into
`crates/config`:
- Keep the `AppConfig` (full, with secret API keys) vs `FrontendConfig`
  (sanitized) split — the sanitizer is reused for any payload the API exposes
  to the frontend.
- Fix the loader's path logic: it currently falls back to a hard-coded
  `src-tauri/` prefix. Re-anchor to the config crate's `CARGO_MANIFEST_DIR` /
  a configurable base path / `APP_CONFIG_PATH` env.
- Port the existing config tests (env override precedence, type coercion,
  sanitization) verbatim into the new crate.

## 7. Frontend (optional, HTTP client)

- Move `src/` → `frontend/`. Remove `@tauri-apps/api`, `-plugin-opener`,
  `-plugin-updater`, `-plugin-process`; delete `UpdateNotification` and the
  tauri update hook.
- Replace `invoke('engine_call', …)` with `fetch('/commands/:name', …)`; add a
  tiny typed API client. `useConfig()` calls `GET` a config endpoint instead of
  the tauri `get_app_config` command.
- Vite dev server proxies `/api` → `appctl serve`. Frontend is fully optional:
  the template is useful headless with just the CLI + API.

## 8. MCP (designed-for, not built)

No MCP server this iteration. The typed registry + `schema(name)` introspection
is precisely what an MCP transport needs:
- Future `appctl mcp` adapter maps `tools/list` → registry schemas and
  `tools/call` → `registry.execute`. Leave a stub subcommand returning
  "unimplemented" and a `docs/` note describing the adapter.

## 9. Teardown checklist (Tauri/desktop removal)

- Delete `src-tauri/` (lib.rs, main.rs, logging.rs, asset_gen.rs, global_config,
  capabilities, icons, tauri.conf.json, build.rs).
- Remove `src-tauri` from workspace members; add `crates/config`.
- `package.json`: drop `@tauri-apps/*`, `tauri` script; rename app; move to
  `frontend/`.
- `Makefile`: replace `tauri-dev`/`tauri-build` with `run` (`cargo run -p cli --
  serve`) / `cargo build`; fix `test` to `cargo test --workspace` (currently
  `cd src-tauri && cargo test`).
- CI: `rust_checks.yaml` — drop GTK/WebKit apt deps; `build_verification.yaml`
  — replace `tauri build` with `cargo build --workspace`; remove/replace
  `release.yml` (Tauri bundling, signing) with a binary release if wanted.
- Remove asset-gen (Gemini icon/banner pipeline) and `make banner`/`logo`, or
  decouple from desktop assets.
- Clean orphan `crates/onboard/` (delete, or promote to a real crate).
- Docs: rewrite `README.md`, `CLAUDE.md` (Tauri → server framing), update
  `update-backend`/`code-quality` skills, archive the old `docs/PRD.md`.

## 10. Phased plan (each phase leaves the tree green)

```
Phase 0  Baseline:  cargo test --workspace passes; branch ready.
Phase 1  Config:    extract crates/config from src-tauri; port tests;
                    src-tauri temporarily depends on it. Tree green.
Phase 2  Contract:  introduce typed Command trait + schemars; port the 5
                    example commands; registry keeps execute()+adds schema().
                    CLI `call` unchanged externally. Tree green.
Phase 3  HTTP API:  add `appctl serve --http` (axum) reusing registry;
                    /healthz /commands /commands/:name /probe /doctor.
                    Retire UDS daemon. Tree green.
Phase 4  Teardown:  delete src-tauri + tauri frontend deps; fix workspace,
                    Makefile, CI; clean orphan crate. Pure server+CLI. Green.
Phase 5  Frontend:  move src→frontend, convert invoke()→fetch(); optional.
Phase 6  MCP stub + docs/README/CLAUDE rewrite; final CI + prek pass.
```

Rationale for order: config and the typed contract must exist **before** Tauri
is deleted, so the core always compiles standalone.

## 11. Risks & mitigations

| Risk | Mitigation |
|------|------------|
| Config loader path assumptions break when src-tauri is gone | Re-anchor to config-crate manifest dir + `APP_CONFIG_PATH`; do it in Phase 1 while src-tauri still builds. |
| Typed-registry type erasure is fiddly in Rust | Object-safe inner trait doing Value↔typed conversion; commands implement the typed trait only. |
| Frontend rewrite scope creep | Frontend is optional and last; ship CLI+API value before touching it. |
| Loss of desktop features later regretted | Documented explicitly as a non-goal; Tauri layer was thin and re-addable atop `engine`. |
| `clipboard`/`emit` commands meaningless server-side | Keep as capability examples (degrade to Unsupported headless) or swap for server-relevant example commands. |

## 12. Open questions

- Release strategy after dropping Tauri bundles: ship CLI binaries via
  `cargo-dist` / GitHub Releases, or out of scope?
- Keep asset-gen (Gemini) at all, or remove with desktop branding?
- Frontend: keep React, or is a minimal static page enough for "visualization"?
```
