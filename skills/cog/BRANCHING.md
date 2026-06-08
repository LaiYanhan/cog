# Experiment & Backup

Two tools for speculative reasoning at different scales.

## Experiment: Lightweight Hypothesis Testing

Experiments test "what if" scenarios on lightweight in-memory snapshots
(BFS subgraph, max 500 nodes) without copying the entire database.

### Commands

| Command | Purpose |
|---------|---------|
| `cog experiment try <entity> --kind <k> --claim "<t>" --grounds "<s>" [--desc "<d>"]` | Quick one-liner: start + hypothesize + evaluate |
| `cog experiment start <entity> --desc "<desc>"` | Start experiment on an entity's dependency subgraph |
| `cog experiment hypothesize <id> --assert ...` | Inject hypothetical assertion |
| `cog experiment hypothesize <id> --delete <entity>` | Inject hypothetical entity deletion |
| `cog experiment evaluate <id>` | Evaluate impact of all staged operations. Returns risk, contradictions, scout suggestions. |
| `cog experiment report <id>` | Show full experiment report |
| `cog experiment commit <id>` | Replay staged operations to real model |
| `cog experiment discard <id>` | Discard experiment |
| `cog experiment list` | List all experiments |

### Quick Workflow (covers 80% of scenarios)

```bash
# One-liner: start + hypothesize + evaluate
cog experiment try auth::login --kind correction \
    --claim "now accepts (user, pass, rate_limit)" \
    --grounds "hypothesis:rate-limit-feature" \
    --desc "add rate_limit parameter to login"

# → Risk: High (0.82)
# → Contradictions: api::login_handler expects 2 params
# → Scout before implementing: [read] api::login_handler — verify current signature

# Commit or discard
cog experiment commit <id>
cog experiment discard <id>
```

### Step-by-Step Workflow (complex scenarios)

```bash
# 1. Start an experiment around a focal entity
cog experiment start auth::login --desc "what if login takes 3 params?"

# 2. Inject hypothetical operations
cog experiment hypothesize <id> --assert auth::login \
    --kind contract --claim "now accepts (user, pass, rate_limit)" \
    --grounds "hypothesis:rate-limit-feature"

# 3. Evaluate the impact
cog experiment evaluate <id>
# → Risk: High (0.82)
# → Affected: 7 assertions
# → Contradictions: api::login_handler expects 2 params
# → Scout before implementing: [read] api::login_handler — verify current signature

# 4. After scouting, re-evaluate if needed (supports Phase 3/4 loop)
cog experiment hypothesize <id> --delete some_entity   # adjust plan
cog experiment evaluate <id>                           # re-evaluate

# 5. Commit (replay to real model) or discard
cog experiment commit <id>
cog experiment discard <id>
```

### Commit Semantics

Commit is deterministic replay of staged operations:
- `HypotheticalAssertion` → `create_assertion` on real DB
- `HypotheticalRetraction` → `retract` + TMS cascade on real DB
- `HypotheticalRelation` → `add_entity_relation` on real DB
- `HypotheticalDeleteEntity` → `delete_entity` on real DB

No diff-merge, no UUID conflict resolution. The experiment records operations
(intent), not state diffs — replay is deterministic.

---

## Backup: Full Model Snapshots

For large-scale refactors, create a full DB snapshot as a safety net.
Backups use `VACUUM INTO` to create a complete copy of the database.

### Commands

| Command | Purpose |
|---------|---------|
| `cog backup create --name <name>` | Full DB snapshot |
| `cog backup list` | List all backups |
| `cog backup restore <name>` | Restore backup as active model |
| `cog backup drop <name>` | Delete backup file |

### Workflow

```bash
# Snapshot before a major change
cog backup create --name "before-refactor"

# ... make large-scale changes ...

# If things go wrong, restore
cog backup restore "before-refactor"

# Clean up
cog backup drop "before-refactor"
```

---

## When to Use Which

| Scenario | Tool |
|----------|------|
| "What if I change this one entity's contract?" | Experiment (`try`) |
| "What if I delete this module?" | Experiment |
| "I'm about to refactor the entire persistence layer" | Backup |
| "I'm about to change the schema" | Backup |
| Quick hypothesis test, discard if wrong | Experiment |
| Need to save and resume across sessions | Experiment |
