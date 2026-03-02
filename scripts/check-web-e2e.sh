#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d web ]]; then
  echo "web/ not found; skipping"
  exit 0
fi
if [[ "${SKIP_E2E:-0}" == "1" ]]; then
  echo "SKIP_E2E=1; skipping e2e"
  exit 0
fi
if jq -e '.scripts["test:e2e"]' web/package.json >/dev/null 2>&1; then
  bun run --cwd web test:e2e
else
  echo "test:e2e script not available; skipping"
fi
