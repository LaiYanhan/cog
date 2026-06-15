# Workflows

The complete cog lifecycle. Every task moves through four phases — **build →
enrich → reason → descend** — then loops. `cog next` reads the workflow state
machine and the model to suggest which phase you're in and what to do next;
this document is the operational detail behind those suggestions.

```
1. BUILD  →  2. ENRICH  →  3. REASON  →  4. DESCEND  →  back to ENRICH
```

- **BUILD**: `cog sync` + `cog index` + `cog query` — auto-construct the structural model, survey it.
- **ENRICH**: `cog assert` + `cog depend` + `cog impact` — manually record contracts, invariants, fragilities; check blast radius.
- **REASON**: `cog experiment` (try / start+hypothesize+evaluate) — test hypotheses in an in-memory snapshot before touching code.
- **DESCEND**: implement code, then `cog sync` + `cog verify` + `cog assert correction` + `cog retract` stale — return to code space and reconcile the model.

The cycle is a loop, not a pipeline: failure discovered during DESCEND (undocumented runtime behaviour, wrong assumptions) is recorded as `fragility`/`correction` assertions, which feeds back into ENRICH. The model is strictly richer after every cycle.

---

## Phase 1 — Build the base model

`cog sync` runs tree-sitter over the codebase and constructs the **structural
layer** automatically: entity names, kinds, containment hierarchy, import and
call relations. Zero LLM input, fully deterministic and idempotent. Your job
here is to *survey*, not to record.

```bash
cog sync --init          # first time: creates .cog/ and scans
cog sync                 # repeat anytime — re-scans, reconciles drift
cog index                # coverage summary — what exists, what's asserted
cog query <entity>       # drill into a specific entity's current state
```

What sync gives you:
- **Entities** for every definition (functions, classes, structs, methods),
  directory modules, and file modules. All carry origin `Scan`.
- **Relations**: `contains` (module → child), `uses` (import), `calls`
  (call site → definition).
- **Metrics**: line counts, fan-in/fan-out, visibility.

What sync does **not** give you: the *why*. That is phase 2.

**Drift detection**: `sync` compares the current scan against stored `Scan`
entities. Removed code → stale entities (deleted, unless they have assertions).
New code → new entities. Drift moves the workflow to `PostChange`, signalling
the model needs reconciliation.

---

## Phase 2 — Enrich with semantics

This is where the irreplaceable value lives. A parser can see that
`auth::login` exists and is called by 4 entities. Only you can record *what it
promises*, *why it's built that way*, and *what would break if it changed*.

### What to record

```bash
# Contracts — what the code promises callers (interfaces, pre/postconditions)
cog assert auth::login --kind contract \
    --claim "Returns Ok(token) on valid credentials, Err on invalid" \
    --grounds "code:auth::login"

# Invariants — what must always hold, or a bug results
cog assert pool --kind invariant \
    --claim "Pool size never exceeds MAX_CONNECTIONS" \
    --grounds "code:pool"

# Fragilities — known risks and traps for future maintainers
cog assert auth::login --kind fragility \
    --claim "Relies on undocumented header format from v2 API" \
    --grounds "manual:review"

# Intent — a design decision not obvious from the code
cog assert retry --kind intent \
    --claim "Retry logic exists because upstream is flaky" \
    --grounds "code:retry"

# Corrections — a mistake that was made and fixed (append-only history)
cog assert bounds --kind correction \
    --claim "Off-by-one in bounds check fixed in abc1234" \
    --grounds "code:bounds"
```

### Decision rule: what NOT to record

Not every function needs an assertion. Ask: *"If this assumption changed, would
it cause a hard-to-find bug?"* If no, skip it. Restating obvious code behaviour
wastes the model's value — the code already says what it does. Record the
*why*, the *promise*, the *risk*, the *history*.

### Link entities structurally

```bash
cog depend <caller> --on <callee> --kind calls   # runtime invocation
cog depend <user>  --on <used>   --kind uses     # structural dependency
# (contains is auto-created by sync — rarely manual)
```

