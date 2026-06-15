# Repository Guidelines

## Project Overview

**Cog** is a cognitive model CLI for LLM coding agents. It scans codebases with tree-sitter to build a structural model (entities + containment hierarchy), then records knowledge claims (assertions) about code — contracts, invariants, fragility points — tracking dependencies so agents can reason about what they know and what might break.

A Rust binary-only crate, edition 2024. No async, no ORM, no build script. Single `cargo build --release` produces `target/release/cog`.

## Architecture & Data Flow

```
main.rs → cli::Cli::parse() → command::<module>::execute(&dyn Repository) → format::* → CommandOutput
                                      ↕
                              repo::SqliteRepository
                           (rusqlite, SQLite, WAL)
                            impl repo::Repository
                                      ↕
                            analysis::Scanner
                    (tree-sitter, 6 languages)
```

### Six-Layer Architecture

Cog is organized into six layers, bottom-up: **code space** (the source code being modeled), **persistence** (`Repository` trait, `SqliteRepository` impl), **analysis+modeling** (tree-sitter scanning + domain types), **cognitive latent space** (graph algorithms: cascade, impact, trace), **workflow guide** (state machine + suggestion engine), and **experiment** (hypothesis evaluation). See `docs/architecture/01-overview.md` for the full layered diagram and design rationale.

### Data Flow (per command)

1. `src/cli.rs` parses args (Clap derive)
2. Dispatches to `command/<module>::execute(repo, args)` where `repo` is `&dyn Repository` (or `&SqliteRepository` for retract)
3. Execute reads/mutates through the `Repository` trait, produces a `CommandOutput { text, exit_code }`
4. `main.rs` calls `output.emit()` and exits with `exit_code` if non-zero

### Key Design Decisions

- **No async** — everything is synchronous. The tree-sitter parsing and SQLite operations are fast enough.
- **anyhow::Result** everywhere — no custom error types. Preconditions use `bail!()` / `anyhow!()`.
- **Repository trait** decouples commands from storage — single `SqliteRepository` impl, tests use `open_in_memory()` for real SQL semantics without disk I/O
- **Output formatting** centralized in `format/` module — `Renderable` trait routes reports to `TextRenderer` (human-readable) or `JsonRender` (machine-readable) via `--output` flag
- **Workflow state machine** guides agent via `cog next` — serialized to `.cog/workflow_state.json`; tracks Uninit → Ready → Changing transitions
- **Short IDs** — UUIDs displayed as first 8 chars; resolved automatically across all commands.

## Key Directories

| `src/cli.rs` + `src/cli/` | Clap-derived CLI definition (args, experiment, backup), workflow state management |
| `src/repo.rs` + `src/repo/` | Repository trait + SqliteRepository (split into 10 focused submodules) |
| `src/domain.rs` + `src/domain/` | Core types: entity, assertion, evidence, relations, grounds, reports |
| `src/command.rs` + `src/command/` | One file per subcommand: most accept `&dyn Repository`, retract accepts `&SqliteRepository` |
| `src/space.rs` + `src/space/` | Graph algorithms: CascadeEngine, ImpactEngine, TraceEngine, SemanticSpace, StructureSpace |
| `src/workflow.rs` + `src/workflow/` | WorkflowState state machine + suggestion engine |
| `src/format.rs` + `src/format/` | TextRenderer — unified text output for all reports |
| `src/analysis.rs` + `src/analysis/` | Tree-sitter scanning: Scanner, ParserPool, FileWalker, language extractors |
| `src/experiment.rs` + `src/experiment/` | Hypothesis experiment: session, ops, report, persistence |
| `src/backup.rs` + `src/backup/` | Full model backup: BackupManager (create/list/restore/drop) |
| `tests/` | Integration + unit tests (24 total) |
| `benchmark/` | Harbor Terminus-2 benchmark harness for A/B evaluation |
| `skills/cog/` | Skill documentation for agent runtime |
## Development Commands

