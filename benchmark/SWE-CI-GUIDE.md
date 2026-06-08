# SWE-CI Benchmark Guide: Testing Cog on Long-Horizon Code Evolution

## Overview

SWE-CI (arXiv 2603.03823) is the primary benchmark for validating cog's value proposition: **an agent with persistent cognitive model maintains code better across iterations than the same agent without it**.

Key property: SWE-CI runs up to 20 CI iterations per task, tracking test pass/fail across the full sequence. This exposes regression — exactly what cog targets.

Official repo: https://github.com/SKYLENAGE-AI/SWE-CI
HuggingFace: https://huggingface.co/datasets/skylenage-ai/SWE-CI
Paper: https://arxiv.org/pdf/2603.03823

---

## Q&A: Pre-Run Analysis

### Q1: Can I use `uv` instead of `conda`?

**Yes, fully compatible.** The SWE-CI project only requires:
1. Python 3.11
2. `pip install -r requirements.txt`

The dependencies are standard PyPI packages (`huggingface_hub`, `Jinja2`, `jsonargparse`, `tomlkit`, etc.) — nothing conda-specific. Steps:

```bash
# this should run in the benchmark folder
cd benchmark && git clone https://github.com/SKYLENAGE-AI/SWE-CI.git
cd SWE-CI

uv venv --python 3.11 .venv
source .venv/bin/activate
uv pip install -r requirements.txt
```

Alternatively, `uv pip install -r requirements.txt` inside any Python 3.11 venv works.

---

### Q2: Can I selectively download tasks instead of the full ~50GB dataset?

**Partially — the harness doesn't support selective download natively, but there are practical workarounds.**

#### How the download works (source: `swe_ci/download.py`)

```python
def download_dataset():
    metadata_path = download_file(..., f"metadata/{CONFIG.splitting}.csv", ...)
    metadata = read_csv(metadata_path)
    task_ids = [task['task_id'] for task in metadata]
    for task_id in task_ids:
        download_hf_folder(CONFIG.hf_repo_id, f"data/{task_id}", ...)
```

It downloads the metadata CSV first, then iterates ALL task IDs in that CSV. No skip/filter mechanism.

#### Available splits (from HuggingFace `metadata/` directory)

| Split | Tasks | Notes |
|-------|-------|-------|
| `lite` | ~40 | Lighter subset — **best starting point** |
| `default` | 100 | Original benchmark split |
| `default_v2` | 100 | Harder tasks, one per repo |
| `full` | 137 | Complete dataset |

**Per-task size**: Each task = `code.zip` (source code) + `image.tar.gz` (Docker environment image). The images dominate size. Rough estimate: ~350MB–500MB per task.

#### Workarounds

**Option A: Use the `lite` split** (~40 tasks, ~15GB). Best first run.

**Option B: Create a custom split CSV.** The metadata format is simple CSV with 9 columns. Steps:
1. Download only the metadata: `metadata/lite.csv` (~17KB)
2. Hand-pick tasks (e.g., 3–5 tasks with diverse `test_gap` values)
3. Create `metadata/my_split.csv` with only those rows
4. Set `splitting = "my_split"` in config or via `--splitting my_split`
5. Run download — it will only download data for tasks in that CSV

**Option C: Interrupt and resume.** The download iterates sequentially. Ctrl+C after enough tasks are downloaded. Then create a CSV matching only the downloaded tasks. The harness checks for `code.zip` + `image.tar.gz` existence per task, so missing tasks will fail initialization — filter them out of the CSV.

**Recommended approach**: Start with Option B — pick 3–5 tasks for a pilot run. This costs ~2GB and validates the full pipeline in ~2 hours instead of 48.

---

### Q3: Can the two phases (initialization + evolution) run on a subset of tasks?

**Yes — the harness is fully task-granular.** Both phases iterate the metadata CSV and process tasks independently.

#### Initialization phase (`init_tasks`)

- Reads metadata CSV → processes each task in parallel (`ProcessPoolExecutor`, `max_workers=16`)
- Per task: extracts code, runs pytest on current and target versions, generates test gap
- **Already-initialized tasks are skipped** (checks for `iteration.jsonl`)
- Failed tasks don't block others — just logged

#### Evolution phase (`run_tasks`)

- Reads **same** metadata CSV → processes each task in parallel
- Per task: runs up to `evolve.max_epoch` iterations of architect→programmer→pytest cycle
- **Already-completed tasks are skipped** (checks `gap == 0` or `current_epoch >= max_epoch`)
- Uses file locks per task to prevent concurrent execution

