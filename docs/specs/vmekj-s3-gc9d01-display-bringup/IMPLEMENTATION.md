# GC9D01 display bring-up implementation

## Runtime path

- `firmware/src/bin/flux_purr.rs` owns the device display lifecycle.
- `gc9d01-rs` is compiled with `panel_160x50` and its synchronous transform.
- ESP32-S3 `SPI2` is wrapped in `ExclusiveDevice` in blocking mode so display transfers cannot stall the `esp-rtos` startup executor waiting for an SPI interrupt.
- `DisplayTimer::after_millis` performs the requested ROM delay eagerly and returns an already-ready future. This matches the upstream synchronous transform, which invokes but does not poll the timer future.
- GC9D01 initialization, startup frame flush, initial runtime UI flush, and later UI refreshes are checked for transport errors; bring-up failures enter the USB-readable hardware recovery path.

## Validation

- Host rendering tests validate the shared framebuffer and UI state projection.
- A host regression test proves the panel delay side effect occurs even when the returned future is never polled.
- The Xtensa release build is the compile-time contract that the display bus, driver, timer, and flush path remain compatible.
- Real-device acceptance requires the startup frame and runtime UI to be visibly present after a USB-triggered reboot; a responsive control plane alone is insufficient display evidence.
