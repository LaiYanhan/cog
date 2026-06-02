# cog

A cognitive model for LLM coding agents. Tracks assertions, dependencies, and evidence in a local SQLite database so agents can reason about what they know and what might break.

## Install

```sh
cargo build --release
```

## Usage

`cog` stores everything in `.cog/cog.db` (project-local). Set `COG_DB` to override.

### Write commands

```sh
# Declare an assertion about an entity
cog assert auth::login --kind safety --claim "login rejects expired tokens" \
    --grounds "manual test 2024-01-15" --depends-on auth::token_check

# Record a dependency between two entities
cog depend auth::login --on auth::session --kind depends_on

# Retract an assertion (cascades to dependents)
cog retract <assertion-id> --reason "no longer valid after refactor"
```

### Read commands

```sh
# Query all assertions for an entity
cog query auth::login

# Show downstream impact of an entity
cog impact auth::login

# Trace the full dependency chain for an entity
cog trace auth::login

# List all known entities
cog index

# Structural consistency checks
cog verify
cog verify --scope auth

# Model statistics
cog stats

# Export the full model (json / toml / dot)
cog export --format json
```

## Concepts

- **Entity** — a code construct (module, function, type) identified by qualified name (`auth::login`)
- **Assertion** — a claim about an entity (safety, correctness, behavior) with human-readable grounds
- **Evidence** — source material backing an assertion (test name, manual check, review)
- **Dependency** — an entity-level or assertion-level `depends_on` relation
- **Retraction** — invalidates an assertion and cascades `Uncertain` status to dependent assertions

## Storage

Single SQLite file at `.cog/cog.db`, WAL mode, foreign keys enabled. Add `.cog/` to `.gitignore` for private use, or commit it to share model state.