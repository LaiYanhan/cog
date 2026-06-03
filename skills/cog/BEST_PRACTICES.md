# Best Practices & Anti-Patterns

Experience-backed guidance for keeping the cognitive model accurate,
maintainable, and useful.

- [Lessons from Practice](#lessons-from-practice)
- [Anti-Patterns](#anti-patterns)
- [When to Use Each Assertion Kind](#when-to-use-each-assertion-kind)

---

## Lessons from Practice

### Query first, read second
Before diving into code, `cog query <entity>`. The model may already have
the contracts and invariants you need. Let existing knowledge reduce how
much code you need to read.

### `index` is your first stop
The assertion count per entity tells you what the model knows most about.
High-count entities are architectural anchors — start there.

### Link assertions with `--depends-on`
Individual assertions are facts; `--depends-on` chains are reasoning.
Without dependency chains, `trace` has nothing to traverse.

### Record cross-cutting invariants on the parent entity
If a rule applies to multiple modules (e.g. "all IDs resolve via prefix
match"), assert it on the root entity (`myapp`) not on each leaf. Impact
traversal will find it.

### `impact` combines both directions
It shows assertions from the entity **and** all reachable neighbours. Use it
before refactoring to understand blast radius.

### Retract fragility after fixing
Once you fix the issue, retract the fragility with
`--reason "fixed: <description>"`. Stale warnings erode trust in the model.

### `trace` is the full picture
It shows assertions (active-only), their evidence, the depends-on tree, and
entity relations. Use it when you need to deeply understand one entity.

### All read commands filter retracted by default
`query`, `impact`, and `trace` only show active assertions. Use `--all`
when investigating retraction history.

### `verify` is a confidence check, not a substitute for reading code
It tells you the model is structurally consistent (no orphan entities, no
bare assertions, no dangling deps). Run it after bulk changes.

### `verify --clean` removes isolated entities
Use this after test runs to clean up artifacts without manual
`delete-entity` calls.

### Grounds should point to current code
`code:` grounds reference entity qualified names (e.g., `code:auth::login`),
not file paths with line numbers. These stay stable across line-number changes
and minor edits. Run `verify --scan` to detect when a referenced entity no
longer exists in the codebase. Grounds still need updating when entities are
renamed or significantly restructured. See
[WORKFLOWS.md — Progressive Grounding Lifecycle](WORKFLOWS.md#progressive-grounding-lifecycle).

### Check merge reports for skips
When merging a branch, `applied N, skipped M` tells you if any evidence or
relation items couldn't be matched. A non-zero skip count needs
investigation. See [BRANCHING.md](BRANCHING.md) for merge semantics.

### Delete-entity is irreversible
It removes the entity **and all its data** — assertions, evidence, relations,
changelog. It reports the counts of removed data before executing, so review
those numbers before confirming.

---

## Anti-Patterns

These patterns produce a fragile or noisy model:

### Asserting on the wrong entity
Putting store behaviour on a type, or user-facing contracts on an internal
helper. If a `query` on the entity returns assertions that feel surprising,
you may be asserting on the wrong scope.

### Creating test assertions and forgetting to retract
Test entities are fine during exploration. Run `verify --clean` or
`delete-entity` before committing. Or use a branch for experimentation
and drop it when done.

### Using `fragility` as a permanent warning
A fragility should either be fixed (→ retract with reason `"fixed: ..."`)
or promoted to a tracked bug. Fragilities that sit forever become noise —
nobody trusts a permanently blinking warning.

### Recording what code does (that's the code's job)
Assertions should capture *why* it does it (`intent`), *what it promises*
(`contract`), *what could break* (`fragility`), or *what was wrong*
(`correction`). Restating obvious code behaviour wastes the model's value.

### Leaving test entities in the model
After experimenting with `cog assert myapp::scratch --kind intent ...`,
remove the entity with `cog delete-entity myapp::scratch`. Run
`cog verify --clean` as part of your closeout checklist.

### Over-modelling internal details
Not every private function needs an assertion. The decision rule: *"If this
assumption changes, would it cause a hard-to-find bug?"* If no, don't assert.

### Forgetting to migrate grounds after implementation
Speculative `plan:...` grounds left in the model after code exists reduce
confidence. Migrate to `code:<entity>` grounds (e.g., `code:auth::login`)
during the grounding phase.
See [Progressive Grounding Lifecycle](WORKFLOWS.md#progressive-grounding-lifecycle).

---

## When to Use Each Assertion Kind

| Kind | Use when | Example |
|------|----------|---------|
| `contract` | The code makes a promise callers depend on | "Returns None on invalid input, never panics" |
| `intent` | A design decision that isn't obvious from the code | "Retry logic exists because upstream is flaky" |
| `invariant` | A property that must always hold, or a bug results | "Pool size never exceeds MAX_CONNECTIONS" |
| `fragility` | A known risk or trap for future maintainers | "Depends on undocumented header format from v2 API" |
| `correction` | A mistake was made and fixed — don't repeat it | "Off-by-one in bounds check fixed in abc1234" |

**Do NOT use** `structure` or `behavior` — they are not valid kinds.

### Entity Relation Kinds

| Kind | Meaning | Example |
|------|---------|---------|
| `contains` | A is a parent scope of B | `myapp::model` contains `myapp::model::store` |
| `calls` | A invokes B at runtime | `retract_cmd` calls `graph::retract` |
| `uses` | A depends on B structurally | `verify` uses `store` |

**Do NOT use** `depends_on` for entity relations — that is an
assertion-level concept (`--depends-on`).

### Impact traversal direction

| Relation | Direction | Example |
|----------|-----------|---------|
| `contains` | Forward — parent change impacts children | `myapp::model` changes → `myapp::model::store` |
| `uses`/`calls` | Reverse — dependency change impacts dependents | `store` changes → `graph` (uses store) |

When recording relations, think: *"If X changes, what does Y mean for
impact?"* Then set direction accordingly.
