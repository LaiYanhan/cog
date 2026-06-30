# Benchmark Target: What We're Validating

## The Hypothesis

LLM coding agents lack persistent memory. Every time an agent revisits the same codebase under a new requirement, it re-reads and re-interprets the code from scratch. This causes:

1. **Understanding drift** — the same function is interpreted differently across sessions
2. **Lost intent** — design decisions, constraints, and invariants from previous work are forgotten
3. **Accidental regression** — changes break previously correct behavior because the agent didn't know what was already guaranteed
4. **Structural erosion** — each modification adds patches rather than extending the architecture, because the agent can't see the whole picture

`cog` provides an external cognitive model that persists across sessions. It lets agents:

- Record contracts, invariants, and fragility points via `cog assert`
- Query structural relationships via `cog query` / `cog impact` / `cog trace`
- Verify consistency via `cog verify`

The hypothesis: **an agent equipped with cog will maintain more stable understanding of a codebase over multiple rounds of modification, producing fewer regressions and less structural degradation than the same agent without cog.**

## What We Need to Measure

| Metric | What it captures | How cog helps |
|--------|-----------------|---------------|
| **Regression rate** | Fraction of modifications that break previously passing tests | `cog assert` records contracts; `cog impact` shows blast radius before editing |
| **Structural erosion** | Growth in code complexity concentration over iterations | `cog assert --kind invariant` records architectural constraints |
| **Understanding consistency** | Variance in agent's interpretation of the same code across sessions | `cog query` provides stable entity descriptions independent of session context |
| **Token efficiency** | Tokens spent per successful modification | `cog query` / `cog impact` replaces ad-hoc file reading; reasoning in the cognitive latent space is cheaper than re-reading source |
| **Task completion** | Fraction of sequential tasks completed without failing earlier ones | All of the above compound |

## What Terminal-Bench Measures (And Why It's Insufficient)

Terminal-Bench is **snapshot-based, single-shot**: agent sees a codebase once, solves one problem, done. This tests:

- Can the agent solve isolated coding tasks? ✓
- Does cog's structural model help? — **Cannot tell**

It cannot tell because:

1. **No iteration** — agent never revisits its own work under changed requirements
2. **No accumulation** — no technical debt builds up over time
3. **No memory pressure** — agent doesn't need to remember what it did 3 sessions ago
4. **No regression risk** — nothing to break because there's only one task

**Our Terminal-Bench results confirm this**: across 16 baseline+cog pairs (Round 1: 12, Round 2: 4 with mandatory workflow), cog produced 0 wins, 3 losses, 13 ties. The agent correctly used cog when forced by prompt design (Round 2), but cog provided zero measurable benefit because the tasks didn't require persistent understanding.

## Recommended Datasets

### Tier 1: Directly Validates Our Hypothesis

| Dataset | Paradigm | Key Metric | Why it fits |
|---------|----------|------------|-------------|
| **SWE-CI** (arXiv 2603.03823) | Evolution-based: up to 20 CI iterations on the same repo | Zero-regression rate | Agent must maintain a real codebase across 71+ commits over 233 days. `cog assert` records contracts from prior fixes; `cog impact` prevents regressions. 75%+ of current agents introduce regressions — this is exactly the gap cog targets. |
| **SlopCodeBench** (arXiv 2603.24755) | Iterative: agent extends its own prior code across 3-8 checkpoints | Verbosity + structural erosion | 93 checkpoints across 20 problems. All models show monotonic quality decay. `cog assert --kind invariant` can encode architectural decisions from early checkpoints; `cog verify` catches drift. Prompt interventions fail to halt degradation — cog offers a structural solution. |
| **BeyondSWE** (arXiv 2603.03194) | Sequential evolution: multi-repo, multi-task | Cross-repo consistency | Tasks span multiple repositories. `cog trace` and `cog impact` across repo boundaries. |

### Tier 2: Validates Specific Capabilities

| Dataset | What it tests | Relevance |
|---------|--------------|-----------|
| **CodeScaleBench** (Sourcegraph) | Agent performance on >400K LOC codebases | "grep stops working at scale" — `cog query` / `cog impact` provide structural navigation. Baseline: 96 tool calls / 84min → 5 calls / 5min with proper tooling. |
| **SWE-bench Verified** | Single-issue fix on real repos | Standard baseline for coding agents. Useful as a control: if cog hurts single-shot performance, that's a cost we need to quantify. |
| **FeatBench** (arXiv 2509.22237) | Feature implementation in existing repos | Agent adds features to real codebases. `cog impact` shows where to hook in without breaking existing functionality. |

### Tier 3: Build Our Own (If No Dataset Available)

If SWE-CI or SlopCodeBench harnesses are not publicly runnable, construct a minimal version:

1. Pick a real mid-size project (cog itself, or a 5K-50K LOC open-source project)
2. Design 8-10 sequential modification tasks on the same codebase
3. Each task depends on code written/modified in previous tasks
4. Measure: regression rate, structural metrics, token cost per task
5. Run baseline (no cog) vs cog-equipped for the full sequence

## Recommended Evaluation Protocol

```
For each task sequence:
  1. Agent starts with fresh codebase (git clone / COPY)
  2. Task 1: make modification, run tests
  3. cog-equipped agent: cog init . + cog assert contracts before editing
  4. Task 2-8: each task modifies the codebase further
  5. After each task: run ALL tests (including from previous tasks)
  6. Measure: pass rate on old tests, pass rate on new tests, code metrics

Compare:
  - Regression rate: % of tasks where agent broke a previously passing test
  - Completion rate: % of tasks completed successfully in sequence
  - Code quality: verbosity, cyclomatic complexity growth per checkpoint
  - Token cost: total tokens consumed across the full sequence
```

## Current Status

- [x] Terminal-Bench Round 1: 12 tasks, zero cog usage (prompt design issue)
- [x] Terminal-Bench Round 2: 4 tasks with mandatory workflow, agent uses cog but no performance gain
- [x] Root cause identified: single-shot paradigm cannot validate cog's value
- [ ] Evaluate SWE-CI / SlopCodeBench harness availability
- [ ] Design multi-iteration test protocol
- [ ] Run first multi-iteration benchmark
