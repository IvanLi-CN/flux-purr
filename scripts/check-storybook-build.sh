#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d web ]]; then
  echo "web/ not found; skipping"
  exit 0
fi
if jq -e '.scripts["build-storybook"]' web/package.json >/dev/null 2>&1; then
  bun run --cwd web build-storybook
else
  echo "build-storybook script not available; skipping"
fi
