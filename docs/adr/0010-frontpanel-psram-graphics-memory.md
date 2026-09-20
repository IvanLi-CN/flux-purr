# Front Panel PSRAM Graphics Memory

## Status

Accepted

## Decision

The ESP32-S3 front panel uses Quad PSRAM, whose detected mapped region must be
exactly 2 MiB, with
a dedicated `EspHeap` that owns only the GC9D01 driver's 16 KiB RGB565
presentation framebuffer. The logical `DisplayCanvas` and the general runtime
heap remain in internal DRAM. SPI2 stays in Mode 0 at a fixed `40 MHz`; there
is no low-speed fallback. If PSRAM mapping or framebuffer allocation fails,
display startup enters the existing USB recovery path and heater runtime is
never admitted. On the pinned `esp-hal 1.0.0` path, a low-level MMU mapping
panic can occur inside `esp_hal::init` before USB exists; that boundary is
accepted as panic-handler software-reset fail-closed behavior until a HAL
version or local patch exposes it as a recoverable result.

The Dashboard light theme draws the large temperature digits twice: first at
one logical pixel down and right using a darker color produced by saturating
each RGB565 channel by `4`, then at the original position using the palette
foreground. Dark themes draw only the foreground. Firmware host preview and
the Web Canvas use the same theme names, temperature palette order, and shadow
rule.

## Consequences

- PSRAM is an explicit display resource, not an extension of the general
  allocator or a compatibility path for boards without PSRAM.
- Display and heater startup share a fail-closed boundary. USB recovery remains
  available for failures returned after HAL initialization; an earlier HAL MMU
  panic resets closed and cannot expose the USB recovery loop.
- The Web preview must pass an explicit `FrontPanelTheme` and cannot silently
  choose a theme at render time.
