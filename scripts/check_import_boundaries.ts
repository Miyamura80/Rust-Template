#!/usr/bin/env bun
/**
 * Crate-boundary import lint.
 *
 * The platform-agnostic core crate `crates/engine` must have NO dependency on
 * any transport crate. Transports (CLI + HTTP API) live in `crates/cli`
 * (package `appctl`); the engine is meant to be reusable by every transport, so
 * a dependency edge from engine -> a transport crate would invert the layering
 * described in CLAUDE.md ("The `engine` core has no transport dependency").
 *
 * This is a deterministic Cargo.toml parse -- no network, no cargo invocation.
 * It fails with a clear message if a forbidden edge exists.
 */

import { existsSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// The core crate that must stay transport-free.
const CORE_CRATE = "crates/engine";

// Transport crates the core must never depend on: directory name -> package name.
const FORBIDDEN_CRATES: Array<{ dir: string; pkg: string }> = [
	{ dir: "cli", pkg: "appctl" },
];

// Also match target-conditional tables, e.g.
// `[target.'cfg(unix)'.dependencies]` / `[target.x86_64-pc-windows-msvc.dev-dependencies]`,
// so a forbidden dep hidden under a platform section isn't silently missed.
const DEP_TABLE_RE =
	/^\[(?:target\..*\.)?(dependencies|dev-dependencies|build-dependencies)\]\s*$/;
const TABLE_RE = /^\[/;
// A dependency line: `key = ...` or `key.feature = ...`. Captures the crate key.
const DEP_KEY_RE = /^([A-Za-z0-9_-]+)(\s*\.\s*[A-Za-z0-9_-]+)?\s*=/;

interface Violation {
	line: number;
	text: string;
	reason: string;
}

function forbiddenNames(): Set<string> {
	const names = new Set<string>();
	for (const c of FORBIDDEN_CRATES) {
		names.add(c.pkg);
		names.add(c.dir);
	}
	return names;
}

function forbiddenPathFragments(): string[] {
	// e.g. `path = "../cli"` -- match `/cli"` or `"cli"`.
	return FORBIDDEN_CRATES.map((c) => c.dir);
}

function checkManifest(manifestPath: string): Violation[] {
	const violations: Violation[] = [];
	const text = readFileSync(manifestPath, "utf-8");
	const lines = text.split("\n");
	const names = forbiddenNames();
	const pathFrags = forbiddenPathFragments();

	let inDepTable = false;
	for (let i = 0; i < lines.length; i++) {
		const raw = lines[i];
		const line = raw.replace(/#.*$/, "").trim();
		if (line === "") continue;

		if (DEP_TABLE_RE.test(line)) {
			inDepTable = true;
			continue;
		}
		if (TABLE_RE.test(line)) {
			inDepTable = false;
			continue;
		}
		if (!inDepTable) continue;

		// Dependency key match (`appctl = ...`, `cli = ...`, `cli.workspace = ...`).
		const m = line.match(DEP_KEY_RE);
		if (m && names.has(m[1])) {
			violations.push({
				line: i + 1,
				text: raw.trim(),
				reason: `depends on transport crate '${m[1]}'`,
			});
			continue;
		}

		// Path-based dependency onto a transport crate dir (`path = "../cli"`).
		for (const frag of pathFrags) {
			const pathRe = new RegExp(`path\\s*=\\s*"[^"]*(^|/)${frag}"`);
			const altRe = new RegExp(
				`path\\s*=\\s*"[^"]*/${frag}"|path\\s*=\\s*"${frag}"`,
			);
			if (pathRe.test(line) || altRe.test(line)) {
				violations.push({
					line: i + 1,
					text: raw.trim(),
					reason: `path dependency onto transport crate dir '${frag}'`,
				});
				break;
			}
		}
	}
	return violations;
}

function main(): number {
	const manifest = join(REPO, CORE_CRATE, "Cargo.toml");
	if (!existsSync(manifest)) {
		console.error(
			`import_lint: core crate manifest not found at ${CORE_CRATE}/Cargo.toml`,
		);
		return 1;
	}

	const violations = checkManifest(manifest);
	if (violations.length > 0) {
		console.error(
			`import_lint FAILED: '${CORE_CRATE}' must not depend on any transport crate.`,
		);
		for (const v of violations) {
			console.error(
				`  ${CORE_CRATE}/Cargo.toml:${v.line}: ${v.text}  (${v.reason})`,
			);
		}
		console.error(
			"The engine core is transport-agnostic; move transport-specific code into crates/cli.",
		);
		return 1;
	}

	console.log(
		`import_lint passed: '${CORE_CRATE}' has no transport-crate dependencies.`,
	);
	return 0;
}

process.exit(main());
