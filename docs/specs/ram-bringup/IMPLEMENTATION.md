# RAM Bring-up Implementation

- `firmware/ram-bringup/src/protocol.rs` defines the narrow typed JSONL
  request, identity, capability, display-color, diagnostic result, and response
  contract. JSONL requests must consume the entire bounded line before command
  validation. Responses carry command-specific evidence for button, ADC, and
  I2C reads plus bounded effect labels for output tests.
- `firmware/ram-bringup/src/main.rs` initializes the ESP32-S3 runtime, emits
  identity before command handling, and owns the safe output boundary.
- The status-light preview uses software PWM on the common-anode RGB outputs to
  play an approximately 8-second hue-changing breathing gradient, then returns
  the LED to the safe-off state. The host CLI renders the returned detail,
  safety fields, and diagnostic samples instead of printing a generic success
  token.
- The interactive button test keeps one verified RAM serial session and polls
  until 30 seconds have elapsed without a button event. The host
  recognizes short-press, long-press, and double-click gestures from the typed
  five-key snapshots, resets the inactivity timer after each button event, and
  prints each event with its session-relative and Unix-millisecond trigger times
  before returning the final evidence envelope. The host keeps the event list
  bounded at 1024 entries; an input stream that reaches that limit ends with
  an explicit `event_limit` stop reason.
- The bounded `test_rgb` action holds each common-anode RGB channel for one
  second in red, green, blue order and repeats the sequence five times before
  returning all channels to safe off.
- The bounded `test_fan` action calibrates the GPIO1 VIN ADC sample through the
  ESP32-S3 ADC curve and the frozen `56 kOhm / 5.1 kOhm` divider model before
  driving GPIO35 as the fan enable and GPIO36 through MCPWM operator 0 at
  25 kHz for 50% duty for five seconds, 100% duty for five seconds, and 0%
  duty for five seconds before returning both outputs to safe off. It reports
  the raw ADC code, calibrated ADC millivolts, calculated input millivolts,
  and a 12.5V minimum result. The RAM image never writes PD; a missing or low
  source contract is a CLI warning rather than an implicit power mutation.
  The firmware emits compact `fp` (power) and `fs` (PWM stage) JSONL frames;
  the host consumes the power frame first, then renders each stage frame in
  the same fan session.
- The bounded `test_buzzer` action bit-bangs GPIO48 as a 50% square wave at
  1 kHz for one second, holds it low for one second, then emits 2 kHz for one
  second before returning the pin low and reporting the typed sequence.
- The diagnostic image uses bounded blocking peripheral calls because it runs
  outside the product Embassy executor; the product firmware's async display
  contract remains scoped to `firmware/` runtime paths.
- `firmware/examples/ram_calibration_asset.rs` renders the product's static
  calibration scene into the panel-order frame embedded by the RAM image. The
  build script verifies the asset against the renderer, and the image stores
  it in data memory to keep the executable segment within its RAM layout.
- `firmware/ram-bringup/memory.x` maps loadable text, rodata, and data to
  internal RAM and emits the complete linker-owned vectors segment. The
  vectors range is accepted only as that exact segment; payload sections may
  not partially overlap it. `tools/check_ram_elf.py` is the release artifact
  gate.
- `scripts/build-ram-bringup.sh` builds the release ELF with Xtensa jump tables
  disabled. The RAM JSON parser must not use indirect table jumps.
- `tools/flux-purr-devd/src/bin/flux_purr/cli/ram_run.rs` enforces the exact
  port, stable ESP32-S3 USB VID/PID/serial identity, the in-process RAM ELF
  safety gate, the `flux-purr.usb.v1` JSONL contract, matching
  identity/build/capability, complete command-specific response evidence, and
  the pinned espflash RAM loader. It rechecks the USB identity after acquiring
  the process lock and before RAM writes or JSONL requests, then holds the
  verified serial connection through the preview command and response.
  Interactive button polling keeps the same lock and rechecks the identity
  immediately before every bounded request.
  Optional display colors remain on the existing `preview_display` capability.
- Direct RAM, daemon flash, direct flash, and recover commands share the same
  per-port process lock; Unix uses `flock` and Windows uses a byte-range file
  lock that survives worker-thread handoff. Session-cache keys use the same
  canonical identity as the process lock, Windows reconnect treats `COMx` as
  an enumerated device name rather than a filesystem path, and unsupported
  Unix platforms fail closed instead of following an unsafe lock-file alias.
- The host RAM loader validates ELF identification, program-header arithmetic,
  loadable segment and section bounds, and internal-memory placement before
  invoking espflash. It requires the entry point to be in a file-backed
  executable vectors or IRAM section and loads the same validated ELF snapshot
  that it checked. It transfers only file-backed sections: the ELF entry is the
  Xtensa reset symbol, whose default `__zero_bss` hook clears `_bss_start` to
  `_bss_end` before `main`; linker-owned `.stack` and `.noinit` memory are not
  sent as synthetic zero-fill sections.
- The host ELF parser keeps its tuple-shaped helper results behind named type
  aliases so the devd clippy gate stays clean without changing the loader or
  wire behavior.
- The product identity now carries `firmwareKind=product`; host discovery
  treats missing or unknown kinds as unknown and LAN validation accepts only
  product firmware.
