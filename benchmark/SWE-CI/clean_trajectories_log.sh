#!/usr/bin/env bash
set -euo pipefail

find experiments/ -type f -name "*.log" -exec sed -i 's/\x1b\[[0-9;]*[a-zA-Z]//g' {} +
