---
title: Rust source-style debt remediation
module: engineering
problem_type: maintainability
component: rust-tooling
tags:
  - rust
  - rustfmt
  - clippy
  - syn
  - embedded
  - ci
related_specs: []
status: active
---

# Rust source-style debt remediation

## Context

Firmware and native device-control code share one repository but compile for
different targets. The public binary and library entrypoints had accumulated
runtime assembly, protocol adapters, CLI presentation, and thousands of tests
in one file. Rust's formatter does not enforce module boundaries, and Clippy's
size lints are opt-in or use crate-local defaults.

## Symptoms

- A small behavior change required navigating a multi-thousand-line entry file.
- Tests were hidden at the end of production entrypoints, so source size did
  not reflect the actual assembly surface.
- Firmware host Clippy did not cover Xtensa-only code, while devd had no local
  Clippy gate.
- Function-size, argument-count, and nesting regressions could be introduced
  without a deterministic local failure.

## Root cause

The repository had formatting checks but no shared structural contract. The
default Clippy levels leave `too_many_lines` disabled, and the nesting lint is
inactive until a threshold is configured. Target-specific code was built in CI
without running the same lint command. Entry files therefore became the
convenient place to add each new control-plane or test path.

## Resolution

- Keep the three public assembly boundaries small: the firmware binary,
  `flux-purr` CLI binary, and devd library facade each contain only startup or
  re-export wiring. Runtime implementation and tests live in named module
  files, so Cargo does not discover implementation files as extra binaries.
- Configure each Rust crate with `clippy.toml` thresholds of 100 lines per
  function, 7 typed parameters, and 4 levels of control-flow nesting.
- Run `rustfmt` for every Rust file at commit time. Run strict Clippy, target
  builds, and the repository `rust-style-check` binary before push and in both
  PR and main CI workflows.
- Treat the historical debt baseline as zero by physically removing every
  structural violation. The checker rejects crate-, module-, file-, and
  function-level `allow`/`expect` for `too_many_lines`,
  `too_many_arguments`, and `excessive_nesting`, including nested `cfg_attr`.
  A new violation fails the same local and CI checks; there is no structural
  baseline or debt waiver.
- Use `syn` for source-shape checks. It parses all firmware and devd Rust
  source, checks the three entry budgets, rejects inline test modules in those
  boundaries, rejects structural lint suppression hidden in `cfg_attr`, and
  keeps table-driven fixtures for the nesting metric. The checker crate itself
  runs under the same strict Clippy command before it scans the workspace.
- Keep macro definitions limited to small, declarative syntax. Runtime
  workflows and function bodies must remain ordinary items so source checks and
  Clippy measure their actual shape instead of allowing a macro expansion to
  hide historical debt.
- Keep Xtensa lint as a required matrix step for 12V, 20V, and 28V builds. A
  missing Xtensa Clippy component is a setup failure, never a reason to skip
  target-only code.

## Guardrails

- Do not impose a universal maximum file length on domain modules. Split at a
  responsibility boundary, not at an arbitrary line number.
- Treat `rustfmt` as formatting only; it cannot replace module or function
  design review.
- Keep function thresholds in crate-local Clippy configuration so firmware and
  host tooling can evolve independently while sharing the same contract.
- Test-only helpers belong in a sibling test module when they make an entry
  file grow. Tests must continue to compile under every target feature matrix.
- Target lint commands must use the exact target and features used for the
  release build. Do not substitute host lint for Xtensa-only code.
- Do not add crate-wide, module-wide, file-wide, or function-level structural
  suppressions. The checker rejects all local structural `allow`/`expect`
  attributes and recursively inspects `cfg_attr`, so an ordered protocol or
  safety workflow must be split into named helpers/state transitions instead
  of waived. An external trait or ABI may use the narrow native Clippy
  exception required by that interface only when it is not one of the three
  structural lints.
- Do not use module-level `allow(dead_code)` to mask reachability after a
  module split. Keep implementation modules private where possible and expose
  their intentional crate API through explicit re-exports; the checker rejects
  file- and module-level `allow`/`expect` attributes.
- Do not use `macro_rules!`, `include!`, or generated wrappers as a substitute
  for a responsibility boundary. A macro may remove repetitive syntax, but the
  control-flow and safety phases it expands must live in named, lint-visible
  functions.

## References

- [Rust source-size linting research](../../research/rust-source-size-linting.md)
- [Clippy `too_many_lines`](https://rust-lang.github.io/rust-clippy/master/index.html#too_many_lines)
- [Clippy `too_many_arguments`](https://rust-lang.github.io/rust-clippy/master/index.html#too_many_arguments)
- [Clippy `excessive_nesting`](https://rust-lang.github.io/rust-clippy/master/index.html#excessive_nesting)
- [Rustfmt](https://github.com/rust-lang/rustfmt)
