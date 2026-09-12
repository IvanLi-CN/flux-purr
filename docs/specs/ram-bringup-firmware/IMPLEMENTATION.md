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
- No physical board was operated in this change because no exact owner-
  authorized serial port was provided.
