#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d web ]]; then
  echo "web/ not found; skipping"
  exit 0
fi
bun run --cwd web build