Relations drive `impact` and `trace` traversal. Think: *"If X changes, what
does Y mean for impact?"* — `calls`/`uses` flow in reverse (dependency change
impacts dependents).

### Check impact before changing anything

```bash
cog impact <entity>        # BFS downstream: who depends on this? how risky?
cog trace <entity>         # full dependency chain + evidence for root-cause
```

`impact` returns a risk score (HIGH/MEDIUM/LOW), downstream entities marked
covered vs blind, and blind-count. A high blind-count means you're about to
change something with no recorded knowledge beneath it — slow down.

### Progressive Grounding Lifecycle

Assertions move through phases as understanding matures:

```
Design (plan:...) → Implement → Ground (code:...) → Maintain
```

| Phase | Grounds | When |
|-------|---------|------|
| **Design** | `plan:design-doc`, `plan:api-spec`, `spec:requirements` | No code exists yet — coarse assertions about boundaries, interfaces, hard constraints |
| **Implementation** | (no model change) | Code the assertions as a checklist |
| **Grounding** | `code:<entity>` | Replace speculative `plan:` grounds once code exists; retract assumptions that were wrong; assert discoveries |
| **Maintenance** | `code:<entity>`, `test:...`, `incident:...` | Update grounds on rename/removal (`verify --scan` detects); retract fragility after fixing root cause; assert corrections |

| Grounds pattern | Phase | Example |
|----------------|-------|---------|
| `plan:design-doc` | Design | Architecture decision before code exists |
| `plan:api-spec` | Design | Interface contract from a spec |
| `plan:constraints` | Design | Hard limit from requirements |
| `code:<entity>` | Ground/Maintain | Evidence from a code entity |
| `test:path/to/test.rs` | Ground/Maintain | Evidence from a passing test |
| `manual:review` | Any | Human code review finding |
| `incident:description` | Maintain | Bug or incident post-mortem |

---

## Phase 3 — Reason in the latent space

Before touching code, test your plan on a lightweight in-memory snapshot.
Experiments load a subgraph (default 500 nodes) around a focal entity, let you
inject hypothetical changes, and evaluate the consequences — **without
modifying the real model**.

### Quick hypothesis (covers 80% of cases)

```bash
cog experiment try auth::login --kind correction \
    --claim "now accepts (user, pass, rate_limit)" \
    --grounds "hypothesis:rate-limit-feature" \
    --desc "add rate_limit parameter to login" \
    [--depends-on <id>]
# → Risk: HIGH (0.82)
# → Contradictions: api::login_handler expects 2 params
# → Blind entities: 3 downstream with no assertions
```

### Step-by-step (complex scenarios)

```bash
# 1. Start around a focal entity
cog experiment start auth::login --description "what if login takes 3 params?"

# 2. Inject hypothetical operations (Open or Evaluated — both allowed)
cog experiment hypothesize <id> --entity auth::login \
    --kind contract --claim "now accepts (user, pass, rate_limit)" \
    --grounds "hypothesis:rate-limit-feature"
cog experiment hypothetical-delete --id <id> --entity legacy_auth
cog experiment hypothetical-relation --id <id> --from a --to b --kind uses

# 3. Evaluate — simulate cascade, detect contradictions, find blind spots
cog experiment evaluate <id>

# 4. Scout: read the actual code to verify your hypothesis holds
#    (the model is a compression — it omits runtime behaviour the code has)

# 5. Adjust and re-evaluate if scouting revealed new constraints
cog experiment hypothesize <id> --entity auth::login ...
cog experiment evaluate <id>

# 6. Commit (replay to real model) or discard
cog experiment commit <id>
cog experiment discard <id>
```

### What evaluate tells you

- **Risk score** — fan-in, active assertions, fragilities, downstream count.
- **Contradictions** — same entity + same kind + different claim, or an
  assertion that would be orphaned by a hypothetical deletion.
