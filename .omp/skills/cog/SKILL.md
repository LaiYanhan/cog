---
name: cog
description: "Cognitive model CLI for LLM coding agents. Use when the agent needs to record, query, or reason about codebase knowledge: contracts, invariants, dependencies, fragility points. Activated by: cog assert/retract/query/impact/trace/depend/verify/export/stats/index/delete-entity/branch commands."
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

**Delete an entity and all its data (cascading):**
```bash
cog delete-entity <qualified_name>
# Removes: entity, all assertions + evidence, relations, changelog entries
# Fails if entity does not exist (exit code 1)
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
**Why does this entity exist? (full dependency chain + entity relations):**
```bash
cog trace <entity>
# Shows: assertions with evidence, entity relation graph (out/in)
```
**List all entities with active assertion counts (sorted by importance):**
```bash
cog index
# Output: - entity_name [N] where N = active assertion count
**Model statistics:**
```bash
cog stats
```
**Structural consistency checks (3 dimensions):**
```bash
cog verify [--scope <entity>] [--clean]
# Checks: isolated entities, missing evidence, dangling dependencies (retracted/uncertain)
# --clean: auto-delete isolated entities found during check
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

### Latent space reasoning (branching)

```
cog branch create [--name <name>]   # Snapshot current model (name auto-generated if omitted)
cog branch list                      # List all branches (excludes _main_backup)
cog branch switch <name>             # Activate branch for editing
cog branch switch _main              # Return to main (saves branch state, clears active marker)
cog branch diff <name>               # Diff main vs branch changes (items ordered by ID, stable)
cog branch diff <name> --item <N>    # Inspect specific change in detail
cog branch merge <name>              # Show merge plan (pending items)
cog branch merge <name> --apply-all  # Apply all changes to main
cog branch merge <name> --apply <N>  # Apply one change
cog branch merge <name> --reject <N> # Reject one change
cog branch drop <name>               # Delete branch file
```

**Workflow**:
```
# 1. Snapshot current model
cog branch create --name my-plan

# 2. Switch to the sandbox (all subsequent commands affect only the copy)
cog branch switch my-plan

# 3. Freely experiment — assert/retract/depend without risk
cog assert new::feature --kind intent --claim "planned feature" --grounds "plan:design-doc"
cog retract d6e3a49f --reason "outdated assumption"

# 4. Return to main, diff to see what changed, then merge
cog branch switch _main
cog branch diff my-plan
cog branch merge my-plan --apply-all

# 5. Clean up
cog branch drop my-plan
```

**Diff semantics**: compares main (base) vs branch file. Each addition/removal/modification is an indexed item. Items within each category are sorted by ID — indexing is stable across processes, so `--item <N>` produces consistent results.

**Merge semantics**: applies changes from branch to main:
- **Entities**: inserted with original UUID — cross-references stay valid
- **Assertions**: inserted with original UUID; evidence and dependency relations are preserved
- **Removals**: entity removals skipped (avoids broken references); assertion removals become retractions
- **Evidence/relations**: verified against existing IDs; skipped items are reported in merge summary
- **Merge reports**: `applied N, skipped M` — check for unexpected skips

**Reserved branch names**: `_main` (switch back target) and `_main_backup` (internal) cannot be created as branches.

**Use case**: Before writing code, create a branch, assert planned invariants and contracts, diff against the real model, then merge only validated knowledge. Keeps the cognitive model clean while allowing speculative reasoning.
---

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
- **`trace` is the full picture**. It shows assertions (active-only), their evidence, the depends-on tree, and entity relations. Use it when you need to deeply understand one entity.
- **All read commands filter retracted by default**. `query`, `impact`, and `trace` only show active assertions. This keeps output clean for working state.
- **`verify` is a confidence check, not a substitute for reading code**. It tells you the model is structurally consistent (no orphan entities, no bare assertions, no dangling deps). Run it after bulk changes to the model.
- **All display uses short IDs**. Verify details, impact reports, trace output — everything uses 8-char IDs. Full UUIDs only appear in `assert` output for reference.
- **`verify --clean` removes isolated entities automatically**. Use this after test runs to clean up artifacts without manual `delete-entity` calls.
- **`delete-entity` removes the entity and ALL its data** — assertions, evidence, relations, changelog. Undo is not possible. The command reports the counts of removed data before deleting.
- **`verify` detects stale entities**. An entity with zero active assertions and zero relations is flagged as isolated. Clean up test artifacts before committing your model.
- **Grounds should point to current code**. When you refactor, update the grounds of affected assertions. Stale line references erode trust in the model.
- **Branch diffs are stable across processes**. Items within each category are sorted by ID, so `--item <N>` produces consistent results across separate invocations.
- **Branch merge preserves UUIDs**. Assertion and evidence IDs are kept identical to the branch copy. This means cross-references (dependencies, assertion relations) remain valid after merge.
- **Check merge reports for skips**. When merging, `applied N, skipped M` tells you if any evidence or relation items couldn't be matched (usually because the referenced assertion wasn't found). A non-zero skip count needs investigation.

## Modeling Anti-Patterns

These patterns produce a fragile or noisy model:
- Asserting on the wrong entity (e.g. store behavior asserted on types)
- Creating test assertions and forgetting to retract them; use `verify --clean` or `delete-entity` to remove stale test entities
- Using `fragility` as a permanent warning instead of retracting after fix
- Recording what code does (that's the code's job) instead of why it does it or what could break
- Leaving test entities in the model after experimentation — run `verify [--clean]` as part of your closeout checklist

## Database Location

Default: `.cog/cog.db` in current directory. Override with `--db <path>` or `COG_DB` env var.