#!/usr/bin/env bun
/**
 * Sync Claude <-> Codex skills, subagents, and root agent docs.
 *
 * - Symlinks `.claude/skills/<name>` -> `../../.agents/skills/<name>` for every
 *   directory under `.agents/skills/`.
 * - Regenerates `.codex/agents/<name>.toml` from each `.claude/agents/<name>.md`.
 * - Mirrors every `CLAUDE.md` to a sibling `AGENTS.md` symlink (Codex reads
 *   `AGENTS.md`; Claude reads `CLAUDE.md`).
 * - Auto-prunes dangling symlinks and orphaned TOMLs silently.
 *
 * Pass `--check` to fail (exit 1) if anything was out of date instead of
 * silently fixing it -- used by the prek hook and CI.
 */

import {
	existsSync,
	lstatSync,
	mkdirSync,
	readdirSync,
	readFileSync,
	readlinkSync,
	symlinkSync,
	unlinkSync,
	writeFileSync,
} from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parse as parseYaml } from "yaml";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const SHARED_SKILLS = join(REPO, ".agents", "skills");
const CLAUDE_SKILLS = join(REPO, ".claude", "skills");
const CLAUDE_AGENTS = join(REPO, ".claude", "agents");
const CODEX_AGENTS = join(REPO, ".codex", "agents");

const FRONTMATTER_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?([\s\S]*)$/;
const CLAUDE_ONLY_KEYS = new Set([
	"tools",
	"model",
	"color",
	"allowed-tools",
	"disable-model-invocation",
]);

const SHARED_SKILL_FORBIDDEN_KEYS = new Set([
	"allowed-tools",
	"disable-model-invocation",
	"user-invocable",
	"context",
	"agent",
	"model",
	"effort",
	"hooks",
	"paths",
	"shell",
	"argument-hint",
]);

const SHARED_SKILL_FORBIDDEN_BODY_PATTERNS: [RegExp, string][] = [
	[/\$ARGUMENTS\b/, "$ARGUMENTS substitution"],
	[/\$[1-9][0-9]*\b/, "positional arg substitution ($1, $2, ...)"],
	[/\$\{CLAUDE_[A-Z_]+\}/, "${CLAUDE_*} interpolation"],
	[/!`[^`]+`/, "!`cmd` shell preprocessing"],
];
const SHARED_SKILL_RAW_BODY_PATTERNS: [RegExp, string][] = [
	[/^```!\s*$/m, "```! shell preprocessing block"],
];

// Directories skipped when walking the tree for CLAUDE.md files.
const MIRROR_SKIP_DIRS = new Set([
	".git",
	"node_modules",
	"target",
	"dist",
	"out",
	"build",
	".next",
	".cache",
]);

type Frontmatter = Record<string, unknown>;

function rel(p: string): string {
	return relative(REPO, p);
}

function die(msg: string): never {
	console.error(msg);
	process.exit(1);
}

function parseMd(path: string): { meta: Frontmatter; body: string } {
	const text = readFileSync(path, "utf-8");
	const m = text.match(FRONTMATTER_RE);
	if (!m) die(`${path}: missing YAML frontmatter`);
	const meta = (parseYaml(m[1]) as Frontmatter) ?? {};
	const body = m[2].replace(/^[\r\n]+/, "");
	return { meta, body };
}

