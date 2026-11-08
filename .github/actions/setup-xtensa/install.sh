#!/usr/bin/env bash
set -euo pipefail

# This recovery may remove the named toolchain. Never run it on a developer host.
if [[ "${GITHUB_ACTIONS:-}" != true ]]; then
  echo "Xtensa install recovery is restricted to disposable GitHub Actions jobs." >&2
  exit 2
fi

espup_bin="${1:?espup executable required}"
toolchain_version="${2:?toolchain version required}"
build_targets="${3:?build targets required}"
install_log="$(mktemp "${RUNNER_TEMP:?}/xtensa-install.XXXXXX")"
trap 'rm -f "$install_log"' EXIT

for attempt in 1 2 3; do
  if "$espup_bin" install --targets "$build_targets" \
    --toolchain-version "$toolchain_version" --skip-version-parse --name esp \
    2>&1 | tee "$install_log"; then
    exit 0
  else
    pipeline_status=("${PIPESTATUS[@]}")
    if [[ "${pipeline_status[1]}" != 0 ]]; then
      exit "${pipeline_status[1]}"
    fi
    install_status="${pipeline_status[0]}"
  fi

  # Retry only an observed transport failure, never a compiler/install failure.
  if [[ "$attempt" == 3 ]] || ! grep -Eq \
    'HTTP GET Error: (408|429|500|502|503|504)( |$)|error sending request|[Cc]onnection (reset|timed out)|[Dd][Nn][Ss] error' \
    "$install_log"; then
    exit "$install_status"
  fi

  echo "Transient Xtensa download failure; discarding the incomplete esp installation."
  # espup can otherwise mistake a directory created before a failed download for
  # an installed GCC. Its own uninstall also removes the matching LLVM link.
  "$espup_bin" uninstall --name esp
  if [[ "$attempt" == 1 ]]; then
    sleep 15
  else
    sleep 45
  fi
done