```sh
# Build
cargo build --release          # produces target/release/cog
cargo build                     # debug build

# Test
cargo test                      # runs tests (subprocess-based)

# Run
cargo run -- init .             # scan current dir
cargo run -- assert my::fn --kind contract --claim "does X" --grounds "code:my::fn"
cargo run -- query my::fn
cargo run -- next                # workflow suggestion
cargo run -- experiment start <entity> -- start a hypothesis experiment
cargo run -- backup create --name <name> -- create a full model backup

# Direct binary (after build)
./target/release/cog sync .
```

**No formatter/linter config overrides** — rustfmt and clippy use all defaults. No pre-commit hooks, no CI config in the repo.

## Code Conventions & Common Patterns

### Module Structure

```
src/command/<name>.rs
  pub fn execute(repo: &dyn Repository, <specific_args>) -> Result<CommandOutput>
```
Every command module exports exactly one public `execute()` function. Most accept `&dyn Repository`; retract accepts `&SqliteRepository`.

### Error Handling

- **`anyhow::Result`** for every fallible function. No custom error types anywhere.
- **`bail!("message")`** for precondition failures (unknown entity, unresolved dependency).
- **`anyhow!("message")** for inline error construction.
- **Non-error exits** use `CommandOutput::with_exit_code(text, 1)` — e.g., entity not found is exit 1 but not a panic/error.
- Commands **never** call `process::exit()` themselves — they return exit code in CommandOutput.

### Naming

- **Entities**: `qualified_name` — `::`-separated (e.g. `cog::repo::sqlite::SqliteRepository`).
- **`snake_case`** for everything: functions, variables, file names.
- **`CommandOutput`** struct with `{ text, exit_code }`.
- **`EntityKind::infer(name)`** heuristic: uppercase start → `Type`, contains `::` → `Function`, else `Module`.

### Core Patterns

- **`SqliteRepository`** wraps a `rusqlite::Connection`. Schema: 6 tables (entities, assertions, evidence, entity_relations, assertion_relations, changelog). UUID primary keys. WAL mode. Foreign keys enabled.
- **`repo.append_changelog(action, target_id, detail)`** — mutations append a changelog entry (Repository trait method).
- **Cascade retraction** — `CascadeEngine::retract()` BFS through `depends_on` edges, marking dependents `Uncertain` or `GroundWeakened`.
- **`ModelSnapshot`** — full state capture used for diffing and export. Serialized via serde (JSON/TOML/DOT).
- **Entity kind inference** — `EntityKind::infer(name)` classifies entities by qualified name: uppercase → `Type`, contains `::` → `Function`, else `Module`.
- **Entity origin** (`EntityOrigin`) — `Scan` (auto-extracted by tree-sitter), `Manual` (created via assert/depend).
- **Assertion status** — `Active`, `Retracted`, `Uncertain`, `GroundWeakened`.

### Graph Algorithms

| `space/cascade.rs` | `CascadeEngine` | BFS — retract one assertion, cascade uncertain to all transitive dependents |
| `space/impact.rs` | `ImpactEngine` | BFS — find all entities downstream of a given entity |
| `space/trace.rs` | `TraceEngine` | Recursive DFS — build dependency tree (cycle protection via visited set) |

### Experiment

- Experiments test "what if" scenarios on lightweight in-memory snapshots (BFS subgraph, max 500 nodes).
- Persisted to `.cog/experiments/<id>.json` for cross-session recovery.
- Draft (unsaved) vs saved checkpoint semantics; `list` distinguishes them.
- Commit replays staged operations deterministically to the real DB — no diff-merge.

### Backup

- Full DB snapshot via `VACUUM INTO` for safety nets before large-scale refactors.
- BackupManager: `create`/`list`/`restore`/`drop`.
- Reserved names: `_main`, `_main_backup`.

### Testing

- **No test framework** beyond `#[test]`. No rstest, test-case, or mocks.
- Three integration tests: happy path workflow, retraction cascade verification. Plus unit tests in command/ and repo/ modules.
- Unit tests in command/repo modules use `SqliteRepository::open_in_memory()`.
- Helper: `cog_bin()` reads `CARGO_BIN_EXE_cog` env var (set by `cargo test`).
- Pattern: `let output = run_ok(&["init", "."], &db_path)` → assert stdout contains expected text.

