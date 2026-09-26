# RAM Bring-up Image

`flux-purr-ram-bringup` is an ESP32-S3 image loaded through the ESP ROM RAM
loader. It has no partition table, flash image, EEPROM writer, PD contract
writer, network stack, or product control-plane dispatcher.

Build the release ELF from the repository root:

```text
cargo +esp build --manifest-path firmware/ram-bringup/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/ram-bringup/target --release
python3 firmware/ram-bringup/tools/check_ram_elf.py firmware/ram-bringup/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup --json
```

The host command `flux-purr ram-run preview ...` or `flux-purr ram-run test ...`
loads that exact ELF into RAM only after the explicit serial port is verified,
then requires a matching `firmwareKind=ram_bringup`, `buildId`, and capability
set before issuing a typed JSONL command. The product firmware rejects the RAM
frame type as malformed.
