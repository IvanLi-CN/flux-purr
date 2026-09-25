# RAM Bring-up Firmware Implementation

## Firmware

- `firmware/ram-bringup` is a separate no-std ESP32-S3 binary with its own
  release linker layout.
- The workspace release profile uses fat LTO and one codegen unit so the
  complete renderer and control loop fit the fixed internal IRAM window; the
  linker window itself is not relaxed.
- Display scenes reuse the product renderer and GC9D01 async SPI driver.
- Preview frames carry only the closed `light`/`dark` theme selector; no
  arbitrary scene, palette, GPIO, or peripheral parameters cross the USB
  boundary.
- GPIO, ADC, I2C, RGB, buzzer, and fan commands are implemented in one
  request-at-a-time USB JSONL loop.
- Startup, command completion, errors, and panic paths force heater PWM, fan,
  buzzer, RGB, and backlight to safe defaults.
- The fan path keeps the product runtime's disabled-output contract: `fan_en`
  is low and the PWM setpoint is `1000‰` before and after the restricted
  `300‰` ten-second test window. The legacy `FanPhase::Stop` value is not used
  as a hardware shutdown duty.

## CLI

`tools/flux-purr-devd/src/bin/flux-purr.rs` owns the `ram-run` state machine.
It validates the exact port and local ELF, probes identity before reset, reuses
only a matching `ram_bringup` build/capability, and waits for a matching
identity after every RAM load. `--reload` bypasses reuse. `--install` routes to
the existing direct flash gate and retains its EEPROM snapshot rules.

`exit` performs a controlled reset without a flash operation and accepts only a
verified `product` identity afterward.

## Validation status

- Host firmware and CLI tests pass.
- Xtensa release build and internal-memory ELF checker pass locally.
- Authorized HIL on `/dev/cu.usbmodem2111401` confirmed RAM identity, buttons,
  ADC, read-only I2C, RGB, buzzer, and the repaired ten-second fan command
  without Flash writes. The ROM preflight reported `0x00000000` security flags
  and did not take a Flash fallback path.
- The first fan HIL attempt exposed a software scheduling defect: the RAM
  command loop used a synchronous busy loop and did not yield to the Embassy
  executor during the ten-second fan window, so the watchdog reset the target.
  The loop now yields with `Timer::after`, and the fan command completed on the
  exact port.
- The first controlled `exit` reset received an identity frame from the
  previously installed application, but its missing `firmwareKind` classified
  it as legacy/unknown. The CLI correctly refused to assume Product Firmware.
  After the explicitly authorized Product Firmware flash, `exit` now verifies
  `firmwareKind=product`; the flash used the documented backup bypass, so no
  EEPROM archive was created and EEPROM health remains unknown.
- The front-panel preview initially reset during rendering because the large
  `FrontPanelUiState` was materialized on the RAM command task stack. The
  bring-up path now allocates each preview state from its internal heap before
  calling the shared renderer; all 26 states complete and return their JSONL
  response within the dedicated preview timeout.
