# GC9D01 display bring-up history

- The original display baseline used the `gc9d01-rs` async API and an async ESP32-S3 SPI bus.
- The `esp-rtos` LAN runtime uses synchronous SPI because the interrupt-driven SPI path can stop display bring-up before the rest of the runtime starts.
- The synchronous migration initially retained an async timer implementation. The synchronous transform discarded the unpolled delay futures, allowing SPI operations to report success before the panel completed reset and Sleep-Out timing. The timer now executes those delays eagerly before returning a ready future.