### Scan/Analysis

- `Scanner::new().scan(ScanConfig)` returns `ScanReport { files, definitions, imports, languages }`.
- BFS directory walk, skips hidden dirs, `target/`, `node_modules/`, `__pycache__/`.
- Per-language extractors are free functions matching tree-sitter node kinds:
  - Python: `function_definition`, `class_definition`, `import_statement`, `import_from_statement`
  - Rust: `function_item`, `struct_item`, `enum_item`, `trait_item`, `impl_item`, `use_declaration`
  - JavaScript: `function_declaration`, `generator_function_declaration`, `class_declaration`, `export_statement`, `import_statement`
  - Go: `function_declaration`, `method_declaration`, `type_declaration`, `import_declaration`
  - C: `function_definition`, `struct_specifier`, `type_definition`, `preproc_include`
  - Java: `class_declaration`, `method_declaration`, `import_declaration`
- `node_text(node, source)` helper extracts source text for a node.

## Important Files

| `src/repo.rs` + `src/repo/` | SqliteRepository — split into 10 focused submodules |
| `src/repo/trait.rs` | Repository trait — persistence contract |
| `src/domain/entity.rs` | Entity, EntityKind, EntityOrigin, last_segment(), parent_qname() |
| `src/domain/assertion.rs` | Assertion, AssertionKind, AssertionStatus |
| `src/space/cascade.rs` | Retraction cascade — two-phase: SemanticSpace + Repository |
| `src/space/impact.rs` | Downstream impact analysis (ImpactEngine) |
| `src/space/trace.rs` | Dependency tracing (TraceEngine) |
| `src/space/semantic.rs` | SemanticSpace — TMS belief system, simulate_retract, assess_risk |
| `src/space/structure.rs` | StructureSpace — entity graph with BFS traversal |
| `src/workflow/state.rs` | WorkflowState machine + transitions |
| `src/workflow/suggestions.rs` | Suggestion engine for cog next |
| `src/format/text.rs` | TextRenderer — human-readable output |
| `src/format/json.rs` | JsonRender — machine-readable JSON output |
| `src/command/verify.rs` | Structural consistency checks |
| `src/command/sync_cmd.rs` | Tree-sitter scanning orchestration (sync) |
| `src/domain/metrics.rs` | EntityMetrics — fan_in, fan_out, line_count, visibility |
| `src/experiment/session.rs` | Experiment session with BFS subgraph loading |
| `src/backup/manager.rs` | BackupManager wrapper around VACUUM INTO |
| `src/analysis/extractors.rs` | Language extractors (Python, Rust, JS, Go, C, Java) |

## Runtime/Tooling Preferences
- **No build script** (`build.rs` absent).
- **No formatter/linter config** — rustfmt and clippy use defaults.
- **Single binary** — no lib target, no workspace.
- **SQLite** via `rusqlite` with `bundled` feature (no system SQLite needed).
- **Gitignore** covers: `/target`, `.omp/`, `.cog/`, `__pycache__/`, `*.pyc`, `terminal-bench/`, `jobs/`.

## Testing & QA
- **Framework**: `#[test]` + `std::process::Command` for black-box CLI tests.
- **Coverage**: integration tests exercising: full CRUD workflow, retraction cascade with verification.
- **Isolation**: Fresh tempdir + temp DB per test. No shared state.
- **Running**: `cargo test` (builds binary, runs integration tests).
- Integration tests are end-to-end through the binary interface; unit tests exercise individual modules.
- **Test helpers**: `cog_bin()`, `run(args, db_path)`, `run_ok(args, db_path)`, `parse_assertion_id(output)`.
- **Error testing**: Commands with `bail!()` failures cause subprocess non-zero exit → caught by `run()` return value.

### Adding Tests

Follow the existing pattern: create a tempdir, run `cog` as a subprocess, assert on stdout content. No mocking. Use `run_ok()` for expected-success and inspect `run()` output for expected-failure.

```rust
let dir = tempfile::tempdir()?;
let db = dir.path().join("cog.db");
let output = run_ok(&["init", "."], &db);
assert!(output.contains("entities created"));
```

