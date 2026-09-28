# RAM Bring-up Image

`flux-purr-ram-bringup` is an ESP32-S3 image loaded through the ESP ROM RAM
loader. It has no partition table, flash image, EEPROM writer, PD contract
writer, network stack, or product control-plane dispatcher.

Build the release ELF from the repository root:

```text
bash scripts/build-ram-bringup.sh
python3 firmware/ram-bringup/tools/check_ram_elf.py firmware/ram-bringup/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup --json
```

The build checks the embedded calibration frame against the product renderer
and injects the current Git commit into the RAM identity, so a committed
source change cannot reuse an older RAM artifact.
After changing that renderer, regenerate the frame with
`cargo run --locked --manifest-path firmware/Cargo.toml --no-default-features --example ram_calibration_asset -- --write`.

The host command `flux-purr ram-run preview ...` or `flux-purr ram-run test ...`
loads that exact ELF into RAM only after the explicit serial port is verified,
then requires a matching `firmwareKind=ram_bringup`, `buildId`, and capability
set before issuing a typed JSONL command. The product firmware rejects the RAM
frame type as malformed.

`ram-run preview display` shows the product renderer's calibration scene by
default. Use `--color red|green|blue|white|black|yellow|cyan|magenta` for a
full-screen solid-color check; the option changes the typed request payload and
does not select a different RAM image.

Successful RAM responses include `ok=true`, a command-specific `result.detail`,
and safety fields confirming `heater=off`, `pd=untouched`, and
`eeprom=untouched`. Button, ADC, and I2C tests also return the sampled input or
readback value, and the CLI displays those fields. `preview status-light` plays
an approximately 8-second PWM breathing rainbow before returning the RGB LED
to off. `ram-run test buttons` prints a ready prompt, recognizes short press,
long press, and double click gestures, and ends after 30 seconds with no
recognized event. That inactivity timer resets after every button event; each
event includes its `elapsedMs` and `triggeredAtUnixMs` values in the final
evidence. A noisy input stream is bounded at 1024 reported events and returns
an explicit `event_limit` stop reason if it reaches that safety limit.