- **Blind entities** — subgraph entities with no active assertions. These are
  your unknowns: the model has no recorded knowledge about them, so reasoning
  over them is unsupported.
- **Cascade** — if a hypothesis retracts an existing assertion, which
  dependents would go uncertain.

### The fidelity gap

The model is a **compression** of the code — it captures structure (names,
calls, containment) but not runtime behaviour (protocols, concurrency,
framework conventions). Reasoning in the latent space is fast but incomplete.
Before implementing, **scout the real code** to verify your hypothesis survives
the details the model doesn't hold. This is why phase 4 exists.

### Commit semantics

`commit` deterministically replays the staged operations to the real model:
`Assertion` → `create_assertion`, `Retraction` → `retract` + TMS cascade,
`Relation` → `add_entity_relation`, `Delete` → `delete_entity`. No diff-merge,
no UUID conflicts. After commit, the workflow moves to `PendingImplement` —
the model now describes a change the code doesn't have yet.

---

## Phase 4 — Descend to code space

Now implement the change in actual code, then reconcile the model with reality.

```bash
# 1. Implement the change in code
#    (edit files — this is outside cog)

# 2. Sync — detect structural drift between model and code
cog sync
# → if drift detected: workflow moves to PostChange
# → PendingImplement + drift → PostChange (structural changes need review)
# → PendingImplement + no drift → Exploring (sync completed)

# 3. Verify structural consistency
cog verify --scan          # stale/unmodeled entities vs actual code
cog verify                 # isolated entities, dangling grounds, etc.
cog verify --clean         # also delete isolated entities

# 4. Reconcile knowledge
cog assert <entity> --kind correction \
    --claim "login now takes rate_limit; handler updated" \
    --grounds "code:auth::login"
cog retract <stale-id> --reason "assumption invalidated by the change"
cog recover                 # restore assertions that went uncertain after
cog recover --apply         #   a cascade, if their deps are active again
```

### The learning loop

Descent is where you discover the model was incomplete or wrong. That
discovery is not waste — **record it**:

- Hit an undocumented runtime protocol the model didn't capture? →
  `assert --kind fragility` so the next agent is warned.
- Found the real root cause of a bug? → `assert --kind correction` so the
  history is preserved.
- A contract you recorded no longer holds? → `retract` it (cascades to
  dependents) and `assert` the corrected version.

This feeds directly back into Phase 2 — the model is strictly richer after
every descent, and the next cycle starts from a better base.

---

## The whole cycle, as `cog next` sees it

`cog next` tracks five workflow phases and suggests the matching action. You
don't need to memorise this — run `cog next` and it tells you:

| State | Meaning | Typical suggestion |
|-------|---------|--------------------|
| `Uninit` | No model yet | `cog sync` (Phase 1) |
| `FreshScan` | Synced, no assertions | Start recording (Phase 2) |
| `Exploring` | Recording + querying | Check impact; experiment if ready (Phase 2→3) |
| `PendingImplement` | Experiment committed, code lags | Implement now, then sync (Phase 4) |
| `PostChange` | Sync detected code drift | Record corrections; verify (Phase 4) |
| `Debugging` | Retraction triggered a cascade | Recover context; record the root cause |

State transitions are automatic: `sync` detects drift, `retract` enters
Debugging, `verify` passing exits Debugging, `experiment commit` enters
PendingImplement. No manual state commands.

Typical first run:

```bash
cog sync --init      # Phase 1: build
cog next             # → "start recording"
cog assert ...       # Phase 2: enrich
cog experiment try   # Phase 3: reason
cog experiment commit# → PendingImplement
# edit code          # Phase 4: descend
cog sync             # → PostChange (drift)
cog verify --scan    # reconcile
cog assert --kind correction  # close the loop
cog next             # → back to Exploring
```

For large-scale changes (schema migrations, layer rewrites), snapshot first:

```bash
cog backup create --name "before-refactor"
# ... changes ...
cog backup restore "before-refactor"   # if needed
cog backup drop "before-refactor"      # cleanup
```
