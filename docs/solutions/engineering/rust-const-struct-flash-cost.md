---
title: Rust const struct templates consume firmware flash
module: engineering
problem_type: performance
component: firmware
tags:
  - rust
  - firmware
  - flash-budget
  - rodata
  - esp32-s3
  - linker-map
status: active
related_specs:
  - docs/specs/heater-pid-frontpanel-runtime/SPEC.md
---

# Rust const struct templates consume firmware flash

## Context

The product image must stay inside the fixed 2 MiB factory-app partition.
Large runtime state is deliberately heap-allocated because it does not fit in
internal DRAM: the thermal tuning runtime alone is about 317 KiB, dominated by
the 256-entry event ring and the two 1001-bucket output histograms.

## Symptoms

- The factory-app image steadily approaches the partition limit even though
  little new logic lands.
- `cargo bloat`-style attribution is flat: no single function or crate
  explains the growth.
- Compiler-flag tuning (lower `opt-level`, more LTO) recovers space only once
  and hides the actual cost.

## Root cause

`ThermalTuningRuntime::new()` returned a `const EMPTY: Self`. Rust lowers a
`const` aggregate into a `.rodata` image at the use site, so the boot path
compiled to `Box::<T>::new_uninit()` followed by a 324,920-byte `memcpy` from
flash into the heap allocation. The template is 99.99% zero bytes, yet the
whole thing is stored in the flashed `.rodata` section: 15.5% of the 2 MiB
partition spent on a copy source.

Evidence that identifies this class of problem:

- `xtensa-esp32s3-elf-nm -S --size-sort` shows one anonymous `.rodata` symbol
  of the struct's size.
- `xtensa-esp32s3-elf-objdump -d` shows `l32r` of that address followed by a
  `memcpy` whose length equals `size_of::<T>()`.
- The matching `-Wl,-Map` input section is `.rodata..Lanon.<hash>.<n>` from the
  firmware crate, not from a dependency.
- A long zero run in the app image (`espflash save-image` output) matches the
  template size.

## Resolution

Construct large idle state with field stores instead of a `const` aggregate:

```rust
impl ThermalTuningRuntime {
    pub fn new() -> Self {
        #[rustfmt::skip]
        let runtime = Self { /* every field, same values as the old EMPTY */ };
        runtime
    }
}
```

The constructor must stay below the 100-line function limit, and the repo style
checker rejects structural `allow`/`expect` exceptions, so the field list is
packed and annotated with `#[rustfmt::skip]`. A packed initializer keeps the
literal exhaustive (Rust requires every field) while `cargo fmt --check` stays
green.

Measured on the same source revision:

- factory-app image 1,829,008 B (87.21%) -> 1,504,816 B (71.75%)
- `.rodata` 486,872 B -> 161,952 B
- `.text` +716 B (field stores replacing one bulk copy)
- boot stack gate unchanged and passing

## Guardrails / Reuse notes

- Do not reintroduce a `const`/`static` aggregate for heap-owned runtime state.
  A `static` moves the cost into `.data` (flash copy plus RAM) and is worse.
- Keep the state definition in one place: any new field must be added to the
  constructor literal, and host tests exercise the resulting idle state.
- Verify with the image gate, not the ELF size alone:
  `just firmware-image-check`, then `scripts/check-firmware-boot-stack.sh`.
- When the partition tightens again, look for `.rodata` templates and
  host-inbound-only deserializers before touching compiler flags.

## References

- [Developer policy](../../guides/flux-purr-developer-policy.md)
- [Heater PID and front panel runtime spec](../../specs/heater-pid-frontpanel-runtime/SPEC.md)
