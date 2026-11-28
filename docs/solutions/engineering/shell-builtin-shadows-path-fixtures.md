---
title: Shell builtins shadow PATH-based test fixtures
module: engineering
problem_type: testing
component: shell-tooling
tags:
  - bash
  - testing
  - fixtures
  - path
  - github-actions
status: active
related_specs:
  - docs/specs/manual-version-release-delivery/SPEC.md
---

# Shell builtins shadow PATH-based test fixtures

## Context

The EdgeOne deploy helper decides whether the CLI output was captured by
inspecting the exit status of the `tee` stage:

```bash
"${bunx_bin}" edgeone@… makers deploy … 2>&1 | tee "${output_file}"
pipeline_status=("${PIPESTATUS[@]}")
tee_status="${pipeline_status[1]}"
```

Its test replaces `tee` through a PATH prefix pointing at
`.github/scripts/test-fixtures/tee`, which always exits non-zero, and then
asserts that the helper stops without retrying.

## Symptoms

- The deploy helper passes locally, and CI (Ubuntu) is green.
- Running the same fixture test inside a shell that provides `tee` as a builtin
  fails with `expected output capture failure to stop without retry`.
- Inserting a marker into the fixture shows it never runs.

## Root cause

`PATH` only selects external commands. When the interpreter has `tee` as a
builtin (some embedded/agent shells do), the pipeline uses the builtin, the
fixture is skipped, `PIPESTATUS[1]` is always `0`, and the helper takes the
success path. The test then fails on a downstream `grep` for text that the
helper never had reason to print.

Diagnosis that separates this from a real regression:

```bash
type -a tee          # "tee is a shell builtin" before "/usr/bin/tee"
/bin/bash -c 'type -t tee'   # "file" in a stock shell
```

Running the same test with `/bin/bash` (or `env -i … /bin/bash`) passes, which
proves the code path is correct and only the command resolution differs.

## Resolution

Inject the capture tool explicitly instead of relying on PATH lookup:

```bash
tee_bin="${EDGEONE_TEE_BIN:-tee}"
[[ "${tee_bin}" == */* && ! -x "${tee_bin}" ]] && { echo "…" >&2; exit 2; }
… | "${tee_bin}" "${output_file}"
```

The test sets `EDGEONE_TEE_BIN` to the fixture and no longer mutates `PATH`.
The release workflow keeps the `tee` default, so production behavior is
unchanged. The workflow contract test asserts both the injection point and the
test-side wiring, so the fixture can silently stop being used again.

## Guardrails / Reuse notes

- Any test that fakes a command through `PATH` must also prove the fake ran
  (marker file, counter, or an explicit injection variable). A fake that is
  never invoked still produces a "plausible" failure later in the assertion.
- Prefer explicit `<TOOL>_BIN` variables over PATH prefixes for pipeline
  stages whose exit status is part of the behavior under test.
- When a test fails only in one shell, compare `type -a <cmd>` and rerun with a
  stock `/bin/bash` before suspecting the code.

## References

- [Manual version release delivery spec](../../specs/manual-version-release-delivery/SPEC.md)
