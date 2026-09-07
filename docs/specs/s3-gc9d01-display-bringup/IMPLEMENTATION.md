# GC9D01 display bring-up implementation

## Runtime path

- `firmware/src/bin/flux_purr.rs` owns the device display lifecycle.
- `gc9d01-rs` is compiled with `async` and `panel_160x50`.
- ESP32-S3 `SPI2` is wrapped in an asynchronous `ExclusiveDevice`, allowing Embassy to schedule USB recovery work while panel operations are pending.
- `DisplayTimer::after_millis` uses the Embassy timer queue, so panel reset and Sleep-Out timing remains cooperative.
- GC9D01 initialization, startup frame flush, initial runtime UI flush, and later UI refreshes are bounded by a one-second timeout. Transport errors and timeouts enter the USB-readable hardware recovery path; a timed-out runtime refresh first disables heater output, clears PPS ownership, requests fixed PD, and keeps cooling active. The cancelled SPI transaction is never reused.
- The App boot path renders the brand splash before power and runtime initialization proceed. The splash copies a checked-in RGB565 template generated from the official dark Logo asset. Its static `FLUX PURR` wordmark is rasterized from a panel-specific path-only SVG whose white primary straight strokes occupy two physical pixels on the `160x50` grid. It preserves an `F` left-upper outer curve, paired inner and outer `L` lower turns, and the `X` diagonals through vector rasterization; the tagline is not included. `FLUX_PURR_FW_VERSION` alone uses the splash's 4x7 bitmap glyph set, at `y=30` and centered on the wordmark's `x=99` visual center. Existing runtime 3x5 text call sites remain outside this scope. The Logo at `x=8,y=11` with a `30x28` size and the `FLUX PURR` / version text group at `x=45,y=13` / `x=55,y=30` share a vertical center, centering the complete composition within the same `y=11..38` extent and its eleven logical pixels of top and bottom margin. The resulting frame has a fixed four-color palette and is replaced by the first normal Dashboard refresh without an artificial dwell. Key Test retains the calibration scene.

## Validation

- Host rendering tests validate the shared framebuffer, splash version text, and UI state projection.
- Host binary tests cover the reusable heap scrub; the Xtensa release build verifies the async display bus, timer, and timeout path together.
- The Xtensa release build is the compile-time contract that the display bus, driver, timer, and flush path remain compatible.
- Real-device acceptance requires the startup frame and runtime UI to be visibly present after a USB-triggered reboot; a responsive control plane alone is insufficient display evidence.
- The Playwright suite allows a ten-second initial assertion window while Vite completes first-route module transformation. This changes test harness timing only, not product behavior.
