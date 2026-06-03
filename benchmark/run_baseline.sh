#!/usr/bin/env bash
# Baseline test — vanilla Terminus-2 without cog.
#
# Usage:
#   ./benchmark/run_baseline.sh [TASK_NAME]
#
# Default task: build-cython-ext

set -euo pipefail

TASK="${1:-build-cython-ext}"
DATASET="terminal-bench@2.0"
MODEL="deepseek/deepseek-chat"
AGENT="terminus-2"
JOB_NAME="baseline-${TASK}-$(date +%Y%m%d-%H%M%S)"

echo "=== Baseline Run ==="
echo "Task:   ${TASK}"
echo "Agent:  ${AGENT}"
echo "Model:  ${MODEL}"
echo "Job:    ${JOB_NAME}"
echo ""

harbor run \
    --dataset "${DATASET}" \
    --agent "${AGENT}" \
    --model "${MODEL}" \
    --include-task-name "${TASK}" \
    --job-name "${JOB_NAME}" \
    --jobs-dir benchmark/jobs \
    "$@"
