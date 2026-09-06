# GC9D01 display bring-up implementation

## Runtime path

- `firmware/src/bin/flux_purr.rs` owns the device display lifecycle.
- `gc9d01-rs` is compiled with `async` and `panel_160x50`.
- ESP32-S3 `SPI2` is wrapped in an asynchronous `ExclusiveDevice`, allowing Embassy to schedule USB recovery work while panel operations are pending.
- `DisplayTimer::after_millis` uses the Embassy timer queue, so panel reset and Sleep-Out timing remains cooperative.
- GC9D01 initialization, startup frame flush, initial runtime UI flush, and later UI refreshes are bounded by a one-second timeout. Transport errors and timeouts enter the USB-readable hardware recovery path; a timed-out runtime refresh first disables heater output, clears PPS ownership, requests fixed PD, and keeps cooling active. The cancelled SPI transaction is never reused.
- The App boot path renders the brand splash before power and runtime initialization proceed. The splash copies a checked-in RGB565 template generated from the official dark Logo asset; the static `FLUX PURR` wordmark uses a 4x7 custom bitmap expanded to 90 by 21 pixels, and `FLUX_PURR_FW_VERSION` is overlaid with a dedicated 3x5 pixel glyph set centered at the wordmark's `x=105` visual center. The resulting frame has a fixed four-color palette and is replaced by the first normal Dashboard refresh without an artificial dwell. Key Test retains the calibration scene.

## Validation

- Host rendering tests validate the shared framebuffer, splash version text, and UI state projection.
- Host binary tests cover the reusable heap scrub; the Xtensa release build verifies the async display bus, timer, and timeout path together.
- The Xtensa release build is the compile-time contract that the display bus, driver, timer, and flush path remain compatible.
- Real-device acceptance requires the startup frame and runtime UI to be visibly present after a USB-triggered reboot; a responsive control plane alone is insufficient display evidence.
