# cog

A cognitive model for LLM coding agents. Scans codebases with tree-sitter to build a structural model (entities + containment hierarchy), then records knowledge claims (assertions) about code — contracts, invariants, fragility points — tracking dependencies so agents can reason about what they know and what might break.

## Install

```sh
cargo build --release
```

The binary is `./target/release/cog`.

## Quickstart
```sh
# Auto-scan codebase structure (entities + containment)
cog init .

# Record what you've learned
cog assert auth::login --kind contract \
    --claim "Returns Ok(token) on valid credentials, Err on invalid" \
    --grounds "code:auth::login"

# Query what you know
cog query auth::login

# Check for stale/unmodeled code
cog verify --scan
```

## Commands

### Scanning

```sh
# Auto-scan a codebase — creates entities + contains relations for all definitions
cog init [PATH]
cog init .                      # scan current directory
cog init src/                   # scan a subtree
cog init . --lang rust          # filter to one language
cog init . --depth 3            # limit directory depth
cog init . --dry-run            # preview without writing

# Detect drift between model and actual code
cog verify --scan               # list stale (removed) and unmodeled (new) entities
cog verify --scan --clean       # also delete stale entities from model
```

Supported languages: Python, Rust, JavaScript/TypeScript, C, Go, Java.

### Writing to the model

```sh
# Declare an assertion about an entity
cog assert <entity> --kind <kind> --claim "<claim>" --grounds "<source>"

# Optional: --depends-on <id> to chain reasoning

# Link two entities structurally
cog depend <entity_a> --on <entity_b> --kind contains|calls|uses

# Retract an assertion (cascades to dependents)
cog retract <id> --reason "<why>"

# Delete an entity and ALL its data (cascading)
cog delete-entity <qualified_name>
```

### Reading from the model

```sh
# Query assertions for an entity (active only by default)
cog query <entity>
cog query <entity> --all          # include retracted

# Show downstream impact (BFS through entity relations)
cog impact <entity>

# Full dependency chain + entity relations
cog trace <entity>

# List all entities with assertion counts
cog index
cog index --kind function       # filter to functions
cog index --origin scan         # filter to auto-scanned entities
cog index --prefix auth::       # filter by qualified name prefix
# Structural consistency checks
cog verify --scan                      # detect stale/unmodeled code
cog verify --scan --scan-path src/     # scan a specific directory
cog verify
cog verify --scope <entity>            # scope to a subtree
cog verify --clean                     # also delete isolated entities

# Model statistics
cog stats

# Export full model (json / toml / dot)
cog export --format json
cog export --format toml
cog export --format dot
```

### Branch workflow (speculative modeling)

Create a sandbox copy of the model for experimentation, then merge only validated knowledge.

```sh
# Snapshot current model
cog branch create --name my-plan

# Switch to sandbox (all subsequent commands affect only the copy)
cog branch switch my-plan

# Freely experiment — assert/retract/depend without risk
cog assert new::feature --kind intent --claim "planned feature" --grounds "plan:design-doc"
cog retract d6e3a49f --reason "outdated assumption"

# Return to main, diff to see what changed, then merge
cog branch switch _main
cog branch diff my-plan
cog branch merge my-plan --apply-all

# Clean up
cog branch drop my-plan
```

Branch commands:

| Command | Description |
|---|---|
| `cog branch create [--name <name>]` | Snapshot current model (auto-named if omitted) |
| `cog branch list` | List all branches |
| `cog branch switch <name>` | Activate a branch for editing |
| `cog branch switch _main` | Return to main (saves branch state) |
| `cog branch diff <name>` | Changes since branch creation |
| `cog branch diff <name> --item <N>` | Inspect a specific change in detail |
| `cog branch merge <name>` | Show merge plan |
| `cog branch merge <name> --apply-all` | Apply all changes to main |
| `cog branch merge <name> --apply <N>` | Apply one change |
| `cog branch merge <name> --reject <N>` | Reject one change |
| `cog branch drop <name>` | Delete branch file |

Merge semantics: UUIDs are preserved so cross-references stay valid. Entity removals are skipped to avoid broken references. Items verified against existing IDs; skips reported in summary.

## Assertion Kinds

| Kind | When | Example |
|---|---|---|
| `contract` | What the code promises | "Returns None on invalid input, never panics" |
| `intent` | Why the code exists | "Retry logic exists because upstream is flaky" |
| `invariant` | What must always be true | "Pool size never exceeds MAX_CONNECTIONS" |
| `fragility` | What could break and why | "Depends on undocumented header format from v2 API" |
| `correction` | What was wrong and how fixed | "Off-by-one in bounds check fixed in abc123" |

Do **not** use `structure`, `behavior`, or `safety` — these are not valid kinds.

## Entity Relation Kinds

| Kind | Meaning | Example |
|---|---|---|
| `contains` | Parent scope | `cog::model` contains `cog::model::store` |
| `calls` | Runtime invocation | `graph::retract` calls `store::execute` |
| `uses` | Structural dependency | `command::verify` uses `model::store` |

Do **not** use `depends_on` for entity relations — that is an assertion-level concept.

## Concepts

- **Entity** — a code construct (module, function, type) identified by qualified name
- **Assertion** — a knowledge claim about an entity with kind, claim text, and grounds
- **Evidence** — source material backing an assertion (code reference, manual review, test)
- **Dependency** — assertion-level `depends-on` chain; when a base assertion is retracted, dependents cascade to `uncertain` (TMS-style truth maintenance)
- **Retraction** — marks an assertion as retracted and cascades `uncertain` to dependent assertions that have no other active support
- **Branch** — snapshot-based sandbox for speculative reasoning; merge preserves UUIDs
- **Scan** — tree-sitter based code structure analysis. `cog init` walks directories, parses files into ASTs, extracts functions/classes/methods/types, and creates entities + `contains` relations. All auto-created entities are grounded `auto:scan`, clearly separated from LLM-authored knowledge.

## Ids

Output shows **short IDs** (first 8 chars of UUID). All commands accepting IDs (`retract`, `assert --depends-on`) resolve short IDs automatically — pass the 8-char form from output.

## Storage

Single SQLite file at `.cog/cog.db` in WAL mode with foreign keys enabled. Override with `--db <path>` or `COG_DB` env var. Gitignore `.cog/` for private use or commit to share model state.
