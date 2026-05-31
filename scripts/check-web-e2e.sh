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
  repo_root="$(git rev-parse --show-toplevel)"
  branch="$(git branch --show-current)"
  hash8="$(python3 - "$repo_root" <<'PY'
import hashlib
import sys

print(hashlib.sha256(sys.argv[1].encode()).hexdigest()[:8])
PY
)"
  scope_id="flux-purr--${hash8}--web-e2e"
  registry="${CODEX_HOME:-$HOME/.codex}/skills/global-port-manager/scripts/port_registry.py"

  choose_ephemeral_port() {
    python3 - <<'PY'
import socket

sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
  }

  allocate_port() {
    local service="$1"
    if [[ ! -f "$registry" ]]; then
      choose_ephemeral_port
      return
    fi

    python3 "$registry" --json allocate \
      --scope-id "$scope_id" \
      --project flux-purr \
      --repo-root "$repo_root" \
      --branch "$branch" \
      --worktree-path "$repo_root" \
      --service "$service" |
      python3 -c 'import json, sys; print(json.load(sys.stdin)["port"])'
  }

  release_leases() {
    [[ -f "$registry" ]] || return 0
    python3 "$registry" --json release-service --scope-id "$scope_id" --service web-e2e >/dev/null 2>&1 || true
    python3 "$registry" --json release-service --scope-id "$scope_id" --service devd-e2e >/dev/null 2>&1 || true
  }
  trap release_leases EXIT

  export E2E_WEB_PORT="${E2E_WEB_PORT:-$(allocate_port web-e2e)}"
  export E2E_DEVD_PORT="${E2E_DEVD_PORT:-$(allocate_port devd-e2e)}"
  echo "E2E_WEB_PORT=$E2E_WEB_PORT E2E_DEVD_PORT=$E2E_DEVD_PORT"
  bun run --cwd web test:e2e
else
  echo "test:e2e script not available; skipping"
fi
