# Flux Purr PD fan-test RAM image

`flux-purr-ram-fan-pd` is a standalone ESP32-S3 RAM image for the
`ram-run test fan-pd` workflow. It owns the FUSB302B sink session, selects the
fan-specific Fixed/PPS contract, drives GPIO35/GPIO36 through the bounded
three-stage fan cycle, and restores a verified Fixed 5V contract before its
summary.

Build and validate the release ELF from the repository root:

```text
bash scripts/build-ram-fan-pd.sh
python3 firmware/ram-bringup/tools/check_ram_elf.py firmware/ram-fan-pd/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-fan-pd --json
```

The fan image is independent from `ram-bringup` and `ram-pd-hil`. The general
`ram-run test fan` command remains PD-untouched and prints a recommendation to
use `fan-pd` when PD negotiation is needed.