#### Implications

1. **To run a subset**: create a CSV with only desired task IDs → both phases only touch those tasks
2. **To add tasks later**: append rows to CSV → re-run → new tasks initialized/evolved, existing tasks skipped
3. **To re-run a specific task**: delete its directory under `experiments/<name>/<task_id>/` → re-run
4. **To reduce epochs**: set `evolve.max_epoch = 5` (or any value < 20) in config.toml

No hardcoded dependency on full dataset. The only requirement is that the metadata CSV entries have matching data files downloaded.

---

### Q4: What are the "two agents"? Are they based on opencode?

**The "dual-agent" is a prompt-level architecture, not two independent frameworks.**

#### How it actually works

```
┌──────────────────────────────────────────────────────┐
│  For each CI iteration:                              │
│                                                      │
│  1. Run pytest → get test failures                   │
│  2. Call CLI agent (opencode) with ARCHITECT prompt  │
│     → agent writes /app/requirement.xml              │
│  3. Call CLI agent (opencode) with PROGRAMMER prompt │
│     → agent modifies /app/code/                      │
│  4. Run pytest → measure gap change                  │
│  5. Repeat up to max_epoch times                     │
└──────────────────────────────────────────────────────┘
```

Both "agents" are the **same opencode CLI binary** (`opencode run --model <model> <prompt>`) invoked twice per iteration with different prompts:

- **Architect agent**: receives `prompt.jinja2` with `role='architect'` — reads test failures, produces a requirement document in XML
- **Programmer agent**: receives `prompt.jinja2` with `role='programmer'` — reads requirement.xml, modifies source code

The LLM model is configured via `api_key` / `base_url` / `model_name` in config.toml. The opencode CLI handles the LLM API calls, tool use (file reading, editing), and session management.

#### What opencode is

