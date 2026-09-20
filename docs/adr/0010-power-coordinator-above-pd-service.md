# Power Coordinator Above PD Service

## Status

Accepted

## Context

Flux Purr must expose one read-only power projection to thermal control, display, fan control, and later consumers while preserving a PD service that is the sole owner of FUSB302B policy state and physical PD I2C. Contract intent can originate in product policy, but source negotiation and immediate heater revocation must remain independent of UI, heater, and fan modules.

## Decision

Place `PowerCoordinator` in the application-layer power domain above `PdService`. It exclusively arbitrates PD contract intent and writes `PowerState`. `PdService` continues to own PD protocol state, source-capability observation, contract confirmation, physical I2C, and safety revocation through `HeaterPwmGate`; it has no dependencies on UI, heater policy, or fan policy. Module dependencies run from policy clients through `PowerCoordinator` to `PdService` and the PD driver, while PD observations flow upward into the coordinator's read-only projection.

## Consequences

The public power contract can grow without coupling product policy into the PD protocol implementation. The coordinator cannot become a physical-output owner: thermal policy remains responsible for requested heat, and the PD service retains its direct fail-closed revoke path. A direct-PD-service state writer or a coordinator embedded in the PD service was rejected because either alternative would make the protocol boundary own whole-device policy.
