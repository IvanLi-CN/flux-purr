# GC9D01 display bring-up implementation

## Runtime path

- `firmware/src/bin/flux_purr.rs` owns the device display lifecycle.
- `gc9d01-rs` is compiled with `async` and `panel_160x50`.
- ESP32-S3 `SPI2` is wrapped in an asynchronous `ExclusiveDevice`, allowing Embassy to schedule USB recovery work while panel operations are pending.
- `DisplayTimer::after_millis` uses the Embassy timer queue, so panel reset and Sleep-Out timing remains cooperative.
- GC9D01 initialization, startup frame flush, initial runtime UI flush, and later UI refreshes are bounded by a one-second timeout. Transport errors and timeouts enter the USB-readable hardware recovery path; a timed-out runtime refresh first disables heater output, clears PPS ownership, requests fixed PD, and keeps cooling active. The cancelled SPI transaction is never reused.

## Validation

- Host rendering tests validate the shared framebuffer and UI state projection.
- Host binary tests cover the reusable heap scrub; the Xtensa release build verifies the async display bus, timer, and timeout path together.
- The Xtensa release build is the compile-time contract that the display bus, driver, timer, and flush path remain compatible.
- Real-device acceptance requires the startup frame and runtime UI to be visibly present after a USB-triggered reboot; a responsive control plane alone is insufficient display evidence.
