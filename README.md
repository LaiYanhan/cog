# cog

A cognitive model for LLM coding agents. Scans codebases with tree-sitter to build a structural model (entities + containment hierarchy), then records knowledge claims (assertions) about code — contracts, invariants, fragility points — tracking dependencies so agents can reason about what they know and what might break.

## Install

```sh
cargo build --release
```

The binary is `./target/release/cog`.

## Quickstart
```sh
# Auto-scan codebase structure (entities + relations, idempotent)
cog sync --init              # first time: creates .cog/ and scans

# Check suggested next actions
cog next

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
# Idempotent full scan — creates entities + relations, cleans stale
cog sync                     # scan (requires existing .cog/)
cog sync --init              # first time: creates .cog/ then scans
cog sync --lang python,rust  # filter to specific languages
cog sync --dry-run           # preview without writing

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

# Migrate assertions from a design-phase entity onto the real scan entity
# (reconciles names like myapp::lexer::Lexer with src::lexer::Lexer)
cog migrate <from_entity> <to_entity>
```

### Reading from the model

```sh
# Query assertions for an entity (active only by default)
cog query <entity>
cog query <entity> --all          # include retracted
cog query <entity> --compact      # compact mode for embedded use

# Show downstream impact (BFS through entity relations)
cog impact <entity>

# Full dependency chain + entity relations
cog trace <entity>

# Coverage summary (default), or full listing with --verbose
cog index
cog index --kind function       # filter to functions
cog index --uncovered           # only entities without assertions
cog index --prefix auth::       # filter by qualified name prefix
cog index --verbose             # full listing

# Structural consistency checks
cog verify --scan                      # detect stale/unmodeled code
cog verify --scan --scan-path src/     # scan a specific directory
cog verify
cog verify --scope <entity>            # scope to a subtree
cog verify --clean                     # also delete isolated entities
# Recover assertions that went uncertain after a retraction cascade
cog recover                  # preview which uncertain assertions can be restored
cog recover --apply          # restore them to active

# Model statistics
cog stats

# Export full model (json / toml / dot)
cog export --format json
cog export --format toml
cog export --format dot
```

### Workflow state machine

cog tracks workflow state in `.cog/workflow_state.json`. Commands implicitly
transition state (e.g. `sync` detecting code drift moves to PostChange), and
`cog next` reads the current state plus model data to suggest the next action.

```sh
# Show suggested next actions based on current state and model data
cog next
```

State transitions are automatic — `sync` detects code changes, `retract`
triggers Debugging, `assert` moves from FreshScan/PostChange to Exploring.
No manual `start-change`/`finish-change`/`abort-change` commands.

Typical cycle: `cog sync --init` (first time), then `cog sync` → `cog next` → make code changes →
`cog verify --scan` → `cog assert` (record what you learned).

### Experiment workflow (hypothesis testing)

Experiments test "what if" scenarios on a lightweight in-memory snapshot
without copying the entire database.

```sh
# Quick one-liner: start + hypothesize + evaluate in one command
cog experiment try <entity> --kind correction \
    --claim "change X to Y" --grounds "code:<entity>" \
    --desc "what if we change X?" [--depends-on <id>]

# Or step-by-step for complex scenarios:
cog experiment start auth::login --description "what if login takes 3 params?"
cog experiment hypothesize <id> --entity auth::login \
    --kind contract --claim "now accepts (user, pass, rate_limit)" \
    --grounds "hypothesis:rate-limit-feature"
cog experiment evaluate <id>

# Commit (replay staged ops to real DB) or discard
cog experiment commit <id>
cog experiment discard <id>

# List all experiments, show detailed report
cog experiment list
cog experiment report <id>
```

Experiment commands:

| Command | Description |
|---|---|
| `cog experiment try <entity> --kind ...` | Start + hypothesize + evaluate in one step |
| `cog experiment start <entity> [--description "<desc>"]` | Start hypothesis experiment |
| `cog experiment hypothesize <id> --entity <entity> --kind ...` | Inject hypothetical assertion |
| `cog experiment hypothetical-relation --id <id> --from <a> --to <b> --kind ...` | Inject hypothetical entity relation |
| `cog experiment hypothetical-delete --id <id> --entity <entity>` | Inject hypothetical entity deletion |
| `cog experiment evaluate <id>` | Evaluate impact of staged operations |
| `cog experiment report <id>` | Show full experiment report |
| `cog experiment commit <id>` | Replay staged ops to real model |
| `cog experiment discard <id>` | Discard experiment |
| `cog experiment list` | List all experiments |
| `cog experiment save <id>` / `cog experiment load <id>` | Save as checkpoint / load saved |


### Backup workflow (full model snapshots)

For large-scale refactors, create a full DB snapshot as a safety net.

```sh
# Snapshot before a major change
cog backup create --name "before-refactor"

# List backups
cog backup list

# Restore if needed
cog backup restore "before-refactor"

# Clean up
cog backup drop "before-refactor"
```

Backup commands:

| Command | Description |
|---|---|
| `cog backup create --name <name>` | Full DB snapshot via VACUUM INTO |
| `cog backup list` | List all backups |
| `cog backup restore <name>` | Restore backup as active model |
| `cog backup drop <name>` | Delete backup file |

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
- **Sync** — tree-sitter based code structure analysis, idempotent and repeatable. Scans from the project root (derived from `.cog/cog.db` location), parses source files into ASTs, extracts definitions (functions/classes/structs/methods), creates entities + `contains` relations, cleans up stale entities (skipping those with existing assertions). All auto-created entities carry origin `Scan`. Use `cog sync --init` to create `.cog/` on first use.
- **Experiment** — lightweight hypothesis testing on in-memory snapshots; `try` for quick one-liners, step-by-step for complex scenarios. Commit replays staged operations to the real model.
- **Backup** — full DB snapshot (VACUUM INTO) for safety nets before large-scale refactors

## Ids

Output shows **short IDs** (first 8 chars of UUID). All commands accepting IDs (`retract`, `assert --depends-on`) resolve short IDs automatically — pass the 8-char form from output.

## Storage

Single SQLite file at `.cog/cog.db` in WAL mode with foreign keys enabled. Override with `--db <path>` or `COG_DB` env var. Gitignore `.cog/` for private use or commit to share model state.
