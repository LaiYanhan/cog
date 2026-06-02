# Branch & Merge

Branches snapshot the model for speculative reasoning — assert, retract, and
experiment without affecting the main model.

## Commands

| Command | Purpose |
|---------|---------|
| `cog branch create [--name <name>]` | Snapshot current model into a branch (name auto-generated if omitted) |
| `cog branch list` | List all branches (`_main_backup` excluded) |
| `cog branch switch <name>` | Activate branch — subsequent commands affect only the copy |
| `cog branch switch _main` | Return to main (saves branch state, clears active marker) |
| `cog branch diff <name>` | Show changes between main and branch |
| `cog branch diff <name> --item <N>` | Inspect a specific change in detail |
| `cog branch merge <name>` | Show merge plan (pending items) |
| `cog branch merge <name> --apply-all` | Apply all branch changes to main |
| `cog branch merge <name> --apply <N>` | Apply one specific change |
| `cog branch merge <name> --reject <N>` | Reject one specific change |
| `cog branch drop <name>` | Delete branch file |

**Reserved names**: `_main` (switch target) and `_main_backup` (internal)
cannot be created as branches.

## Workflow

```bash
# 1. Snapshot
cog branch create --name my-plan

# 2. Experiment (all writes affect only the branch copy)
cog branch switch my-plan
cog assert new::feature --kind intent --claim "planned feature" --grounds "plan:design-doc"
cog retract d6e3a49f --reason "outdated assumption"

# 3. Return to main, review diff
cog branch switch _main
cog branch diff my-plan

# 4. Merge validated changes
cog branch merge my-plan --apply-all

# 5. Clean up
cog branch drop my-plan
```

## Diff Semantics

Compares main (base) vs branch file. Each addition, removal, or modification
is an indexed item. Items within each category are sorted by ID — indexing is
**stable across processes**, so `--item <N>` produces consistent results.

## Merge Semantics

When applying branch changes to main:

| What | Behaviour |
|------|-----------|
| **Entity insertions** | Inserted with original UUID — cross-references stay valid |
| **Assertion insertions** | Inserted with original UUID; evidence and dependency relations preserved |
| **Entity removals** | **Skipped** (avoids broken references) |
| **Assertion removals** | Become retractions on main, not deletions |
| **Evidence/relations** | Verified against existing IDs; skipped items reported in summary |

Merge reports: `applied N, skipped M` — investigate any non-zero skip count.

## Use Cases

### Speculative design (from scratch)

Before writing code, branch, assert planned contracts and invariants, then
merge only the validated subset after implementation. Keeps the cognitive
model clean.

### Testing alternative architectures

Branch A: `myapp::core` as library, Branch B: `myapp::core` as subprocess.
Diff both against the real model, adopt the better one.

### Safe retraction experiments

Unsure if retracting an assertion will cascade too far? Branch first,
retract, inspect the damage, then decide whether to merge or drop.