[opencode](https://www.npmjs.com/package/opencode-ai) is an open-source CLI coding agent (Node.js, installed via npm). It:
- Connects to an LLM API (OpenAI-compatible protocol)
- Runs inside a Docker container with the codebase mounted at `/app`
- Has file system tools (read, write, edit)
- Reports token usage via a local SQLite DB (`opencode.db`)

The alternative agent option is `iflow` — another CLI agent with similar capabilities. Both are thin wrappers around LLM API calls with tool-use capabilities.

The npm packages of agent are installed into docker container, the host should not install them.

#### Key implication for cog testing

Since both "agents" are just the same LLM with different prompts, **cog must be injected once** into the Docker container. The prompts can be modified to instruct both roles to use cog:

- **Architect**: `cog init .` → `cog assert` contracts from test analysis → `cog impact` before writing requirements
- **Programmer**: `cog query` to understand code → `cog assert --kind invariant` before modifying → `cog verify` after changes

---

### Q5: How to embed cog, ensure agent usage, and test feasibility cheaply?

#### 5.0 Why this problem is fundamentally different from Terminal-Bench

Terminal-Bench failed because cog was positioned as "an optional tool you might find useful." The agent had no reason to use it — single-shot tasks don't need persistent memory.

SWE-CI is different in a critical way: **the CI loop creates the exact problem cog was designed to solve**. Each task runs up to 20 iterations of test→analyze→modify. The agent must maintain a codebase it modified in previous iterations without forgetting what it learned. This is cognitive persistence, not a nice-to-have.

The strategy shift: **don't force cog as an external tool — embed it as the agent's memory system that naturally accumulates across iterations**.

#### 5.1 Embedding cog into the Docker container

The Docker image is built in two stages:

1. **Base image** (`image.tar.gz` from HuggingFace): contains OS + project dependencies (Python, pytest, etc.)
2. **Agent layer** (`Dockerfile.opencode`): installs Node.js + opencode on top of base

Cog must be added as a third layer:

```dockerfile
# Dockerfile.opencode-cog
ARG BASE_IMAGE
FROM ${BASE_IMAGE}

# ---- Install opencode (same as original) ----
ARG NODE_VERSION=22.18.0
ARG AGENT_NPM_PKG=opencode-ai
ARG AGENT_BIN=opencode
ARG AGENT_HOME=/opt/agent

RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl xz-utils tar dpkg git; \
    arch="$(dpkg --print-architecture)"; \
    case "$arch" in amd64) node_arch="x64" ;; arm64) node_arch="arm64" ;; *) echo "unsupported arch: $arch" >&2; exit 1 ;; esac; \
    mkdir -p "${AGENT_HOME}/node" "${AGENT_HOME}/npm-global" "${AGENT_HOME}/npm-cache" "${AGENT_HOME}/home" "${AGENT_HOME}/cache"; \
    curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" -o /tmp/node.tar.xz; \
    tar -xJf /tmp/node.tar.xz -C "${AGENT_HOME}/node" --strip-components=1; \
    rm /tmp/node.tar.xz; \
    export NPM_CONFIG_PREFIX="${AGENT_HOME}/npm-global"; export npm_config_cache="${AGENT_HOME}/npm-cache"; export PATH="${AGENT_HOME}/node/bin:$PATH"; \
    "${AGENT_HOME}/node/bin/npm" install -g "${AGENT_NPM_PKG}"; \
    test -x "${AGENT_HOME}/npm-global/bin/${AGENT_BIN}"; \
    rm -rf "${AGENT_HOME}/npm-cache"/*; rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    printf '#!/bin/sh\nset -eu\nBASE="%s"\nexport NODE_NO_WARNINGS=1\nexport HOME="$BASE/home"\nexport XDG_CACHE_HOME="$BASE/cache"\nexport NPM_CONFIG_PREFIX="$BASE/npm-global"\nexport npm_config_cache="$BASE/npm-cache"\nexport PATH="$BASE/node/bin:$BASE/npm-global/bin:$PATH"\nexec "$BASE/npm-global/bin/%s" "$@"\n' \
        "${AGENT_HOME}" "${AGENT_BIN}" > "/usr/local/bin/${AGENT_BIN}"; \
    chmod +x "/usr/local/bin/${AGENT_BIN}"; chmod -R 777 "${AGENT_HOME}"

# ---- Install cog ----
COPY cog /usr/local/bin/cog
RUN chmod +x /usr/local/bin/cog && cog --version

WORKDIR /app
```

**Build steps:**
1. `cargo build --release` → produces `target/release/cog` (single static binary, rusqlite bundled)
2. Copy binary to SWE-CI repo root as `cog`
3. Build uses this Dockerfile instead of `Dockerfile.opencode`

No runtime dependencies needed — cog is fully self-contained.

#### 5.2 Persistence: how the cog DB survives across iterations

This is the critical architectural question. In SWE-CI, each iteration creates fresh containers. But the cog DB persists naturally because of how the harness archives code:

```
Iteration N:
  1. Architect container: code copied in → architect runs → requirement.xml copied out → container destroyed
  2. Programmer container: code copied in → programmer modifies code → entire /app/code/ copied out → container destroyed
  3. If epoch succeeds:
     - current_dir/ renamed to archive/<timestamp>/
     - tmp_dir/ renamed to current_dir/

Persistence chain:
  /app/code/.cog/cog.db  →  tmp_dir/code/.cog/cog.db  →  current_dir/code/.cog/cog.db
  (programmer's container)    (copied back from container)   (becomes next iteration's input)
```

The `.cog/` directory lives inside the code directory and travels with it. No harness modification needed.

**Key constraint**: Only the PROGRAMMER's cog writes persist. The architect runs in a separate container; only `requirement.xml` is extracted. So:

| Role | cog reads | cog writes |
|------|-----------|------------|
| Architect | `cog query`, `cog impact` (can read prior knowledge) | **Not recommended** — writes lost when container destroyed |
| Programmer | `cog query`, `cog impact`, `cog trace`, `cog next` | `cog init`, `cog assert`, `cog retract`, `cog verify` — all persist |

#### 5.3 Prompt design: cog as the agent's memory system

The prompt must NOT present cog as "an extra tool to use." It must frame cog as the agent's **persistent cognitive model** that accumulates understanding across CI iterations.

##### Core design principles

1. **`cog next` is the primary entry point.** Don't enumerate all cog commands in the prompt — `cog next` already knows what the model needs and suggests appropriate actions. The agent calls `cog next`, follows the suggestion, and the workflow state machine handles the rest.

2. **Two-layer value.** The first `cog init /app/code/` builds the structural layer (entities, containment, imports) at zero LLM cost via tree-sitter. The programmer then adds the understanding layer (contracts, invariants, fragility) through `cog assert`. Both layers compound across iterations.

3. **Knowledge accumulates.** Iteration 1's `cog assert` contracts are available in iteration 5's `cog query`. This is the core value — the agent "remembers" what it learned, not just what it sees.

4. **`cog impact` prevents regressions.** Before modifying any entity, `cog impact <entity>` shows the blast radius. The agent sees what depends on the entity it's about to change and can avoid breaking downstream contracts.

##### Programmer prompt design (implemented in prompt.jinja2)

The prompt.jinja2 uses `{% if cog %}` to branch between baseline and cog variants. The cog variant adds two mandatory steps (0 and 4) that bookend the existing workflow. The baseline variant renders the original upstream prompt unchanged. The switching is automatic via `cog = true` in config_cog.toml.

The design intent for the cog variant:

```xml
<step index="0" action="cognitive_model">
    Before reading requirements, check the cognitive model for this codebase.
    The cognitive model is your PERSISTENT MEMORY that accumulates across CI iterations.
    It remembers what you learned about the code in previous iterations so you don't
    have to re-derive everything from scratch.

    1. If /app/code/.cog/ does not exist, run: cog init /app/code/
       This scans the codebase structure (functions, classes, modules) using tree-sitter.
    2. Run: cog next
       This tells you what the model needs based on its current state.
       Follow the suggestion — it knows which entities lack assertions.
</step>

<!-- Existing step 1 (read_requirements) unchanged -->

<step index="2" action="inspect_code">
    Based on the requirement list, read the relevant code files in /app/code/.
    For each key entity you encounter, check the cognitive model:
    - Run: cog query <entity> to see existing contracts and prior knowledge.
    - If the entity has no assertions, record what you learn:
      cog assert <entity> --kind contract --claim "<what it promises callers>" --grounds "code:<entity>"
    Only record non-obvious knowledge: contracts, invariants, design intent, known risks.
    Do NOT record what the code obviously does — that's the code's job.
    If necessary, consult the relevant test cases in /app/code/tests/.
</step>

<step index="3" action="implement">
    Based on the requirements and the current state of the code, plan and implement changes.
    Before modifying any entity, check the blast radius:
    - Run: cog impact <entity> to see what depends on it.
    - If downstream entities have active contracts, ensure your changes respect them.
    After implementing, verify and update the cognitive model:
    - Run: cog verify --scope <modified_entity> to check structural consistency.
    - If your changes break a prior assertion, retract it:
      cog retract <id> --reason "behavior changed in iteration N"
    - Record new knowledge discovered during implementation:
      cog assert <entity> --kind correction --claim "<what changed and why>" --grounds "code:<entity>"
</step>
```

**Why this works better than Terminal-Bench's approach:**

- **Not optional**: cog is embedded in the numbered `<step>` workflow, same as "read requirements" and "implement"
- **`cog next` is self-guiding**: the agent doesn't need to remember *which* cog command to run — `cog next` tells it
- **Accumulation is the value**: each iteration's `cog assert` builds on prior ones, creating a growing knowledge base
- **`cog impact` directly targets SWE-CI's regression problem**: the agent sees downstream dependencies before modifying

##### What NOT to do in the prompt

Based on lessons from Terminal-Bench Round 2 (where mandatory workflow produced cog usage but zero performance gain):

1. **Don't over-prescribe cog commands.** Let `cog next` guide usage. Over-detailed instructions cause the agent to mechanically execute steps without understanding why.
2. **Don't assert on everything.** The cog best practices are clear: only assert where "if this assumption changes, would it cause a hard-to-find bug?" Internal helpers, boilerplate, and obvious patterns don't need assertions.
3. **Don't put cog in the architect prompt's write path.** The architect container's state is discarded. Only read-only cog commands make sense for the architect.
4. **Don't measure success by cog call count.** A single `cog impact` that prevents a regression is worth more than 20 `cog assert` calls on trivial code.

#### 5.4 Low-cost feasibility test protocol

The goal is to validate **three specific properties** before committing to the full run:

1. **Persistence**: Does the cog DB actually survive across iterations?
2. **Usage**: Does the agent make meaningful cog calls (not just `cog init` once)?
3. **Accumulation**: Does the model grow richer over iterations (more assertions, more entities)?

##### Phase 0: Smoke Test — Persistence + Basic Usage (~30 min)

Pick: `igrek51__wat__ecddda__8efafa` (test_gap=5, smallest task in lite split)

```toml
# config_smoke.toml
experiment_name = "cog-smoke"
splitting = "smoke"
agent_name = "opencode"
hf_token = "none"
hf_repo_id = "skylenage-ai/SWE-CI"
save_root_dir = "."

# ✏️ LLM API credentials — fill in before running
api_key = "sk-ddcc430a74da4867a9bbd05f1af92fee"
base_url = "https://api.deepseek.com"
model_name = "deepseek-chat"

mode = "tdd"

[agent]
node_version = "22.18.0"
npm_pkg = "opencode-ai"
npm_bin = "opencode"
dockerfile = "Dockerfile.opencode-cog"

[init]
max_workers = 1

[evolve]
max_epoch = 2
max_workers = 1

[evolve.architect]
timeout = 3600
max_try = 10

[evolve.programmer]
timeout = 3600
max_try = 10

[docker]
storage_disk = ""
cpus = ""
read_bps = "128mb"
write_bps = "128mb"
memory = "8192mb"
memory_reservation = "4096mb"

[pytest]
timeout = 3600

Run, then verify:

```bash
# 1. Check the archived iterations have cog DBs
ls experiments/cog-smoke/<task_id>/2026-*/code/.cog/cog.db
# Should see at least one archived snapshot with .cog/cog.db

# 2. Check the current iteration has a cog DB
ls experiments/cog-smoke/<task_id>/current/code/.cog/cog.db

# 3. Inspect the model state
cog stats --db experiments/cog-smoke/<task_id>/current/code/.cog/cog.db
# Should show entities from cog init + any assertions the agent added

# 4. Check task log for cog commands
grep "cog " experiments/cog-smoke/<task_id>/task.log | head -20
# Should see: cog init, cog next, cog query, cog assert, cog impact, cog verify
```

**Go/No-Go criteria for Phase 0:**
- ✅ `.cog/cog.db` exists in the final `current/code/` directory
- ✅ Task log shows ≥3 distinct cog command types (not just `cog init`)
- ✅ `cog stats` shows entities (from init) and ≥1 assertion (from agent)
- ❌ If any of these fail → iterate on prompt design before proceeding

##### Phase 1: Accumulation Test — Does Knowledge Actually Grow? (~2-3 hours)

Pick 3 tasks with different complexity:

| Task | test_gap | Why |
|------|----------|-----|
| `igrek51__wat__ecddda__8efafa` | 5 | Small — agent should be able to assert on most entities |
| `eliben__pycparser__7f6b34__6ba954` | 8 | Classic parser, well-structured modules |
| `15r10nk__inline-snapshot__3bb05d__e2b9b2` | 16 | Moderate — enough scope for multi-iteration accumulation |

```toml
# config_pilot.toml (Phase 1 — same as config_cog_pilot.toml)
experiment_name = "cog-pilot-v1"
splitting = "pilot"
agent_name = "opencode"
hf_token = "none"
hf_repo_id = "skylenage-ai/SWE-CI"
save_root_dir = "."

# ✏️ LLM API credentials — fill in before running
api_key = "sk-ddcc430a74da4867a9bbd05f1af92fee"
base_url = "https://api.deepseek.com"
model_name = "deepseek-chat"

mode = "tdd"

[agent]
node_version = "22.18.0"
npm_pkg = "opencode-ai"
npm_bin = "opencode"
dockerfile = "Dockerfile.opencode-cog"

[init]
max_workers = 2

[evolve]
max_epoch = 5
max_workers = 2

[evolve.architect]
timeout = 3600
max_try = 10

[evolve.programmer]
timeout = 3600
max_try = 10

[docker]
storage_disk = ""
cpus = ""
read_bps = "128mb"
write_bps = "128mb"
memory = "8192mb"
memory_reservation = "4096mb"

[pytest]
timeout = 3600

After the run, inspect knowledge accumulation across iterations:

```bash
# For each archived iteration snapshot, check model growth
for dir in experiments/cog-pilot-v1/<task_id>/2026-*/; do
    if [ -f "$dir/code/.cog/cog.db" ]; then
        echo "=== $(basename $dir) ==="
        cog stats --db "$dir/code/.cog/cog.db"
        echo "Assertions:"
        cog index --db "$dir/code/.cog/cog.db" | head -10
    fi
done

# Final state
cog stats --db experiments/cog-pilot-v1/<task_id>/current/code/.cog/cog.db
cog index --db experiments/cog-pilot-v1/<task_id>/current/code/.cog/cog.db
```

**Go/No-Go criteria for Phase 1:**
- ✅ Assertion count grows across iterations (iteration 5 > iteration 1)
- ✅ `cog query <core_entity>` returns meaningful contracts in later iterations
- ✅ Agent used `cog impact` at least once per iteration (grep the log)
- ✅ Zero-regression rate comparable to or better than naive expectation
- ❌ If assertion count stays flat → prompt not driving accumulation, redesign
- ❌ If agent never uses `cog impact` → the "prevent regression" pathway isn't active

##### Phase 2: A/B Comparison on lite Split (~15 hours per arm)

Only after Phase 1 confirms accumulation. Run the `lite` split twice:

1. **Baseline**: unmodified SWE-CI, standard opencode agent, no cog
2. **Treatment**: cog-enabled opencode agent with modified prompt

Compare:

| Metric | Source | Expected cog effect |
|--------|--------|-------------------|
| ANC (Average Normalized Change) | `iteration.jsonl` per task | Primary metric — cog should improve this |
| Zero-regression rate | Check if `passed` count ever decreases between iterations | **This is the core hypothesis** — cog should reduce regressions via `cog impact` |
| Resolution rate | Final `gap == 0` per task | Secondary — cog might help or might slow down |
| Token cost | `iteration.jsonl` architect/programmer token fields | Expected ~5-10% overhead from cog calls |

##### Phase 3: Full Benchmark (default split, ~48 hours per arm)

Only if Phase 2 shows positive signal on zero-regression rate or ANC.

#### 5.5 Verifying cog usage quality (not just quantity)

High cog call count doesn't mean high value. Check for these quality signals:

**Good signals:**
- `cog impact <entity>` calls BEFORE code modifications (not after)
- `cog assert --kind contract` on public interfaces, not internal helpers
- `cog retract` with meaningful reasons when behavior intentionally changes
- `cog query` at the START of an iteration (leveraging prior knowledge)
- Assertion count growing steadily (each iteration adds new understanding)

**Bad signals:**
- Only `cog init` once, never `cog query` or `cog assert` after
- `cog assert` on every entity indiscriminately (over-modeling anti-pattern)
- All `cog impact` calls AFTER modifications (too late to prevent regression)
- Assertion count flat across iterations (no accumulation)
- All assertions are `fragility` or `intent`, none are `contract` or `invariant` (missing the high-value kinds)

#### 5.6 Harness modification: trajectory recording (REQUIRED)

The default SWE-CI harness **does not save agent trajectories**. The `opencode.py` module copies the `opencode.db` (SQLite) from the container to a temp directory, extracts only token counts, then discards the DB. The full message history — prompts, responses, tool calls (including cog invocations) — is lost.

**Three files modified** (in the local SWE-CI clone):

1. **`src/swe_ci/benchmark/agents/opencode.py`**:
   - Added `shutil` import
   - `valid_and_parse()`: new `save_trajectory_to` parameter — copies `opencode.db` + WAL/SHM + stdout/stderr to the specified directory before the temp dir is cleaned up
   - `call_opencode()`: new `save_trajectory_to` parameter — passed through to `valid_and_parse`

2. **`src/swe_ci/benchmark/tools.py`**:
   - `call_cli_agent()`: new `save_trajectory_to` parameter — forwarded to the agent function (opencode only)

3. **`src/swe_ci/benchmark/run.py`**:
   - Evolution loop: creates `task_dir / "trajectories" / "epoch_NNN" / "architect"` and `"programmer"` directories
   - Passes these to `call_cli_agent` for both architect and programmer calls

**Result**: After each run, `trajectories/` contains the full session data for analysis:

```
<task_id>/
  trajectories/
    epoch_001/
      architect/
        opencode.db          # Full session DB with all messages & tool calls
        stdout.log           # opencode CLI stdout
        stderr.log           # opencode CLI stderr
      programmer/
        opencode.db
        stdout.log
        stderr.log
    epoch_002/
      ...
```

The `opencode.db` SQLite contains `session` and `message` tables. Each message has a `data` JSON column with role, content, tokens, and tool invocation details. The evaluation script parses these to extract tool call counts and cog command usage.
---

## Implementation Plan

### Step 1: Environment Setup

```bash
# Clone SWE-CI, run in benchmark folder
cd benchmark && git clone https://github.com/SKYLENAGE-AI/SWE-CI.git
cd ~/SWE-CI

# Python environment with uv
uv venv --python 3.11 .venv
source .venv/bin/activate
uv pip install -r requirements.txt
```

### Step 2: Build cog binary for Linux

```bash
cd ~/GitHub/cog
cargo build --release
# Produces target/release/cog
```

### Step 3: Select pilot tasks

From `lite.csv`, pick 3 tasks:

| Priority | Task | test_gap | Reason |
|----------|------|----------|--------|
| 1 | `igrek51__wat__ecddda__8efafa` | 5 | Smallest gap — quick validation |
| 2 | `eliben__pycparser__7f6b34__6ba954` | 8 | Classic parser project, well-structured |
| 3 | `15r10nk__inline-snapshot__3bb05d__e2b9b2` | 16 | Moderate complexity |

Create `metadata/pilot.csv` with these 3 rows.

### Step 4: Apply harness patches (REQUIRED)
All patches live in the local SWE-CI clone. They are **non-destructive** — the baseline path works exactly like upstream SWE-CI.
#### Patch 1: Make `dockerfile` configurable (`config.py`)
**File**: `src/swe_ci/config.py`, line 128
```python
# Before:
cfg.agent.dockerfile = str(agent_dir / "Dockerfile.opencode")

# After:
cfg.agent.dockerfile = getattr(cfg.agent, "dockerfile", None) or str(agent_dir / "Dockerfile.opencode")
```
#### Patch 2: Accept custom splits (`download.py`)
**File**: `src/swe_ci/download.py`, lines 59–72
Wrap the HuggingFace validation in try/except and check for local CSV first. Custom splits (like `pilot`) only exist locally.
#### Patch 3: Trajectory recording (`opencode.py`, `tools.py`, `run.py`)
Saves full `opencode.db` + stdout/stderr per epoch per role. See §5.6 for details. This patch is **harmless for baseline** — trajectories are saved regardless of cog.
#### Patch 4: Prompt cog flag (`run.py`)
Passes `cog` config flag to the Jinja2 template so the prompt automatically selects baseline or cog variant:
```python
cog_enabled = getattr(CONFIG, "cog", False)
programmer_prompt = load_prompt(prompt_file, template_args = {..., "cog": cog_enabled})
```
### Step 5: Place cog binary + Dockerfiles
```bash
# Copy cog binary to SWE-CI repo root (Dockerfile.opencode-cog COPYs it)
cp ~/GitHub/cog/target/release/cog benchmark/SWE-CI/cog
```
Two Dockerfiles exist in `src/swe_ci/benchmark/agents/`:
- `Dockerfile.opencode` — **original** upstream, no cog (baseline uses this)
- `Dockerfile.opencode-cog` — adds `COPY cog /usr/local/bin/cog` layer (cog uses this)
### Step 6: Download task data
```bash
cd benchmark/SWE-CI
source .venv/bin/activate

# Download only pilot tasks (reads metadata/pilot.csv)
PYTHONPATH=src python -m swe_ci.download --config_file config_cog.toml
```
### Step 7: Run experiments
Two config files control the two paths:
| Config | `cog` | Dockerfile | Prompt | Experiment name |
|--------|-------|------------|--------|-----------------|
| `config_baseline.toml` | absent (false) | `Dockerfile.opencode` (original) | Original programmer prompt | `baseline-pilot-v1` |
| `config_cog.toml` | `true` | `Dockerfile.opencode-cog` | Cog-enabled programmer prompt | `cog-pilot-v1` |
```bash
# Run baseline (no cog)
PYTHONPATH=src python -u -m swe_ci.evaluate --config_file config_baseline.toml

# Run cog experiment
PYTHONPATH=src python -u -m swe_ci.evaluate --config_file config_cog.toml
```
Or use the helper script:
```bash
./run_experiment.sh baseline    # baseline only
./run_experiment.sh cog         # cog only
./run_experiment.sh both        # sequential: baseline then cog
```
### Step 8: Compare results
```bash
# Analyze each experiment
python ../../evaluate_swe_ci.py experiments/baseline-pilot-v1
python ../../evaluate_swe_ci.py experiments/cog-pilot-v1
```
---

## Key Metrics to Track

Rather than writing ad-hoc shell scripts each time, create a reusable Python evaluation script early (e.g., `benchmark/evaluate_swe_ci.py`). The sections below define what to extract; the script should parse `iteration.jsonl`, archived snapshots, and task logs to produce a single summary report.

### Primary metrics (from SWE-CI output)

| Metric | Source | What it tells us |
|--------|--------|-----------------|
| ANC (Average Normalized Change) | `iteration.jsonl` per task | Primary SWE-CI metric — does cog improve it? |
| Zero-regression rate | Check if `passed` count ever decreases between iterations | **Core hypothesis**: cog should reduce regressions via `cog impact` |
| Resolution rate | Final `gap == 0` per task | Does cog help complete more tasks? |
| Token cost | `iteration.jsonl` architect/programmer token fields | Expected ~5-10% overhead from cog calls |

### Cog-specific metrics (from post-run analysis)

| Metric | Source | What it tells us |
|--------|--------|-----------------|
| Persistence rate | Archived `.cog/cog.db` across iterations | Does the DB actually survive? |
| Assertion growth | `cog stats --db <path>` per archived snapshot | Is knowledge accumulating? |
| Assertion kind distribution | `cog index --db <path>` per archived snapshot | Are high-value kinds (contract, invariant) represented? |
| `cog impact` timing | `task.log` — before vs after code modifications | Is impact used proactively? |
| `cog next` usage | `task.log` grep | Is the agent using the guidance system? |

### Evaluation script

`benchmark/evaluate_swe_ci.py` — run after each experiment phase:

```bash
# Basic analysis
python benchmark/evaluate_swe_ci.py benchmark/SWE-CI/experiments/cog-pilot-v1

# With cog binary for detailed stats
python benchmark/evaluate_swe_ci.py benchmark/SWE-CI/experiments/cog-pilot-v1 --cog-bin ./target/release/cog

# Summary table only
python benchmark/evaluate_swe_ci.py benchmark/SWE-CI/experiments/cog-pilot-v1 --summary-only

# Full JSON report
python benchmark/evaluate_swe_ci.py benchmark/SWE-CI/experiments/cog-pilot-v1 --json
```

The script analyzes:
1. `iteration.jsonl` → per-epoch gap, passed/failed, regressions, token costs, SWE-CI metrics (EvoScore, Resolved, Zero_reg, ZRR)
2. Archived snapshots → cog DB presence, entity/assertion counts, assertion kind distribution
3. `trajectories/` → opencode.db message history, tool call analysis, cog command detection
4. `task.log` → cog mention frequency

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Agent ignores cog despite prompt | Phase 0 smoke test catches this cheaply — check Go/No-Go criteria before proceeding |
| Cog DB doesn't persist across iterations | Phase 0 verifies persistence explicitly; if fails, check `copy_dir_from_container` behavior |
| Agent only does `cog init`, never asserts | Phase 1 checks assertion count growth; if flat, redesign prompt to drive deeper usage |
| Agent over-models (asserts on everything) | Check assertion kind distribution; if many trivial assertions, tighten prompt guidance |
| Cog adds too much token overhead | Measure in Phase 1; if >15% overhead without quality improvement, reduce to `cog impact` only |
| cog binary incompatible with container OS | Base images are Debian-based; Rust static binary should work. Verify in Phase 0 |
| Docker resource exhaustion | Lower `max_workers`, increase `memory` limit. cog is lightweight (~10MB binary) |
| `cog init /app/code/` fails (tree-sitter on unexpected code) | Phase 0 validates; cog supports Python/Rust/JS/Go/C/Java, SWE-CI tasks are Python |

## File Locations (in SWE-CI repo)

### Config files (A/B switching)

| File | Purpose |
|------|---------|
| `config_baseline.toml` | Baseline config — no `cog` key, no custom dockerfile |
| `config_cog.toml` | Cog config — `cog = true`, `dockerfile = "Dockerfile.opencode-cog"` |
| `metadata/pilot.csv` | Task metadata for pilot (3 tasks from lite.csv) |
| `run_experiment.sh` | Helper script: `./run_experiment.sh {baseline\|cog\|both}` |

### Harness patches (applied to upstream source)

| File | Patch | Effect |
|------|-------|--------|
| `src/swe_ci/config.py` | Patch 1 | `dockerfile` configurable via config.toml |
| `src/swe_ci/download.py` | Patch 2 | Custom splits accepted (no HF validation) |
| `src/swe_ci/benchmark/agents/opencode.py` | Patch 3 | Trajectory recording |
| `src/swe_ci/benchmark/tools.py` | Patch 3 | Forwards `save_trajectory_to` |
| `src/swe_ci/benchmark/run.py` | Patch 3+4 | Trajectory dirs + `cog` flag to prompt template |
| `src/swe_ci/benchmark/prompt.jinja2` | Patch 4 | `{% if cog %}` branches: baseline vs cog programmer prompt |

### Dockerfiles

| File | Purpose |
|------|---------|
| `src/swe_ci/benchmark/agents/Dockerfile.opencode` | **Original** upstream — baseline uses this |
| `src/swe_ci/benchmark/agents/Dockerfile.opencode-cog` | Adds `COPY cog` layer — cog run uses this |
| `cog` (repo root) | Static binary copied into Docker image by Dockerfile.opencode-cog |

## File Locations (in cog repo)

| File | Purpose |
|------|---------|
| `benchmark/evaluate_swe_ci.py` | Post-run analysis script |
| `benchmark/SWE-CI-GUIDE.md` | This guide |
| `benchmark/TARGET.md` | Overall benchmark strategy |