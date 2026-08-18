# GC9D01 display bring-up history

- The original display baseline used the `gc9d01-rs` async API and an async ESP32-S3 SPI bus.
- A later synchronous migration retained an async timer implementation even though the synchronous transform did not poll it; panel operations could therefore report success before reset and Sleep-Out timing completed.
- The current runtime uses async SPI again, so timer delays and panel transfers yield to the Embassy executor. Every display operation that participates in startup or runtime refresh has a finite timeout; after a timeout, the interrupted bus is not reused and the firmware enters USB-readable recovery.
