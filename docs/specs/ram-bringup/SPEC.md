# RAM Bring-up

## Goal

Provide a flash-free ESP32-S3 image and a direct CLI workflow for physical
bring-up of display, front-panel, status-light, GPIO, ADC, I2C identification,
RGB, buzzer, and fan paths.

## Contract

- The image is a separate `flux-purr-ram-bringup` workspace crate. It starts
  with a minimal JSONL `hello` identity before any optional peripheral action.
- The identity contains `firmwareKind=ram_bringup`, the release `buildId`, the
  source SHA, and only the capabilities implemented by this image.
- Commands use `type=ram_bringup`, a request id, an operation, and the matching
  capability. Unknown operations, mismatched capabilities, and non-allowlisted
  I2C addresses are rejected.
- Product firmware keeps its existing dispatcher and rejects `ram_bringup`
  frames as malformed. The two control planes cannot be mixed.
- The CLI accepts one explicit local serial port. It never scans for a
  replacement port, changes selectors, writes flash, reads or writes EEPROM,
  negotiates PD, or treats ROM download mode as an application identity.
- `ram-run` reuses a matching RAM image only when firmware kind, build id, and
  requested capability all match. Otherwise it validates the release ELF
  against the internal-RAM safety contract and loads it through the pinned
  `espflash=4.5.0` RAM loader.
- Every command starts and ends with heater output off. PD and EEPROM remain
  untouched. Fan and buzzer actions are bounded, and I2C is limited to
  read-only identification addresses.

## Commands

```text
flux-purr ram-run preview display --port <authorized-port>
flux-purr ram-run preview frontpanel --port <authorized-port>
flux-purr ram-run preview status-light --port <authorized-port>
flux-purr ram-run test buttons|adc|i2c|rgb|buzzer|fan --port <authorized-port>
flux-purr ram-run exit --port <authorized-port>
```

## ELF gate

`tools/check_ram_elf.py` parses ELF program headers and requires Xtensa
loadable segments to stay in the internal IRAM/DRAM windows, avoid vectors and
reserved memory, preserve `p_filesz <= p_memsz`, and fit the explicit budgets.
Flash mapped addresses and non-identity load addresses fail the check.

## Related ADRs

None
