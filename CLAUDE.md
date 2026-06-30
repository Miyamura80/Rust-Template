This file provides guidance to AI agents working with code in this repository.

## Project Overview

Tauri template for desktop application development with React and TypeScript.
**Note:** This project has migrated away from Python. Use Rust for backend logic and Node/Bun for frontend/scripts.
**Before any other work in this repo, enable prek:** `bun add -g prek && prek install`. Hooks are defined in `prek.toml`.

## Common Commands

```bash
# Frontend / Tauri
bun install             # Install dependencies
bun run tauri dev       # Run the app in development mode
bun run build           # Build the frontend
bun run tauri build     # Build the Tauri application
bun run check           # Run formatting and linting (Biome)

# Rust / Backend
cargo test              # Run Rust tests
cargo check             # Check Rust code
cargo clippy            # Run Rust linter
```

## Architecture

- **src/** - Tauri frontend (React + TypeScript + Vite)
- **src-tauri/** - Tauri host (Rust) — wraps engine commands as Tauri handlers
- **crates/engine/** - Platform-agnostic backend logic (no Tauri dependency)
- **crates/cli/** - Headless CLI (`appctl`) for testing engine logic
- **docs/** - Documentation (Next.js app)

> **Making backend changes?** Use the `update-backend` skill for architecture details, command patterns, trait implementations, config access, and `appctl` testing workflows.

## Code Style

### TypeScript (Frontend)
- `camelCase` for functions/variables
- `PascalCase` for components/classes
- Use Biome for formatting/linting

## Configuration Pattern

Configuration is handled in Rust and exposed to the frontend.
Source of truth: `src-tauri/global_config.yaml` (and `.env` overrides).

```rust
// Accessing config in Rust
let config = crate::global_config::get_config();
println!("Model: {}", config.default_llm.default_model);
```

## Commit Message Convention

Use emoji prefixes indicating change type and magnitude (multiple emojis = 5+ files):
- 🏗️ initial implementation
- 🔨 feature changes
- 🐛 bugfix
- ✨ formatting/linting only
- ✅ feature complete with E2E tests
- ⚙️ config changes
- 💽 DB schema/migrations

## Long-Running Code Pattern

Structure as: `init()` → `continue(id)` → `cleanup(id)`
- Keep state serializable
- Use descriptive IDs (runId, taskId)
- Handle rate limits, timeouts, retries at system boundaries

## Subagents

- Folder-size CI failure → spawn subagent `.claude/agents/folder-refactor-advisor.md`.

## Git Workflow
- **Review**: Always trigger Greptile review MCP before pushing a PR and resolve any branch issues. If the Greptile MCP is not available, explicitly inform the user.
- **Protected Branch**: `main` is protected. Do not push directly to `main`. Use PRs.
- **Merge Strategy**: Squash and merge.
- **Pre-commit CI gate**: Always run `make ci` before committing any changes. Ensure it passes with zero errors. Do not commit if `make ci` fails - fix all issues first, then commit.
- **Prek hooks**: Always run `prek install` before starting work on a new PR to ensure Git hooks are active.

## Runbooks

Operational runbooks live in `docs/runbooks/`. After resolving a difficult issue that required significant back-and-forth or investigation, ask the user: "Should I add a runbook for this?" and if yes, create a new markdown file in `docs/runbooks/` documenting the symptoms, root cause, and resolution steps.

---

## Automated Translation (Jules Sync)

Docs under `docs/content/` are auto-translated by the **Jules Translation Sync**
workflow (`.github/workflows/jules-sync-translations.yml`). Do NOT manually
translate doc files — edit the English source and the workflow will update all
locales (`zh`, `es`, `ja`).

See [`docs/translation-guide.md`](docs/translation-guide.md) for the full
glossary, file naming conventions, and translation rules.
See [`docs/ops.md`](docs/ops.md) for operational runbook and failure modes.
