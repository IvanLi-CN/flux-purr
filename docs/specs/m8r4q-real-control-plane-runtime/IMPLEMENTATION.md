# Flux Purr 真实控制平面运行时实现状态（#m8r4q）

> 当前有效规范仍以 `./SPEC.md` 为准；这里记录实现覆盖、交付进度与 rollout 相关事实。

## Current Status

- Implementation: native `devd` foundation and shared control-plane contract are present; firmware USB JSONL runtime adapter and Web live transport remain rollout work
- Lifecycle: active
- Catalog note: Web + firmware + native devd real transport contract

## Coverage / rollout summary

- `docs/interfaces/http-api.md` defines the shared domain model, USB JSONL framing, native `devd` HTTP surface, browser Web Serial boundary, artifact verification, dry-run flash guard, and error envelope.
- `tools/flux-purr-devd` provides a localhost daemon crate with mock device support, native serial discovery constrained to the configured authorized port, lease handling, bounded device events, artifact catalog/verification, dry-run flash boundary, optional real flash execution guard, and redacted serial transport events.
- The daemon defaults to `127.0.0.1:30080`, enables loopback development CORS for local Web previews, and keeps real flash disabled unless `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1` is set.
- Native serial discovery is constrained by `FLUX_PURR_DEVD_SERIAL_PORT`, defaulting to `/dev/cu.usbmodem21221401`; absent authorized ports do not cause automatic selection of another `/dev/cu.*` or `/dev/tty.*` device.
- `scripts/devd-hardware-smoke.py` provides a machine-readable smoke runner for the native daemon contract. Mock-device mode requires `--allow-mock-device`; hardware mode preflights the authorized serial port before network or serial I/O and refuses to substitute a re-enumerated port.
- `bun run check:devd` runs Python syntax validation for the smoke script and `cargo test --manifest-path tools/flux-purr-devd/Cargo.toml`.
- Root package and hook configuration include the `check:devd` gate so the daemon crate remains covered with the rest of the repo checks.
- The project-level native bridge solution links this spec and records reusable guardrails around lease heartbeat, port-scoped serial locking, persistent serial sessions, event emission outside state locks, and the Flux Purr v1 boundary of Web -> native `devd` -> USB JSONL.

## Remaining Gaps

- Firmware still needs the `web_serial` USB JSONL adapter in `firmware/src/control_plane.rs`, including identity, network, status, runtime config, WiFi redaction, startup-busy semantics, and host tests.
- Web still needs live `devd` transport wiring, browser Web Serial client, capability gates, live target selection, Update dry-check UI, monitor trace mapping, and Storybook/Playwright coverage.
- Direct firmware HTTP / `net_http` server remains future work and must not be advertised as a current capability.
- Hardware smoke requires an explicitly authorized MCU port and must not auto-select a re-enumerated serial path.
- Real flash requires an authorized port, a passing dry-run for the same artifact, explicit `confirm=FLASH`, and `FLUX_PURR_DEVD_ALLOW_REAL_FLASH=1`.

## Verification

- `bun run check:devd`
- `git diff --check`

## References

- `./SPEC.md`
- `../../solutions/device-control/web-native-wifi-bridge-console.md`
- `../hhwq8-web-control-plane-demo/SPEC.md`
