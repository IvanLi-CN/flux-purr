# GC9D01 display bring-up history

## Legacy identity

- Former legacy ID: `vmekj`.

## Lifecycle

- `active`: the display driver and host-preview baseline remains current even though later runtime behavior is maintained elsewhere.

- The original display baseline used the `gc9d01-rs` async API and an async ESP32-S3 SPI bus.
- A later synchronous migration retained an async timer implementation even though the synchronous transform did not poll it; panel operations could therefore report success before reset and Sleep-Out timing completed.
- The current runtime uses async SPI again, so timer delays and panel transfers yield to the Embassy executor. Every display operation that participates in startup or runtime refresh has a finite timeout; after a timeout, the interrupted bus is not reused and the firmware enters USB-readable recovery.
- The named `startup` preview now resolves to the branded App splash. The retained `startup-calibration` scene continues to serve Key Test and display bring-up diagnostics.
- The branded splash uses a checked-in four-color RGB565 template derived from the official Logo asset, rather than runtime approximations of the mark or a general-purpose font.
