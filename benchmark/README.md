# cog Benchmark on Terminal-Bench 2.0

## Overview

Evaluating whether `cog` (Cognitive Model CLI) improves LLM agent performance on real-world coding tasks via controlled A/B testing in Harbor's Terminus-2 framework.

## Initial Task: `build-cython-ext`

**Rationale**: This task hits cog's sweet spot — understanding dependency relationships, locating compatibility break points, and iteratively fixing multi-file issues.

| Attribute | Value |
|-----------|-------|
| Difficulty | medium |
| Category | debugging |
| Tags | coding, dependency, compilation |
| Agent timeout | 900s |
| Description | Fix Cython extensions (chelpers, ccomplexity, cinvariants) in pyknotid v0.5.3 to work with Numpy 2.3.0, compile from source, and pass tests |

This task maps directly to cog's workflow: `cog assert` (record dependency contracts) → `cog depend` (track compat breakage) → `cog branch` (sandbox fix attempts) → `cog verify` (check structural consistency).

## Prerequisites

1. **cog binary**: `cargo build --release` (produces `target/release/cog`)
2. **Harbor CLI**: installed via `uv tool install harbor`
3. **Dataset**: already downloaded to `benchmark/terminal-bench/`
4. **API key**: `DEEPSEEK_API_KEY` set in environment

## Directory Structure

```
benchmark/
├── README.md                   # This file
├── cog_terminus.py             # Custom agent: Terminus-2 + cog binary injection
├── run_baseline.sh             # Phase 1: vanilla Terminus-2 (no cog)
├── run_cog.sh                  # Phase 2: Terminus-2 + cog binary + skills
├── cog_skills/                 # Skill files injected via --skill flag
│   └── cog/
│       ├── SKILL.md
│       ├── WORKFLOWS.md
│       ├── BRANCHING.md
│       └── BEST_PRACTICES.md
├── terminal-bench/             # Downloaded task dataset (89 tasks)
└── jobs/                       # Run results (created automatically)
```

## Running the Benchmark

### Phase 1: Baseline (no cog)

```bash
# Default: build-cython-ext
./benchmark/run_baseline.sh

# Or specify a different task
./benchmark/run_baseline.sh fix-code-vulnerability
```

### Phase 2: With cog

```bash
# Default: build-cython-ext
./benchmark/run_cog.sh

# Or specify a different task
./benchmark/run_cog.sh fix-code-vulnerability
```

### Manual invocation

```bash
# Baseline
harbor run \
    --dataset terminal-bench@2.0 \
    --agent terminus-2 \
    --model deepseek/deepseek-chat \
    --include-task-name build-cython-ext \
    --jobs-dir benchmark/jobs

# With cog
harbor run \
    --dataset terminal-bench@2.0 \
    --agent-import-path benchmark.cog_terminus:CogEquippedTerminus \
    --model deepseek/deepseek-chat \
    --include-task-name build-cython-ext \
    --skill benchmark/cog_skills/ \
    --jobs-dir benchmark/jobs
```

## How It Works

1. **CogEquippedTerminus** (in `cog_terminus.py`) inherits from Terminus-2 and overrides two things:
   - `setup()`: copies the compiled `cog` binary into the container at `/usr/local/bin/cog`
   - `run()`: inlines the cog SKILL.md content + usage guidance directly into the task instruction, so the model knows about cog from the first turn

2. **Skill injection** is done by the custom agent itself (not relying on the model to `cat` a skill file). The full SKILL.md content plus a concise "when to use cog" preamble are appended to the task instruction. Harbor's `--skill` flag is still passed to make skill files available in the container for the agent to reference if needed.

3. **Comparison**: Both runs use the same model (`deepseek/deepseek-chat`), same task, same timeout. The only variable is the presence of cog binary + inline skill instructions.

## Observation Points

After both runs complete, compare:

- **Pass/Fail**: Check `reward.txt` in job results
- **Behavior traces**: Did the cog-equipped agent use `cog assert`/`cog depend` to model the codebase?
- **Branch usage**: Did it create `cog branch` for sandbox experimentation before applying fixes?
- **Error recovery**: Did `cog retract` help correct wrong assumptions?
- **Token efficiency**: Compare trajectory lengths
