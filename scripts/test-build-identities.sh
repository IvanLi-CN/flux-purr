#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
source_sha="$(git -C "${repo_root}" rev-parse HEAD)"
expected="$(python3 "${repo_root}/scripts/product-version.py" --mode development --repo-root "${repo_root}" --source-sha "${source_sha}")"
before="$(python3 -c 'from pathlib import Path; print(Path("VERSION").read_bytes().hex())' )"

devd_version="$(cargo run --quiet --manifest-path "${repo_root}/tools/flux-purr-devd/Cargo.toml" --bin flux-purr-devd -- --version)"
cli_version="$(cargo run --quiet --manifest-path "${repo_root}/tools/flux-purr-devd/Cargo.toml" --bin flux-purr -- --version)"
test "${devd_version}" = "flux-purr-devd ${expected}"
test "${cli_version}" = "flux-purr ${expected}"

if [[ -f "${repo_root}/web/dist/build-info.json" ]]; then
  python3 - "${repo_root}/web/dist/build-info.json" "${expected}" "${source_sha}" <<'PY'
import json
import sys
from pathlib import Path

payload = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
assert payload["version"] == sys.argv[2]
assert payload["sourceSha"] == sys.argv[3]
assert payload["channel"] == "local"
PY
fi

after="$(python3 -c 'from pathlib import Path; print(Path("VERSION").read_bytes().hex())' )"
test "${before}" = "${after}"
printf 'build identities agree: %s\n' "${expected}"
