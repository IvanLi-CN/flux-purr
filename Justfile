set shell := ["bash", "-uc"]

devd_manifest := "tools/flux-purr-devd/Cargo.toml"

# List available project commands.
default:
    @just --list

# Run the host CLI without exposing its Cargo invocation.
cli *args:
    @cargo run --manifest-path {{ devd_manifest }} --bin flux-purr -- {{ args }}

# Bind an explicitly named logical target to an explicit devd device and URL.
hardware-save id device devd:
    @cargo run --manifest-path {{ devd_manifest }} --bin flux-purr -- hardware save --id {{ id }} --name {{ id }} --device {{ device }} --devd {{ devd }}

# Open the buzzer test session for an explicit device or saved target; optional --devd follows it.
buzzer-play *selector:
    @cargo run --manifest-path {{ devd_manifest }} --bin flux-purr -- buzzer play {{ selector }}

# Run the host daemon and CLI test suite.
check-devd:
    @bun run check:devd

# Generate an offline factory image and verify its application partition size; no hardware access.
firmware-image-check elf="firmware/target/xtensa-esp32s3-none-elf/release/flux-purr":
    @bash scripts/check-firmware-image-size.sh {{ quote(elf) }}

# Check repeated public HTTP reads against one explicitly identified device; no writes or credentials.
lan-http-read-check base_url device_id:
    @python3 scripts/check-lan-http-reads.py --base-url {{ quote(base_url) }} --device-id {{ quote(device_id) }}
