---
name: cog
description: "Cognitive model CLI for LLM coding agents. Use when the agent needs to record, query, or reason about codebase knowledge: contracts, invariants, dependencies, fragility points. Activated by: cog assert/retract/query/impact/trace/depend/verify/export/stats/index/delete-entity/branch commands."
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
`Type` (`::PascalCase` ending). Created implicitly by `assert`.

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

### Writing

| Command | Purpose |
|---------|---------|
| `cog assert <entity> --kind <kind> --claim "<text>" --grounds "<source>" [--depends-on <id>]` | Record a knowledge claim |
| `cog depend <entity_a> --on <entity_b> --kind contains\|calls\|uses` | Link two entities structurally |
| `cog retract <id> --reason "<why>"` | Retract a claim (cascades to dependents) |
| `cog delete-entity <qualified_name>` | Delete entity + all assertions, evidence, relations, changelog. Irreversible. |

### Reading

| Command | Purpose |
|---------|---------|
| `cog query <entity> [--all]` | Show active (or all) assertions for an entity |
| `cog impact <entity>` | BFS downstream impact — what changes if this entity changes? |
| `cog trace <entity>` | Full picture: assertions, evidence, depends-on tree, entity relations |
| `cog index` | List all entities sorted by active assertion count |
| `cog stats` | Model statistics (entity/assertion/relation counts) |
| `cog verify [--scope <entity>] [--clean]` | Structural consistency check: isolated entities, missing evidence, dangling deps |
| `cog export [--format json\|toml\|dot]` | Export model in machine-readable format |

### Branching

| Command | Purpose |
|---------|---------|
| `cog branch create [--name <name>]` | Snapshot current model into a branch |
| `cog branch list` | List all branches |
| `cog branch switch <name>` | Activate branch for editing |
| `cog branch switch _main` | Return to main (saves branch state) |
| `cog branch diff <name> [--item <N>]` | Diff main vs branch changes |
| `cog branch merge <name> [--apply-all\|--apply <N>\|--reject <N>]` | Merge branch changes into main |
| `cog branch drop <name>` | Delete branch file |

## Assertion Kinds

| Kind | When | Example |
|------|------|---------|
| `contract` | What the code promises callers | "Returns None on invalid input, never panics" |
| `intent` | Why the code exists / design goal | "Retry logic because upstream is flaky" |
| `invariant` | What must always be true | "Pool size never exceeds MAX_CONNECTIONS" |
| `fragility` | What could break and why | "Depends on undocumented header format from v2 API" |
| `correction` | What was wrong and how it was fixed | "Off-by-one in bounds check fixed in abc1234" |

**Do NOT use** `structure` or `behavior` — they are not valid kinds.

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

## When to Use Which Guide

| Situation | Guide |
|-----------|-------|
| Starting a new project (no code yet) | [WORKFLOWS.md — From Scratch](WORKFLOWS.md#starting-a-new-project-from-scratch) |
| Understanding unfamiliar existing code | [WORKFLOWS.md — Retrofitting](WORKFLOWS.md#after-reading-unfamiliar-code-retrofitting) |
| Planning a risky refactor | [WORKFLOWS.md — Risky Refactor](WORKFLOWS.md#before-a-risky-refactor) |
| Found a subtle trap in the code | [WORKFLOWS.md — Subtle Trap](WORKFLOWS.md#when-you-find-a-subtle-trap) |
| Retracting outdated knowledge | [WORKFLOWS.md — Retracting](WORKFLOWS.md#when-retracting-outdated-knowledge) |
| Learned from a mistake | [WORKFLOWS.md — Correction](WORKFLOWS.md#when-you-learn-from-a-mistake) |
| Linking entities during exploration | [WORKFLOWS.md — Linking Entities](WORKFLOWS.md#linking-entities-during-architecture-exploration) |
| Grounds lifecycle (plan → code migration) | [WORKFLOWS.md — Progressive Grounding](WORKFLOWS.md#progressive-grounding-lifecycle) |
| Branch / merge / sandbox experiments | [BRANCHING.md](BRANCHING.md) |
| Modelling best practices & anti-patterns | [BEST_PRACTICES.md](BEST_PRACTICES.md) |
