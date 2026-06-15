# Best Practices & Anti-Patterns

The workflow state machine and `cog next` encode operational best practices.
These are the timeless patterns and warnings that the state machine can't enforce.

## Anti-Patterns

### Asserting on the wrong entity
Putting store behaviour on a type, or user-facing contracts on an internal
helper. If a `query` on the entity returns assertions that feel surprising,
you may be asserting on the wrong scope.

### Creating test assertions and forgetting to clean up
Test entities are fine during exploration. Run `verify --clean` or
`delete-entity` before finishing. Or use an experiment and discard it when done.

### Using `fragility` as a permanent warning
A fragility should either be fixed (→ retract with reason `"fixed: ..."`)
or promoted to a tracked bug. Fragilities that sit forever become noise.

### Recording what code does (that's the code's job)
Assertions should capture *why* it does it (`intent`), *what it promises*
(`contract`), *what could break* (`fragility`), or *what was wrong*
(`correction`). Restating obvious code behaviour wastes the model's value.

### Over-modelling internal details
Not every private function needs an assertion. Decision rule: *"If this
assumption changes, would it cause a hard-to-find bug?"* If no, don't assert.

### Forgetting to migrate grounds after implementation
Speculative `plan:...` grounds left in the model after code exists reduce
confidence. Migrate to `code:<entity>` grounds during the grounding phase.
See [Progressive Grounding Lifecycle](WORKFLOWS.md#progressive-grounding-lifecycle).

### Leaving stale grounds
`code:` grounds reference entity qualified names (e.g., `code:auth::login`),
not file paths with line numbers. These stay stable across minor edits.
Run `verify --scan` to detect when a referenced entity no longer exists.

### Delete-entity is irreversible
It removes the entity **and all its data** — assertions, evidence, relations,
changelog. Review the reported counts before confirming.

---

## Assertion Kinds

| Kind | Use when | Example |
|------|----------|---------|
| `contract` | The code makes a promise callers depend on | "Returns None on invalid input, never panics" |
| `intent` | A design decision that isn't obvious from the code | "Retry logic exists because upstream is flaky" |
| `invariant` | A property that must always hold, or a bug results | "Pool size never exceeds MAX_CONNECTIONS" |
| `fragility` | A known risk or trap for future maintainers | "Depends on undocumented header format from v2 API" |
| `correction` | A mistake was made and fixed — don't repeat it | "Off-by-one in bounds check fixed in abc1234" |

**Do NOT use** `structure`, `behavior`, or `safety` — they are not valid kinds.

## Entity Relation Kinds

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
