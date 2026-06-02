---
name: cog
description: "Cognitive model CLI for LLM coding agents. Use when the agent needs to record, query, or reason about codebase knowledge: contracts, invariants, dependencies, fragility points. Activated by: cog assert/retract/query/impact/trace/depend/verify/export/stats/index commands."
---

# cog — Cognitive Model for Coding Agents

`cog` is a CLI tool that maintains a **cognitive model** of your codebase in a local SQLite database (`.cog/cog.db`, gitignored). It lets you record what you know about code, track dependencies between those knowledge claims, and reason about impact when things change.

## Core Concepts

### Entities
A qualified name for a code construct: `module::function`, `crate::module::Type`. Kinds are inferred: `Module` (contains `::` but not `::camelCase`), `Function` (`::camelCase` ending), `Type` (`::PascalCase` ending). Created implicitly by `assert`.

### Assertions
A knowledge claim about an entity. Each assertion has:
- **kind**: `contract` | `intent` | `invariant` | `fragility` | `correction`
- **claim**: free-text description of what you know
- **grounds**: evidence source (e.g. `code:src/foo.rs:10-20`, `manual:review`, `test:bar_test`)
- **status**: `active` → `retracted` (or `uncertain` via cascade)

### Assertion Dependencies
An assertion can depend on another via `--depends-on`. If the base is retracted, dependents cascade to `uncertain` unless they have other active grounds. This is TMS-style truth maintenance.

### Entity Relations
`depend --kind contains|calls|uses` links two entities structurally. Used by `trace` to follow cross-entity dependency chains.

### IDs
Output shows **short IDs** (first 8 chars of UUID). All commands accepting IDs (`retract`, `assert --depends-on`) resolve short IDs automatically — pass the 8-char form you see in output.

## Commands

### Writing to the model

**Record a knowledge claim:**
```bash
cog assert <entity> --kind <kind> --claim "<claim>" --grounds "<grounds>"
# Optional: --depends-on <id> to link to a base assertion
```

**Link two entities:**
```bash
cog depend <entity_a> --on <entity_b> --kind contains|calls|uses
```

**Retract a claim (cascades to dependents):**
```bash
cog retract <id> --reason "<why>"
```

### Reading from the model
**Show active assertions for an entity (default filters out retracted):**
```bash
cog query <entity>
cog query <entity> --all  # include retracted
```
**What happens if this entity changes? (BFS downstream impact):**
```bash
cog impact <entity>
```
**Why does this entity exist? (full dependency chain):**
```bash
cog trace <entity>
```
**List all entities with active assertion counts (sorted by importance):**
```bash
cog index
# Output: - entity_name [N] where N = active assertion count
```
**Model statistics:**
```bash
cog stats
```
**Structural consistency checks:**
```bash
cog verify [--scope <entity>]
```
**Export model (json/toml/dot):**
```bash
cog export [--format json|toml|dot]
```

## Assertion Kinds — When to Use Each

| Kind | When | Example |
|---|---|---|
| `contract` | What the code promises to do | "Returns None on invalid input, never panics" |
| `intent` | Why the code exists / design goal | "Retry logic exists because upstream is flaky" |
| `invariant` | What must always be true | "Pool size never exceeds MAX_CONNECTIONS" |
| `fragility` | What could break and why | "Depends on undocumented header format from v2 API" |
| `correction` | What was wrong and how it was fixed | "Off-by-one in bounds check fixed in abc123" |

**Do NOT use** `structure` or `behavior` — they are not valid kinds.

## Entity Relation Kinds

| Kind | Meaning | Example |
|---|---|---|
| `contains` | A is a parent scope of B | `cog::model` contains `cog::model::store` |
| `calls` | A invokes B at runtime | `cog::command::retract` calls `cog::model::graph::retract` |
| `uses` | A depends on B structurally | `cog::command::verify` uses `cog::model::store` |

**Do NOT use** `depends_on` for entity relations — that is an assertion-level concept (`--depends-on`).

### Impact traversal direction
`cog impact` traverses entity relations respecting their semantics:
- **`contains`**: forward — parent change impacts children (`cog::model` contains `cog::model::store`)
- **`uses`/`calls`**: reverse — dependency change impacts dependents (`graph uses store` → `impact store` finds `graph`)

When recording relations, think: "if X changes, what does Y mean for impact?" Then set direction accordingly.

## Typical Workflows

### After reading unfamiliar code
```bash
cog assert auth::login --kind contract --claim "Returns Ok(token) on valid credentials, Err on invalid" --grounds "code:src/auth.rs:45-67"
cog assert auth::login --kind invariant --claim "Password is zeroed from memory before returning" --grounds "code:src/auth.rs:62"
```

### When you find a subtle trap
```bash
cog assert cache::invalidate --kind fragility --claim "Race condition: concurrent invalidation + get-or-fill can return stale data" --grounds "manual:code-review"
```

### Linking entities during architecture exploration
```bash
cog depend cog::model::graph --on cog::model::store --kind uses
cog depend cog::model --on cog::model::graph --kind contains
```

### Before a risky refactor
```bash
cog impact cog::model::store
# Review affected assertions — check each one
cog trace cog::model::store
# Full dependency tree — understand why things depend on this
cog verify
# Structural consistency check across the model
```

### When retracting outdated knowledge
```bash
cog retract <short_id> --reason "assumption invalidated by refactor"
# Cascade marks dependent assertions as uncertain
```

### When you learn from a mistake
```bash
cog assert cog --kind fragility --claim "Output format changes must be verified against all ID-consuming input paths" --grounds "incident:short-id-roundtrip-gap"
cog assert cog --kind correction --claim "Added resolve_assertion_id for short ID prefix matching" --grounds "code:src/model/store.rs:274-298"
```

## Design Principles

- **Structural, not semantic**: `verify` checks structural consistency (orphan assertions, missing grounds), not whether claims are semantically correct. The LLM judges meaning.
- **Trace, don't diagnose**: `trace` returns the full dependency chain for the LLM to reason about. No automatic symptom matching.
- **Plain text output**: All output is plain text. No colors, no JSON in default output. Use `export --format json` for machine-readable snapshots.
- **Single-file DB**: `.cog/cog.db` with WAL mode and foreign keys. Gitignored — each developer/agent maintains their own model.
- **Short IDs everywhere**: Display uses 8-char IDs. Input accepts 8-char or full UUIDs.

## Lessons from Practice

- **`query` defaults to active-only**. Use `--all` only when investigating retraction history. Most workflow needs active knowledge.
- **`index` is your first stop**. The assertion count tells you which entities the model knows most about. High-count entities are architectural anchors.
- **Link assertions with `--depends-on`**. Individual assertions are facts; `--depends-on` chains are reasoning. Without dependency chains, `trace` has nothing to traverse.
- **Record cross-cutting invariants on the parent entity**. If a rule applies to multiple modules (e.g. "all IDs resolve via prefix match"), assert it on the root entity (`cog`) not on each leaf.
- **`impact` combines both directions**. It shows assertions from the entity AND all reachable neighbors. Use it before refactoring to understand blast radius.
- **Retract fragility assertions after fixing**. Once you fix the issue, retract the fragility with reason="fixed: ...". Don't leave stale warnings in the model.
## Database Location

Default: `.cog/cog.db` in current directory. Override with `--db <path>` or `COG_DB` env var.
