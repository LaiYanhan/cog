#!/usr/bin/env bash
# Experimental test — Terminus-2 equipped with cog binary + inline skill docs.
#
# Usage:
#   ./benchmark/run_cog.sh [TASK_NAME]
#
# Default task: build-cython-ext
#
# Prerequisites:
#   - cargo build --release  (produces target/release/cog)

set -euo pipefail

TASK="${1:-build-cython-ext}"
DATASET="terminal-bench@2.0"
MODEL="deepseek/deepseek-chat"
AGENT_IMPORT="benchmark.cog_terminus:CogEquippedTerminus"

JOB_NAME="cog-${TASK}-$(date +%Y%m%d-%H%M%S)"

echo "=== Cog Experimental Run ==="
echo "Task:   ${TASK}"
echo "Agent:  CogEquippedTerminus"
echo "Model:  ${MODEL}"

echo "Job:    ${JOB_NAME}"
echo ""

harbor run \
    --dataset "${DATASET}" \
    --agent-import-path "${AGENT_IMPORT}" \
    --model "${MODEL}" \
    --include-task-name "${TASK}" \
    --job-name "${JOB_NAME}" \
    --jobs-dir benchmark/jobs \
    "$@"
