# Flux Purr 5.6cm Enclosure Models

## Background

Flux Purr has documented `56 mm x 56 mm` heater-plate variants, but the matching enclosure print assets need a stable repository location, durable filenames, and model metadata that can be reviewed without relying on external working directories.

## Goals

- Store the 5.6cm enclosure print-ready STL files as long-term hardware assets.
- Use stable filenames that describe product, size, and part role.
- Document model purpose, source filename mapping, geometry metadata, validation results, and preview images.
- Keep editable STEP sources outside the repository.

## Non-Goals

- No firmware, Web UI, native daemon, or runtime API changes.
- No regeneration or geometry repair beyond copying the approved STL outputs.
- No enclosure support for the `70 mm x 70 mm` heater-plate variant.

## Requirements

- The repository stores exactly these 5.6cm enclosure STL parts:
  - bottom shell
  - inner frame
  - joystick cap
- Hardware model STL files are tracked through Git LFS using the scoped rule `docs/hardware/models/**/*.stl`.
- The hardware documentation links each STL to its source filename, SHA-256, bounding box, triangle count, validation status, and preview image.
- The model documentation references the existing `heater-5p6-3p2` and `heater-5p6-4p5` heater-plate variants.

## Acceptance Checklist

- The three repository STL files match the approved source SHA-256 values.
- Blender 3D Print Toolbox reports `0` non-manifold edges, `0` intersect faces, `0` zero faces, `0` thin faces, and `1` shell for every part.
- Preview PNGs render each part clearly enough to identify its role.
- `docs/hardware/enclosure-5p6cm.md` is the human-facing hardware model index.
- `README.md` links the enclosure model documentation from the hardware references.

## References

- [5.6cm enclosure model documentation](../../hardware/enclosure-5p6cm.md)
- [5.6cm 3.2 ohm heater plate](../../hardware/heater-plates/heater-5p6-3p2.md)
- [5.6cm 4.5 ohm heater plate](../../hardware/heater-plates/heater-5p6-4p5.md)
