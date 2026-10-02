# Flux Purr PD Sink Voltage HIL History

## Current Identity

This topic defines the approved RAM-based HIL contract for exact Fixed PDO and
PPS voltage negotiation, VIN hold evidence, recovery, and default-state
verification. It is separate from the product PD Sink runtime contract and the
general RAM bring-up capability set.

## Decision Boundary

The PPS 5V envelope, current ceiling, initial and terminal fixed 5V recovery
boundaries, direct tier transitions, measurement tolerance, evidence
granularity, RAM image boundary, session ownership, and external-source
diagnostic mode are settled in `SPEC.md`. The RAM image and host workflow are
implemented. The authorized port and IsolaPurr `port_c` record show stable
source telemetry from 5V through the requested fixed and PPS levels, and the
complete external-source diagnostic has a terminal passing summary. Formal ADC
acceptance remains a separate gate.
