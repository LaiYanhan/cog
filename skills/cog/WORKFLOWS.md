# Workflows

Scenario-based recipes for using cog. Each workflow is a concrete sequence
of commands — run them, read the output, act on it.

- [Analyzing & Debugging an Existing Codebase](#analyzing--debugging-an-existing-codebase)
- [Starting a New Project (From Scratch)](#starting-a-new-project-from-scratch)
- [After Reading Unfamiliar Code (Retrofitting)](#after-reading-unfamiliar-code-retrofitting)
- [Before a Risky Refactor](#before-a-risky-refactor)
- [When You Find a Subtle Trap](#when-you-find-a-subtle-trap)
- [When Retracting Outdated Knowledge](#when-retracting-outdated-knowledge)
- [When You Learn from a Mistake](#when-you-learn-from-a-mistake)
- [Linking Entities During Architecture Exploration](#linking-entities-during-architecture-exploration)
- [Progressive Grounding Lifecycle](#progressive-grounding-lifecycle)

## Analyzing & Debugging an Existing Codebase

When you encounter an unfamiliar codebase with existing code, use `cog init`
to build a structural skeleton automatically, then deepen it with semantic
knowledge that only an LLM can provide.

### Phase 1: Structural scan (automated)

`cog init` uses tree-sitter to parse source files and extract definitions
(functions, classes, structs, methods), directory structure (modules), and
import/dependency relationships — all without any manual input.

```bash
# Scan the project directory
cog init /path/to/project

# Review what was discovered
cog stats                        # entity/relation counts
cog index                        # entities sorted by assertion count
cog trace <root-module>          # see the full hierarchy
```

All auto-generated entities carry grounds `auto:scan` so you can distinguish
them from manually asserted knowledge.

### Phase 2: Semantic deepening (your job — automated analysis cannot do this)

Read the code. For each entity you understand, assert the knowledge that
tree-sitter cannot extract:

* **contracts** — what does this function promise its callers?
* **invariants** — what must always be true, or a bug results?
* **fragility** — what could break and why?
* **intent** — why does this code exist? What design decision does it encode?

```bash
# After reading a function, record what you learned
cog assert auth::login --kind contract \
  --claim "Returns Ok(token) on valid credentials, Err on invalid" \
  --grounds "code:auth::login"

cog assert auth::login --kind invariant \
  --claim "Password is zeroed from memory before returning" \
  --grounds "code:auth::login"

cog assert auth::login --kind fragility \
  --claim "Token expiry not checked on refresh — stale tokens accepted after rotation" \
  --grounds "code:auth::login"

# Link entities discovered during reading
cog depend auth::login --on auth::token::validate --kind calls
```

### Phase 3: Change planning

Before making changes, understand the blast radius.

```bash
cog impact auth::login          # what depends on this?
cog trace auth::login           # full picture: assertions + deps

# Sandbox your planned fix
cog branch create --name fix-token-refresh
cog branch switch fix-token-refresh
```

### Phase 4: Validation

```bash
# Check model ↔ code consistency (detects stale/unmodeled entities)
cog verify --scan

# Check structural consistency (orphan entities, dangling deps)
cog verify
```

> **Key principle**: Automated scanning gives you structure (entity names,
> kinds, containment, imports). Your job is to add the semantics — the
> *why*, the *what-could-break*, the *what-it-promises*. That is the
> irreplaceable value an LLM provides over a parser.

---

## Starting a New Project (From Scratch)

When no code exists yet, model the *design first* — planned modules, contracts,
invariants. As you implement, assertions migrate from speculative
(`plan:...` grounds) to code-grounded (`code:...`). This keeps the model in
sync without retroactive documentation.

### Phase 1: Design (before writing code)

Create a branch and assert your planned architecture.

```bash
# 1. Snapshot the (empty) model into a sandbox
cog branch create --name my-project-design

# 2. Switch to the sandbox
cog branch switch my-project-design

# 3. Assert top-level module intent
cog assert myapp --kind intent \
  --claim "CLI tool for batch image processing" \
  --grounds "plan:design-doc"
cog assert myapp::cli --kind intent \
  --claim "Parse args and dispatch to subcommands" \
  --grounds "plan:design-doc"
cog assert myapp::core --kind intent \
  --claim "Image processing primitives (resize, crop, filter)" \
  --grounds "plan:design-doc"
cog assert myapp::io --kind intent \
  --claim "File I/O with progress reporting" \
  --grounds "plan:design-doc"

# 4. Model module structure
cog depend myapp --on myapp::cli --kind contains
cog depend myapp --on myapp::core --kind contains
cog depend myapp --on myapp::io --kind contains

# 5. Assert contracts for key interfaces (write code to satisfy these)
cog assert myapp::core::resize --kind contract \
  --claim "Takes (image, width, height), returns resized image or Err" \
  --grounds "plan:api-spec"
cog assert myapp::core::resize --kind invariant \
  --claim "Output dimensions never exceed MAX_RESOLUTION (4096x4096)" \
  --grounds "plan:constraints"
cog assert myapp::io::read_image --kind intent \
  --claim "Use lazy decoding — read EXIF headers first, decode pixels on demand" \
  --grounds "design:perf-notes"

# 6. Check your design
cog query myapp
cog trace myapp
```

**What grounds to use when code doesn't exist:**

| Grounds pattern | When |
|----------------|------|
| `plan:design-doc` | Design decisions from an architecture doc |
| `plan:api-spec` | Interface contracts from an API specification |
| `plan:constraints` | Hard constraints (performance, memory, security) |
| `design:perf-notes` | Performance-related design choices |
| `spec:requirements` | Requirements or user stories the code must satisfy |

### Phase 2: Implement

Write code to match the asserted contracts and invariants. The assertions
serve as a checklist — you ground them for real once code exists.

```bash
# Re-read your design to stay on track
cog query myapp::core::resize
cog impact myapp::core     # what depends on this?
```

### Phase 3: Ground (migrate speculative → code-grounded)

Once code exists, update grounds to point at real source. Retract design
assumptions that implementation proved wrong.

```bash
# Migrate a contract from plan to code
cog retract <resize_contract_id> --reason "re-grounding to implementation"
cog assert myapp::core::resize --kind contract \
  --claim "Takes (image, width, height), returns resized image or Err" \
  --grounds "code:myapp::core::resize"

# Retract a design choice that turned out wrong
cog retract <lazy_decode_id> --reason "implementation chose eager decode — simpler"

# Assert new knowledge discovered during implementation
cog assert myapp::core::resize --kind fragility \
  --claim "JPEG EXIF rotation metadata stripped during resize — must preserve orientation" \
  --grounds "code:myapp::core::resize"

cog assert myapp::core::resize --kind correction \
  --claim "Added orientation preservation in resize pipeline (commit abc1234)" \
  --grounds "code:myapp::core::resize"
```

### Phase 4: Merge (if using a branch)

```bash
cog branch switch _main
cog branch diff my-project-design     # review what changed
cog branch merge my-project-design --apply-all
```

### Phase 5: Closeout

```bash
cog branch drop my-project-design
cog verify --clean
```

> **Tip**: You can assert directly on main if you are confident.
> Use the branch when exploring alternatives or keeping speculative assertions
> separate from validated ones.

---

## After Reading Unfamiliar Code (Retrofitting)

Build a cognitive model as you read, capturing contracts and invariants before
you modify anything.

```bash
# Read a function, assert what it promises
cog assert auth::login --kind contract \
  --claim "Returns Ok(token) on valid credentials, Err on invalid" \
  --grounds "code:auth::login"

# Record a key invariant
cog assert auth::login --kind invariant \
  --claim "Password is zeroed from memory before returning" \
  --grounds "code:auth::login"

# Link entities you discover
cog depend auth --on auth::login --kind contains
cog depend auth::login --on auth::token::validate --kind calls

# Query what the model already knows
cog query auth
```

---

## Before a Risky Refactor

Analyse blast radius before touching a module.

```bash
# Step 1: What does this entity promise?
cog query store::ConnectionPool

# Step 2: What depends on it? (BFS downstream)
cog impact store::ConnectionPool

# Step 3: Full dependency tree with evidence
cog trace store::ConnectionPool

# Step 4: Structural health check
cog verify

# Step 5: Fill gaps before refactoring
cog assert store::ConnectionPool --kind contract \
  --claim "get() blocks until a connection is available, never returns None" \
  --grounds "code:store::ConnectionPool"
```

---

## When You Find a Subtle Trap

Record fragility so future agents see the warning.

```bash
cog assert cache::invalidate --kind fragility \
  --claim "Race condition: concurrent invalidation + get-or-fill can return stale data" \
  --grounds "manual:code-review"

# If the fix is known, record it too
cog assert cache::invalidate --kind correction \
  --claim "Fix: use compare-and-swap with version counter" \
  --grounds "manual:code-review"
```

---

## When Retracting Outdated Knowledge

```bash
cog retract <short_id> --reason "assumption invalidated by refactor"
```

The cascade marks dependent assertions as `uncertain`. Review them after:

```bash
cog query <entity> --all
```

---

## When You Learn from a Mistake

Record both the fragility (to prevent recurrence) and the correction (to
explain the fix).

```bash
cog assert myapp --kind fragility \
  --claim "Output format changes must be verified against all ID-consuming input paths" \
  --grounds "incident:short-id-roundtrip-gap"

cog assert myapp --kind correction \
  --claim "Added resolve_assertion_id for short ID prefix matching" \
  --grounds "code:cog::model::store"
```

---

## Linking Entities During Architecture Exploration

Entity relations power `trace` and `impact`. Record them as you discover
structure.

```bash
cog depend myapp::model::graph --on myapp::model::store --kind uses
cog depend myapp::model --on myapp::model::graph --kind contains
cog depend myapp::command::verify --on myapp::model::store --kind uses
cog depend myapp::command::retract --on myapp::model::graph::retract --kind calls
```

When recording, think: *"If X changes, what does that mean for Y?"*

| Relation | Impact direction | Example |
|----------|------------------|---------|
| `contains` | Forward — parent change impacts children | `myapp::model` changes → `myapp::model::store` impacted |
| `uses` | Reverse — dependency change impacts dependents | `store` changes → `graph` (uses store) impacted |
| `calls` | Reverse — callee change impacts callers | `retract` changes → `verify` (calls retract) impacted |

---

## Progressive Grounding Lifecycle

Assertions move through four phases over their life:

```
Design (plan:...) → Implement → Ground (code:...) → Maintain
```

### Design phase
Grounds: `plan:design-doc`, `plan:api-spec`, `spec:requirements`

Use these when no code exists. Keep assertions coarse — module boundaries,
key interfaces, hard constraints. Avoid over-modelling internal details that
will change during implementation.

### Implementation phase
No model changes needed. Code the assertions as a checklist.

### Grounding phase
Grounds: `code:<entity>`, `test:path/to/test.rs`

Replace `plan:...` grounds with `code:...` after writing the real implementation.
Retract design assumptions that turned out wrong. Assert new knowledge
discovered during implementation (fragility, corrections).

### Maintenance phase
Grounds: `code:<entity>`, `incident:description`, `manual:review`

Update grounds when entities are renamed or removed (detected by `verify --scan`).
Retract fragility after fixing the root cause. Assert corrections when fixing bugs.

### Quick reference

| Grounds pattern | Phase | Example |
|----------------|-------|---------|
| `plan:design-doc` | Design | Architecture decision before code exists |
| `plan:api-spec` | Design | Interface contract from a spec |
| `plan:constraints` | Design | Hard limit from requirements |
| `design:perf-notes` | Design | Performance tradeoff decision |
| `spec:requirements` | Design | Requirement the code must satisfy |
| `code:<entity>` | Ground/Maintain | Evidence from a code entity identified by qualified name |
| `test:path/to/test.rs` | Ground/Maintain | Evidence from passing test |
| `manual:review` | Any | Human code review finding |
| `incident:description` | Maintain | Bug or incident post-mortem |
| `runtime_error:desc` | Maintain | Observed runtime failure |
