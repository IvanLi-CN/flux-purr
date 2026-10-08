# Model-generated temperature reference for Approach

## Status

Accepted by the Operator. Refines [ADR-0017](0017-unified-production-thermal-controller.md).

## Context

Approach previously derived a desired rise from instantaneous final-target error and held minimum power for the calibrated transport delay. It had no independently evolving temperature reference. A high-loss plate can cool before reactive recovery; changing legacy power bounds does not establish trajectory tracking.

## Decision

Warmup remains independent full-power heating. Approach starts with an immediate 5% braking request, then tracks an explicit model-generated temperature/rate reference. Calibrated losses, capacity and source capability bound feasible reference rates. Braking distance controls handoff; damping and lead shape the reference response. Legacy floor/ceiling fields do not constrain the model controller.

The tracker predicts command-arrival temperature using bounded, time-weighted applied-power history. Model feedforward and temperature/rate feedback determine subsequent 5%–100% requests. Applied power is estimated from VIN, physical duty and resistance, never presented as measured electrical power. Hold tracks the constant final target using the protected PI. PD/PPS ownership, voltage/PWM mapping and source limits remain unchanged.

## Consequences

PPS measurement epochs neither restart the reference nor erase heat in flight. Actual temperature remains authoritative for phase entry and qualification; reaching a reference cannot qualify a trial. Initial points use fresh calibration and production constants; later search uses only the same run. Both source classes require exact-artifact eight-target HIL and independent confirmations before saving.

## Alternatives considered

A fixed minimum-power wait ignores predictable loss. Instantaneous final-target feedback cannot represent the approved independent decelerating response. Freezing PPS voltage or relaxing Hold/time gates changes separate contracts without repairing this control boundary.
