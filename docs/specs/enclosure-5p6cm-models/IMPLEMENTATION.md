# Implementation

## Coverage

- Hardware model STLs live under `docs/hardware/models/enclosure-5p6cm/`.
- Git LFS tracks hardware model STLs with the scoped pattern `docs/hardware/models/**/*.stl`.
- Preview renders live under `docs/hardware/models/enclosure-5p6cm/previews/`.
- Blender validation output lives under `docs/hardware/models/enclosure-5p6cm/validation/blender-stl-check.json`.
- The human-facing model index is `docs/hardware/enclosure-5p6cm.md`.

## Validation State

The checked models are binary STL meshes copied from the approved repaired precision outputs. SHA-256 checks and Blender 3D Print Toolbox validation are recorded in the hardware model documentation.

## Remaining Gaps

- Editable STEP sources are not checked into this repository.
- Print orientation, material choice, support strategy, and heat-cycle fit validation remain bench-process decisions outside this model archive.