function tomlBasicString(s: string): string {
	// Only used for `name` and `description`. Frontmatter rules already forbid
	// control chars and newlines in these, so plain escape of backslash, quote,
	// tab, and the newline pair is sufficient.
	const escaped = s
		.replace(/\\/g, "\\\\")
		.replace(/"/g, '\\"')
		.replace(/\t/g, "\\t")
		.replace(/\r/g, "\\r")
		.replace(/\n/g, "\\n");
	return `"${escaped}"`;
}

function tomlMultilineString(s: string): string {
	// Triple-quoted; escape sequences of 3+ double quotes so the string can't
	// close prematurely. A literal `"""` becomes `""\"`.
	const escaped = s.replace(/\\/g, "\\\\").replace(/"""/g, '""\\"');
	// Leading newline right after the opening """ is stripped by TOML, so add one
	// so the content starts on its own line for readability.
	return `"""\n${escaped}"""`;
}

function renderToml(meta: Frontmatter, body: string, source: string): string {
	if (!meta.name) die(`${source}: missing \`name\` in frontmatter`);
	const name = String(meta.name);
	const description = String(meta.description ?? "");
	const instructions = `${body.replace(/\s+$/, "")}\n`;

	let out = "";
	out += `name = ${tomlBasicString(name)}\n`;
	out += `description = ${tomlBasicString(description)}\n`;
	out += `developer_instructions = ${tomlMultilineString(instructions)}\n`;

	const extras = Object.entries(meta).filter(([k]) => CLAUDE_ONLY_KEYS.has(k));
	if (extras.length > 0) {
		out +=
			"\n# Claude-only frontmatter (preserved for reference, not used by Codex):\n";
		for (const [k, v] of extras) {
			out += `# ${k} = ${JSON.stringify(v)}\n`;
		}
	}
	return out;
}

function scanBacktickSpan(
	t: string,
	i: number,
	precededByBang: boolean,
): { emit: string; next: number } {
	const n = t.length;
	let run = 0;
	while (i + run < n && t[i + run] === "`") run++;
	const closer = "`".repeat(run);
	const close = t.indexOf(closer, i + run);
	const unterminated = close === -1 || t.slice(i + run, close).includes("\n");
	if (unterminated) {
		return { emit: t.slice(i, i + run), next: i + run };
	}
	if (precededByBang && run === 1) {
		// Preserve `!`cmd`` verbatim so the shell-preprocessing pattern still matches.
		return { emit: t.slice(i, close + run), next: close + run };
	}
	return { emit: "", next: close + run };
}

function stripCode(text: string): string {
	const t = text.replace(/^[ ]{0,3}(`{3,})[\s\S]*?^[ ]{0,3}\1`*/gm, "");
	const out: string[] = [];
	let i = 0;
	while (i < t.length) {
		if (t[i] === "`") {
			const precededByBang = out.length > 0 && out[out.length - 1] === "!";
			const { emit, next } = scanBacktickSpan(t, i, precededByBang);
			if (emit) out.push(emit);
			i = next;
		} else {
			out.push(t[i]);
			i++;
		}
	}
	return out.join("");
}

function validateSharedSkill(skillDir: string): string[] {
	const skillMd = join(skillDir, "SKILL.md");
	if (!existsSync(skillMd)) {
		return [`${rel(skillDir)}: missing SKILL.md`];
	}
	let parsed: { meta: Frontmatter; body: string };
	try {
		parsed = parseMd(skillMd);
	} catch (e) {
		return [String(e)];
	}
	const { meta, body } = parsed;
	const errs: string[] = [];
	const badKeys = Object.keys(meta)
		.filter((k) => SHARED_SKILL_FORBIDDEN_KEYS.has(k))
		.sort();
	if (badKeys.length > 0) {
		errs.push(
			`${rel(skillMd)}: Claude-only frontmatter keys in shared skill: [${badKeys.map((k) => `'${k}'`).join(", ")}]`,
		);
	}
	for (const [pat, label] of SHARED_SKILL_RAW_BODY_PATTERNS) {
		if (pat.test(body))
			errs.push(`${rel(skillMd)}: body uses Claude-only feature: ${label}`);
	}
	const scan = stripCode(body);
	for (const [pat, label] of SHARED_SKILL_FORBIDDEN_BODY_PATTERNS) {
		if (pat.test(scan))
			errs.push(`${rel(skillMd)}: body uses Claude-only feature: ${label}`);
	}
	if (!meta.name) errs.push(`${rel(skillMd)}: missing \`name\` in frontmatter`);
	if (!meta.description)
		errs.push(`${rel(skillMd)}: missing \`description\` in frontmatter`);
	return errs;
}

function validateAllSharedSkills(names: string[]): void {
	const errors: string[] = [];
	for (const n of names)
		errors.push(...validateSharedSkill(join(SHARED_SKILLS, n)));
	if (errors.length > 0) {
		for (const e of errors) console.error(`ERROR: ${e}`);
		process.exit(1);
	}
}

function materializeSymlink(name: string): string | null {
	const link = join(CLAUDE_SKILLS, name);
	const target = join("..", "..", ".agents", "skills", name);
	let exists = false;
	let isSymlink = false;
	try {
		const st = lstatSync(link);
		exists = true;
		isSymlink = st.isSymbolicLink();
	} catch {
		// not present
	}
	if (isSymlink) {
		const current = readlinkSync(link);
		if (current === target) return null;
		unlinkSync(link);
	} else if (exists) {
		die(
			`ERROR: name collision - .claude/skills/${name} is a real directory (Claude-only skill) ` +
				`but .agents/skills/${name} also exists (shared skill). Resolve by removing one of them.`,
		);
	}
	symlinkSync(target, link);
	return `symlinked ${rel(link)}`;
}

function syncSkillSymlinks(): string[] {
	const changes: string[] = [];
	const sharedExisted = existsSync(SHARED_SKILLS);
	if (!sharedExisted) mkdirSync(SHARED_SKILLS, { recursive: true });
	mkdirSync(CLAUDE_SKILLS, { recursive: true });

	const wanted = readdirSync(SHARED_SKILLS, { withFileTypes: true })
		.filter((e) => e.isDirectory())
		.map((e) => e.name);
	const wantedSet = new Set(wanted);
	validateAllSharedSkills(wanted);

	for (const name of wanted) {
		const change = materializeSymlink(name);
		if (change) changes.push(change);
	}

	// If .agents/skills/ was missing entirely (sparse checkout, manual rm) and we
	// just created it empty, refuse to prune -- otherwise we'd silently delete every
	// Claude symlink. User-created symlinks elsewhere are unaffected either way.
	if (!sharedExisted && wanted.length === 0) return changes;

	for (const entry of readdirSync(CLAUDE_SKILLS, { withFileTypes: true })) {
		if (entry.isSymbolicLink() && !wantedSet.has(entry.name)) {
			const p = join(CLAUDE_SKILLS, entry.name);
			unlinkSync(p);
			changes.push(`pruned dangling ${rel(p)}`);
		}
	}
	return changes;
}

function syncAgents(): string[] {
	const changes: string[] = [];
	mkdirSync(CODEX_AGENTS, { recursive: true });
	mkdirSync(CLAUDE_AGENTS, { recursive: true });

	const wanted = new Set<string>();
	const mdFiles = readdirSync(CLAUDE_AGENTS, { withFileTypes: true })
		.filter((e) => e.isFile() && e.name.endsWith(".md"))
		.map((e) => e.name);
	for (const mdName of mdFiles) {
		const mdPath = join(CLAUDE_AGENTS, mdName);
		const { meta, body } = parseMd(mdPath);
		const tomlName = `${mdName.slice(0, -3)}.toml`;
		const tomlPath = join(CODEX_AGENTS, tomlName);
		const fresh = renderToml(meta, body, rel(mdPath));
		const current = existsSync(tomlPath)
			? readFileSync(tomlPath, "utf-8")
			: null;
		if (current !== fresh) {
			writeFileSync(tomlPath, fresh, "utf-8");
			changes.push(`wrote ${rel(tomlPath)}`);
		}
		wanted.add(tomlName);
	}

	for (const entry of readdirSync(CODEX_AGENTS, { withFileTypes: true })) {
		if (
			entry.isFile() &&
			entry.name.endsWith(".toml") &&
			!wanted.has(entry.name)
		) {
			const p = join(CODEX_AGENTS, entry.name);
			unlinkSync(p);
			changes.push(`pruned orphan ${rel(p)}`);
		}
	}
	return changes;
}

function findClaudeMds(dir: string, acc: string[]): void {
	for (const entry of readdirSync(dir, { withFileTypes: true })) {
		if (entry.isDirectory()) {
			if (MIRROR_SKIP_DIRS.has(entry.name)) continue;
			findClaudeMds(join(dir, entry.name), acc);
		} else if (entry.isFile() && entry.name === "CLAUDE.md") {
			acc.push(join(dir, entry.name));
		}
	}
}

function mirrorAgentsMd(): string[] {
	// Every directory holding a CLAUDE.md gets a sibling AGENTS.md symlink so
	// Codex (which reads AGENTS.md) sees the same doc Claude reads.
	const changes: string[] = [];
	const claudeMds: string[] = [];
	findClaudeMds(REPO, claudeMds);

	for (const claudeMd of claudeMds) {
		const agentsMd = join(dirname(claudeMd), "AGENTS.md");
		let isSymlink = false;
		let exists = false;
		try {
			const st = lstatSync(agentsMd);
			exists = true;
			isSymlink = st.isSymbolicLink();
		} catch {
			// not present
		}
		if (isSymlink) {
			if (readlinkSync(agentsMd) === "CLAUDE.md") continue;
			unlinkSync(agentsMd);
		} else if (exists) {
			// A drifted hand-written AGENTS.md -- replace it with the symlink.
			unlinkSync(agentsMd);
		}
		symlinkSync("CLAUDE.md", agentsMd);
		changes.push(`symlinked ${rel(agentsMd)} -> CLAUDE.md`);
	}
	return changes;
}

function main(): number {
	const check = process.argv.includes("--check");
	const changes = [
		...syncSkillSymlinks(),
		...syncAgents(),
		...mirrorAgentsMd(),
	];
	for (const c of changes) console.log(c);
	if (check && changes.length > 0) {
		console.error(
			"sync-agent-config introduced changes; stage them and commit again.",
		);
		return 1;
	}
	return 0;
}

process.exit(main());
