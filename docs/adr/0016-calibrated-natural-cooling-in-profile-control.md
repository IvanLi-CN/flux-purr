# Calibrated natural cooling in 5A profile control

Status: Accepted

## Context

The 5A profile controller discarded the calibrated plant input. Static profile braking and a per-frame raw cooling threshold left Hold recovery insensitive to a slow but sustained decline, while tuning classified the subsequent low-band exit as Approach underpower. The owner requires the cooling data to participate in control and the repeated saturated search to stop.

## Decision

Retain the source-class selection in ADR 0015 and the shared production profile controller. Supply its existing plant input to 5A, reuse the fitted passive-loss equation, and bound model-assisted braking with the profile's Warmup hysteresis. Use calibrated delay for Approach prediction and raw cooling plus modelled loss to anticipate Hold recovery. Seed the existing PI blend with bounded loss feedforward. Keep physical phase admission, safety precedence, power mapping, candidate persistence and independent qualification unchanged.

Use complete point equality to detect unchanged or cyclic proposals. Treat mostly-off Hold lower-band exits as recovery evidence before Approach underpower. For verified timely approach followed by mild two-sided Hold ripple at saturated Hold output, reduce Hold pulse size and predictive lead and recover earlier instead of increasing Approach energy. Require the measured band, output and ripple evidence defined in the control contract; this is not an escape for sustained insufficient heating. Preserve the external runner; document these firmware strategy differences explicitly.

## Consequences

Normal 5A heating and firmware tuning receive the same model assistance. Model predictions are estimates, including extrapolation outside the passive calibration range; they do not establish real electrical power or actively cooled response. Saved profile fields retain their units and PI role. The 3A loss calculation is refactored without numerical change. Firmware qualification requires fresh exact-artifact HIL; old accepted traces cannot establish the new controller's performance.
