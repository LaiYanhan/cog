# Workflows

Core patterns for using cog. The workflow state machine and `cog next` handle
step-by-step guidance — these are the concepts behind the suggestions.

## Structural Scan + Semantic Deepening

`cog init` gives you structure (entity names, kinds, containment, imports).
Your job is to add semantics — the *why*, the *what-could-break*, the
*what-it-promises*. That's the irreplaceable value an LLM provides over a parser.

All auto-generated entities carry grounds `auto:scan` so you can distinguish
them from manually asserted knowledge.

```bash
cog init .                    # structural skeleton
cog next                      # see what the model needs
cog index                     # entities sorted by assertion count
```

## Progressive Grounding Lifecycle

Assertions move through four phases:

```
Design (plan:...) → Implement → Ground (code:...) → Maintain
```

### Design phase
Grounds: `plan:design-doc`, `plan:api-spec`, `spec:requirements`

Use when no code exists. Keep assertions coarse — module boundaries,
key interfaces, hard constraints.

### Implementation phase
No model changes needed. Code the assertions as a checklist.

### Grounding phase
Replace `plan:...` grounds with `code:<entity>`. Retract design assumptions
that turned out wrong. Assert new knowledge discovered during implementation.

### Maintenance phase
Update grounds when entities are renamed or removed (`verify --scan` detects this).
Retract fragility after fixing the root cause. Assert corrections when fixing bugs.

### Grounds reference

| Grounds pattern | Phase | Example |
|----------------|-------|---------|
| `plan:design-doc` | Design | Architecture decision before code exists |
| `plan:api-spec` | Design | Interface contract from a spec |
| `plan:constraints` | Design | Hard limit from requirements |
| `code:<entity>` | Ground/Maintain | Evidence from a code entity identified by qualified name |
| `test:path/to/test.rs` | Ground/Maintain | Evidence from passing test |
| `manual:review` | Any | Human code review finding |
| `incident:description` | Maintain | Bug or incident post-mortem |