## Self-Bootstrapping (Cog Uses Cog)

This project is **self-bootstrapping** — cog serves as its own external cognitive model. Agents working on this codebase MUST use cog to record and query knowledge about the codebase itself.

### Required Workflow

Before making any significant change (refactor, bugfix, new feature):

```sh
# 1. Initialize or refresh the structural model
cargo run -- init .

# 2. Query the entities you'll be touching
cargo run -- query <module>::<entity>
cargo run -- impact <module>::<entity>   # blast radius
cargo run -- trace <module>::<entity>    # dependency chain

# 3. Record contracts, invariants, and fragility before changing
cargo run -- assert <entity> --kind contract \
    --claim "<what it promises>" --grounds "code:<entity>"
cargo run -- assert <entity> --kind fragility \
    --claim "<what could break>" --grounds "code:<entity>"

# 4. After the change, update or retract stale assertions
cargo run -- retract <assertion-id> --reason "refactored in current session"
cargo run -- assert <entity> --kind correction \
    --claim "<what changed and why>" --grounds "code:<entity>"

# 5. Verify structural consistency
cargo run -- verify --scan
```

### What to Record

| Scenario | Assertion Kind | Example |
|----------|---------------|---------|
| Understanding a module's API contract | `contract` | `"SqliteRepository::open creates DB in WAL mode, returns Err on I/O failure"` |
| Noting a design decision or rationale | `intent` | `"Experiment commit replays staged operations deterministically — no diff-merge"` |
| A constraint that must hold | `invariant` | `"CascadeEngine::retract() BFS never revisits already-marked assertions"` |
| Something fragile or risky | `fragility` | `"Node text extraction assumes tree-sitter node is within source bounds — panics if not"` |
| A fix applied during this session | `correction` | `"Fixed off-by-one in path_to_qualified — was dropping first segment"` |

## Meta-Loop: Using Cog to Improve Cog

This project's self-bootstrapping creates a unique feedback loop: **modeling cog with cog naturally exposes cog's own limitations**. Every friction point you encounter while using cog on itself is valuable signal about what needs improvement.

### The Meta-Loop Cycle

```
┌────────────────────────────────────────────────────┐
│  1. Model cog with cog (as described above)        │
│     → init, assert, query, impact, verify          │
│                                                    │
│  2. Feel the friction                              │
│     → "This query output is too verbose"           │
│     → "I can't express X kind of knowledge"        │
│     → "The verify check misses Y pattern"          │
│     → "This command is missing a flag I need"      │
│                                                    │
│  3. Record the problem as a fragility assertion    │
│     → cog assert <entity> --kind fragility ...     │
│                                                    │
│  4. Experiment or prototype the fix                │
│     → cog experiment start <entity> --desc "..."   │
│     → cog experiment hypothesize <id> --assert ... │
│     → cog experiment evaluate <id>                 │
│     → Implement the code change                    │
│                                                    │
│  5. Validate, commit, merge                        │
│     → Run tests, verify                            │
│     → cog experiment commit <id>                   │
│     → Or: cog backup create before large changes   │
│                                                    │
│  6. Retract the old fragility, assert correction   │
│     → cog retract <old-fragility-id>               │
│     → cog assert <entity> --kind correction ...    │
└────────────────────────────────────────────────────┘
```

### Concrete Scenarios

| What you feel | The meta-loop action |
|---------------|---------------------|
| Output is hard to parse or too verbose | Assert `format::*` fragility → experiment to add `--json` or `--quiet` flag → implement → commit |
| Can't batch-record many assertions at once | Assert `command::assert_cmd` fragility → experiment to add bulk-assert or `--file` flag → implement → commit |
| `verify` misses a specific issue pattern | Assert `command::verify::execute` fragility → experiment to add the check → implement → commit |
| `init` scanned too much noise (e.g. benchmark/) | Assert `analysis::Scanner::scan` fragility → experiment to add better skip patterns or `.cogignore` → implement → commit |
| Missing a command or flag | Assert the relevant module's `intent` with `"would benefit from <feature>"` → experiment to design and implement → commit |
| A graph algorithm is too slow for a real codebase | Assert `space::cascade` fragility → experiment to optimize → benchmark → commit |

