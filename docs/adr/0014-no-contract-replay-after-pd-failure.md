# No Contract Replay After PD Failure

## Status

Accepted

## Context

A source detach, protocol reset, stale observation, timeout, or PD/I2C fault invalidates the physical assumptions behind a Requested Contract. Replaying an earlier voltage/current target after recovery could authorize heating from a source whose capabilities or attached state have changed.

## Decision

On a PD failure, PdService immediately clears the physical heater permit and forces heater PWM off, while PowerCoordinator settles any in-flight ticket with the fault, clears Requested Contract and Confirmed Active Contract, and retains no replayable target. The current PowerIntentOwner may remain as a product-mode identity, but after fresh source discovery it must recompute and explicitly submit a new contract request before heating can resume.

## Consequences

Source recovery cannot silently restore a previous power target. Thermal and calibration modes need explicit recovery paths that handle the terminal failure and issue a fresh request only when their current conditions still justify it. The direct physical revoke remains correct even if the coordinator or any subscriber is delayed.
