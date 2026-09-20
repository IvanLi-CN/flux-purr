# Front Panel PSRAM Graphics Memory

## Status

Accepted

## Decision

The ESP32-S3 front panel uses Quad PSRAM, mapped as a fixed 2 MiB region, with
a dedicated `EspHeap` that owns only the GC9D01 driver's 16 KiB RGB565
presentation framebuffer. The logical `DisplayCanvas` and the general runtime
heap remain in internal DRAM. SPI2 stays in Mode 0 at a fixed `40 MHz`; there
is no low-speed fallback. If PSRAM mapping or framebuffer allocation fails,
display startup enters the existing USB recovery path and heater runtime is
never admitted.

The Dashboard light theme draws the large temperature digits twice: first at
one logical pixel down and right using a darker color produced by saturating
each RGB565 channel by `4`, then at the original position using the palette
foreground. Dark themes draw only the foreground. Firmware host preview and
the Web Canvas use the same theme names, temperature palette order, and shadow
rule.

## Consequences

- PSRAM is an explicit display resource, not an extension of the general
  allocator or a compatibility path for boards without PSRAM.
- Display and heater startup share a fail-closed boundary, while USB recovery
  remains available for diagnosis.
- The Web preview must pass an explicit `FrontPanelTheme` and cannot silently
  choose a theme at render time.
