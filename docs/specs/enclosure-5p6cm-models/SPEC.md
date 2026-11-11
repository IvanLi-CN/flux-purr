# Flux Purr 5.6cm Enclosure Models

## Background

Flux Purr has documented `56 mm x 56 mm` heater-plate variants and keeps the editable mechanical sources in the repository under `hardware/models/`.

## Goals

- Keep the 5.6 cm enclosure STEP and FreeCAD sources in a stable repository location.
- Keep related 7 cm sources and shared mechanical parts discoverable from the same model index.
- Document the relationship between model sources and heater-plate hardware.

## Non-Goals

- No firmware, Web UI, native daemon, or runtime API changes.
- No automatic mesh generation, print validation, or manufacturing process definition.

## Requirements

- The 5.6 cm enclosure sources live under `hardware/models/5p6cm/`.
- The 5.6 cm model set includes the bottom shell, inner frame, and FreeCAD project sources.
- The shared joystick cap source lives at `hardware/models/flux-purr-Joystick.step`.
- The human-facing model index is `docs/hardware/enclosure-5p6cm.md`.
- The model documentation references the existing `heater-5p6-3p2` and `heater-5p6-4p5` heater-plate variants.

## Acceptance Checklist

- The documented source paths exist under `hardware/models/`.
- The model index identifies the source format and the related heater-plate profiles.
- The repository does not claim that derived STL meshes, previews, or validation reports are tracked when they are absent.

## References

- [5.6cm enclosure model documentation](../../hardware/enclosure-5p6cm.md)
- [Mechanical model README](../../hardware/models/README.md)
- [5.6cm 3.2 ohm heater plate](../../hardware/heater-plates/heater-5p6-3p2.md)
- [5.6cm 4.5 ohm heater plate](../../hardware/heater-plates/heater-5p6-4p5.md)
