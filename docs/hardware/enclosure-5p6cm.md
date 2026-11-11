# Flux Purr 5.6cm Enclosure Models

This document indexes the editable CAD sources for the `56 mm x 56 mm` Flux Purr heater-plate family. The model sources are stored under [`hardware/models/`](../../hardware/models/), while this file records their hardware context.

## Scope

- Target heater plate footprint: `56 mm x 56 mm`
- Related heater profiles:
  - [heater-5p6-3p2](heater-plates/heater-5p6-3p2.md)
  - [heater-5p6-4p5](heater-plates/heater-5p6-4p5.md)
- Model directory: [`hardware/models/`](../../hardware/models/)

## Model Sources

| Part | CAD source |
| --- | --- |
| Bottom shell | [`hardware/models/5p6cm/flux-purr-5p6-Bottom.step`](../../hardware/models/5p6cm/flux-purr-5p6-Bottom.step) |
| Inner frame | [`hardware/models/5p6cm/flux-purr-5p6-Iframe.step`](../../hardware/models/5p6cm/flux-purr-5p6-Iframe.step) |
| FreeCAD project | [`hardware/models/5p6cm/flux-purr-5p6.FCStd`](../../hardware/models/5p6cm/flux-purr-5p6.FCStd) |
| Joystick cap | [`hardware/models/flux-purr-Joystick.step`](../../hardware/models/flux-purr-Joystick.step) |

The related `7cm` model sources are indexed in [`hardware/models/`](../../hardware/models/), alongside the 5.6 cm sources.

## Source Policy

The repository stores editable STEP and FreeCAD sources as the mechanical design baseline. Derived print meshes, previews, and validation reports are generated outputs and are not part of the current model archive.
