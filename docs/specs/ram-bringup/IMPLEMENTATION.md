# RAM Bring-up Implementation

- `firmware/ram-bringup/src/protocol.rs` defines the narrow typed JSONL
  request, identity, capability, and response contract.
- `firmware/ram-bringup/src/main.rs` initializes the ESP32-S3 runtime, emits
  identity before command handling, and owns the safe output boundary.
- `firmware/ram-bringup/memory.x` maps loadable text, rodata, and data to
  internal RAM. `tools/check_ram_elf.py` is the release artifact gate.
- `tools/flux-purr-devd/src/bin/flux_purr/cli/ram_run.rs` enforces the exact
  port, the in-process RAM ELF safety gate, matching identity/build/capability,
  and the pinned espflash RAM loader.
- The product identity now carries `firmwareKind=product`; host discovery
  treats missing or unknown kinds as unknown and LAN validation accepts only
  product firmware.