### Rules of the Meta-Loop

- **Every friction point must be captured as an assertion** — `fragility` for the problem, `correction` after the fix. Future agents learn from past pain.
- **Use experiments for prototypes** — never modify the main model directly when experimenting with improvements to cog itself. Experiment snapshots are lightweight and discardable.
- **Use backups for large-scale changes** — `cog backup create --name pre-refactor` before schema or architecture changes.
- **Ground the improvement design** — use `plan:*` grounds for assertions that describe the *planned* improvement, migrate to `code:*` after implementation.
- **When the meta-loop reveals a fundamental design limitation** — assert it as an `intent` limitation on the affected module, then experiment with a redesign.

### Example: Discovering and Fixing a Limitation

```sh
# Step 1: Record the friction as a fragility
cog assert format::entity_index_with_counts --kind fragility \
  --claim "Output uses aligned columns that are hard to parse programmatically" \
  --grounds "meta-loop:cog-self-modeling"

# Step 2: Experiment with a fix
cog experiment start format::text::TextRenderer --desc "add --json flag for index"
cog experiment hypothesize <id> --assert command::index_cmd::execute \
    --kind intent --claim "Should support --json flag for machine-parseable output" \
    --grounds "plan:improvement"

# Step 3: Implement the change in src/command/index_cmd.rs and src/format/text.rs

# Step 4: Validate and commit
cargo test
cog verify
cog experiment commit <id>

# Step 5: Record the correction
cog retract <old-fragility-id> --reason "implemented --json flag for index command"
cog assert format --kind correction \
  --claim "Added --json output mode to index command for programmatic consumption" \
  --grounds "code:command::index_cmd::execute"
```

## Data Preservation During Breaking Changes

Cog is under active development. Schema changes, query interface changes, or storage format changes will occur. **You MUST NEVER delete or reset `.cog/cog.db`** — that file accumulates valuable knowledge across sessions.

### Migration Strategy (not destructive reset)

1. **Backup first** — snapshot the current model before any migration code runs:
   ```sh
   cargo run -- backup create --name pre-migration
   ```

2. **Write a migration** — the `SqliteRepository` already supports schema migrations. Add a new migration in `src/repo/sqlite/helpers.rs` that:
   - Checks current schema version (table exists, column exists, or a `pragma user_version` / schema version marker)
   - Alters tables additively (add columns, create new tables) — NEVER `DROP TABLE` or `DROP COLUMN`
   - Transforms existing data in-place or provides backward-compatible defaults for new columns

3. **Test migration path** — write an integration test that:
   - Creates a DB with the OLD schema (by building an older binary or constructing the schema manually)
   - Opens it with the NEW code
   - Verifies all pre-existing data is readable and correctly mapped

4. **Verify after migration**:
   ```sh
   cargo run -- stats                    # entity/assertion counts intact
   cargo run -- verify                   # no spurious issues from migration
   cargo run -- backup list              # backups preserved
   cargo run -- export --format json     # full data export readable
   ```

### What "Preserve Data" Means

- **All entities, assertions, evidence, and relations** must remain readable after migration. No silent data loss.
- **UUIDs must be stable** — changing ID generation breaks cross-references in backup snapshots and the changelog.
- **Backup snapshots** (separate DB files) must remain loadable. If the schema changes, old backup files must be migratable on open, not rejected.
- **If backward compatibility is impossible**, use `VACUUM INTO` to create a migrated copy and keep the original file as a backup — never in-place destroy.

### Prohibited Actions

- `rm -rf .cog/` or any equivalent — **NEVER**.
- `DROP TABLE` in a migration — **NEVER**. Additive schema changes only.
- Silently ignoring old data that doesn't fit the new schema — **NEVER**. Migrate it or error with a clear message.
- Reusing UUIDs or reassigning entity IDs — **NEVER**.
- Silently falling back to a fresh DB when the existing one has an incompatible schema — **NEVER**. The agent must report the incompatibility and propose a migration.
