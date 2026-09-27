# RAM Bring-up Implementation

- `firmware/ram-bringup/src/protocol.rs` defines the narrow typed JSONL
  request, identity, capability, display-color, and response contract.
- `firmware/ram-bringup/src/main.rs` initializes the ESP32-S3 runtime, emits
  identity before command handling, and owns the safe output boundary.
- `firmware/examples/ram_calibration_asset.rs` renders the product's static
  calibration scene into the panel-order frame embedded by the RAM image. The
  build script verifies the asset against the renderer, and the image stores
  it in data memory to keep the executable segment within its RAM layout.
- `firmware/ram-bringup/memory.x` maps loadable text, rodata, and data to
  internal RAM and emits the complete linker-owned vectors segment. The
  vectors range is accepted only as that exact segment; payload sections may
  not partially overlap it. `tools/check_ram_elf.py` is the release artifact
  gate.
- `scripts/build-ram-bringup.sh` builds the release ELF with Xtensa jump tables
  disabled. The RAM JSON parser must not use indirect table jumps.
- `tools/flux-purr-devd/src/bin/flux_purr/cli/ram_run.rs` enforces the exact
  port, the in-process RAM ELF safety gate, matching identity/build/capability,
  and the pinned espflash RAM loader. It holds the verified serial connection
  through the preview command and response, and encodes optional display colors
  on the existing `preview_display` capability.
- The host RAM loader validates ELF identification, program-header arithmetic,
  loadable segment bounds, and internal-memory placement before invoking
  espflash.
- The host ELF parser keeps its tuple-shaped helper results behind named type
  aliases so the devd clippy gate stays clean without changing the loader or
  wire behavior.
- The product identity now carries `firmwareKind=product`; host discovery
  treats missing or unknown kinds as unknown and LAN validation accepts only
  product firmware.
