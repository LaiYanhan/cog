---
name: cog
description: "Cognitive model CLI for LLM coding agents. Use when the agent needs to record, query, or reason about codebase knowledge: contracts, invariants, dependencies, fragility points. Activated by: cog assert/retract/query/impact/trace/depend/verify/export/stats/index/delete-entity/sync/experiment/backup/recover/next commands."
---

# cog — Cognitive Model for Coding Agents

`cog` maintains a persistent cognitive model of your codebase in a local
SQLite database (`.cog/cog.db`, gitignored). Record what you know about code
— contracts, invariants, intent, fragility — track dependencies between
those claims, and reason about impact when things change.


## Core Concepts

**Entity** — a qualified name for a code construct:
`module::function`, `crate::module::Type`. Kinds are inferred:
`Module` (contains `::` but not `::camelCase`), `Function` (`::camelCase` ending),
`Type` (`::PascalCase` ending). Created automatically by `sync` or implicitly by `assert`.

**Assertion** — a knowledge claim about an entity. Each has a `kind`
(contract / intent / invariant / fragility / correction), a `claim`
(free text), `grounds` (evidence source), and a `status`
(active → retracted, or uncertain via cascade).

**Assertion Dependency** — one assertion can depend on another via
`--depends-on`. If the base is retracted, dependents cascade to `uncertain`
unless they have other active grounds (TMS-style truth maintenance).

**Entity Relation** — `depend --kind contains|calls|uses` links two
entities structurally. Drives `trace` and `impact` traversal.

**ID** — output uses short IDs (first 8 chars of UUID). All commands
accepting IDs resolve short IDs automatically.

## Design Principles

- **Structural, not semantic**: `verify` checks structural consistency,
  not whether claims are semantically correct. The LLM judges meaning.
- **Trace, don't diagnose**: `trace` returns the full dependency chain
  for the LLM to reason about. No automatic symptom matching.
- **Plain text output**: No colours, no JSON in default output.
  Use `export --format json` for machine-readable snapshots.
- **Single-file DB**: `.cog/cog.db` with WAL mode and foreign keys.
  Gitignored — each agent maintains their own model.
- **Short IDs everywhere**: 8-char display; 8-char or full UUID input.

## Command Reference

### Scanning

| Command | Purpose |
|---------|---------|
| `cog sync [--init] [--dry-run] [--lang python,rust,...]` | Idempotent full scan with tree-sitter. `--init` creates `.cog/` at CWD before syncing. Creates entities for definitions (functions, classes, structs, methods), directory modules, import relationships, and cleans up stale entities (skipping those with assertions). All auto-generated entities carry origin `Scan`. Replaces the removed `cog init`. |

### Writing

| Command | Purpose |
|---------|---------|
| `cog assert <entity> --kind <kind> --claim "<text>" --grounds "<source>" [--depends-on <id>]` | Record a knowledge claim. Returns the entity's full assert state (new assertion highlighted). |
| `cog depend <entity_a> --on <entity_b> --kind contains\|calls\|uses` | Link two entities structurally. Returns the entity's full relation set. |
| `cog retract <id> --reason "<why>"` | Retract a claim. Cascades to dependents. Returns entity's remaining assertions with status marks. |
| `cog delete-entity <qualified_name>` | Delete entity + all assertions, evidence, relations, changelog. Irreversible. |

### Reading

| Command | Purpose |
|---------|---------|
| `cog query <entity> [--all] [--compact]` | Show active assertions for an entity. `--all` includes retracted. `--compact` emits one assertion per line for embedded use. |
| `cog impact <entity>` | BFS downstream impact with risk assessment (HIGH/MEDIUM/LOW) and per-entity covered/blind markers |
| `cog trace <entity>` | Full picture: assertions, evidence, depends-on tree, entity relations |
| `cog index [--uncovered] [--verbose] [--kind <k>] [--prefix <p>]` | Coverage summary by default; `--verbose` restores full listing. `--uncovered` shows only unasserted entities. |
| `cog next` | Single entry point: model summary, active experiments, suggestions, stagnation warnings |
| `cog stats` | Detailed model statistics (entity/assertion/relation counts) |
| `cog verify [--scope <entity>] [--clean] [--scan]` | Structural consistency check. `--scan` also compares the model against the actual codebase, reporting unmodeled and stale entities. |
| `cog export [--format json\|toml\|dot]` | Export model in machine-readable format |

### Experiment & Backup

| Command | Purpose |
|---------|---------|
| `cog experiment try <entity> --kind <k> --claim "<t>" --grounds "<s>" [--desc "<d>"] [--depends-on <id>]` | Quick one-liner: start + hypothesize + evaluate. Covers 80% of scenarios. |
| `cog experiment start <entity> [--description "<desc>"] [--max-nodes <n>]` | Start hypothesis experiment on in-memory snapshot (default 500 nodes) |
| `cog experiment hypothesize <id> --entity <entity> --kind <k> --claim "<t>" --grounds "<s>"` | Inject a hypothetical assertion |
| `cog experiment hypothetical-relation --id <id> --from <a> --to <b> --kind contains\|calls\|uses` | Inject a hypothetical entity relation |
| `cog experiment hypothetical-delete --id <id> --entity <entity>` | Inject a hypothetical entity deletion |
| `cog experiment evaluate <id>` | Evaluate impact of staged operations — returns risk, contradictions, blind entities |
| `cog experiment report <id>` | Show full experiment report |
| `cog experiment commit <id>` | Replay staged operations to real model |
| `cog experiment discard <id>` | Discard experiment |
| `cog experiment list` | List all experiments (drafts vs saved) |
| `cog experiment save <id>` | Mark experiment as a saved checkpoint |
| `cog experiment load <id>` | Load a saved experiment |
| `cog backup create --name <name>` | Full DB snapshot (VACUUM INTO) |
| `cog backup list` | List all backups |
| `cog backup restore <name>` | Restore backup as active model |
| `cog backup drop <name>` | Delete backup file |

## Assertion Kinds

| Kind | When | Example |
|------|------|---------|
| `contract` | What the code promises callers | "Returns None on invalid input, never panics" |
| `intent` | Why the code exists / design goal | "Retry logic because upstream is flaky" |
| `invariant` | What must always be true | "Pool size never exceeds MAX_CONNECTIONS" |
| `fragility` | What could break and why | "Depends on undocumented header format from v2 API" |
| `correction` | What was wrong and how it was fixed | "Off-by-one in bounds check fixed in abc1234" |

**Do NOT use** `structure`, `behavior`, or `safety` — they are not valid kinds.

## Entity Relation Kinds

| Kind | Meaning | Impact direction | Example |
|------|---------|-----------------|---------|
| `contains` | A parent-scope of B | Forward — parent change impacts children | `model` contains `model::store` |
| `calls` | A invokes B at runtime | Reverse — callee change impacts callers | `retract_cmd` calls `graph::retract` |
| `uses` | A depends on B structurally | Reverse — dependency change impacts dependents | `verify` uses `store` |

**Do NOT use** `depends_on` for entity relations — that is an assertion-level
concept (`--depends-on`).

## Database Location

Default: `.cog/cog.db` in current directory.
Override: `--db <path>` or `COG_DB` environment variable.

---

## Further Reading

| Guide | Content |
|-------|---------|
| [WORKFLOWS.md](WORKFLOWS.md) | Structural scan + semantic deepening, progressive grounding lifecycle |
| [BEST_PRACTICES.md](BEST_PRACTICES.md) | Anti-patterns, assertion/relation kind reference |
| [BRANCHING.md](BRANCHING.md) | Experiment hypothesis testing, backup snapshots |
