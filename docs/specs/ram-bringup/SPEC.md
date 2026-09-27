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
- `preview display` uses the pinned GC9D01 `panel_160x50` initialization,
  including the display-on command and the physical column `15..64`, row
  `0..159` window. It turns on the active-low backlight and displays the
  product renderer's static calibration scene by default, with corner
  direction markers, color and grayscale blocks, and the panel/resolution
  label. The checked-in panel-order RGB565 frame must match the product
  renderer's output. It accepts an optional bounded `color` parameter for a
  full-screen RGB565 solid-color check: `red`, `green`, `blue`, `white`,
  `black`, `yellow`, `cyan`, or `magenta`. The request stays on the same
  `preview_display` capability and does not require a new RAM image.
  `preview frontpanel` remains a compatibility alias for the solid-green
  physical-path check; it is not the product front-panel UI preview. A
  successful display transfer sets the status light cyan.
  The SPI bus must be flushed before each DC or CS transition.
  A successful command response confirms transfer completion; physical
  display acceptance requires observing the lit panel.

## Commands

```text
flux-purr ram-run preview display --port <authorized-port>
flux-purr ram-run preview display --color red --port <authorized-port>
flux-purr ram-run preview frontpanel --port <authorized-port>
flux-purr ram-run preview status-light --port <authorized-port>
flux-purr ram-run test buttons|adc|i2c|rgb|buzzer|fan --port <authorized-port>
flux-purr ram-run exit --port <authorized-port>
```

## ELF gate

`tools/check_ram_elf.py` parses ELF program headers and loadable section
headers. Only allocated (`SHF_ALLOC`) payload sections are loadable. Xtensa
loadable segments must stay in the internal IRAM/DRAM windows,
except for the complete linker-owned vectors segment at
`0x40378000..0x40378400`. Payload sections must avoid vectors and reserved
memory, preserve `p_filesz <= p_memsz`, and fit the explicit budgets. Partial
vector overlaps, flash mapped addresses, non-identity load addresses, missing
section tables, and missing loadable sections fail the check. The entry point
must also fall inside a file-backed executable section: the complete
linker-owned vectors section or an executable payload section within IRAM; the
host validates and loads one immutable ELF snapshot.

## Related ADRs

None
