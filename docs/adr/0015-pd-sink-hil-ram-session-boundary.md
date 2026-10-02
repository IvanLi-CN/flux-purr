# PD Sink HIL Uses A Capability-Scoped RAM Session

## Status

Accepted

## Context

The product FUSB302B PD service owns the physical PD I2C bus, participates in
PowerCoordinator policy, and protects heater permission with freshness and
failure boundaries. The existing RAM bring-up image is intentionally smaller:
it has no FUSB302B protocol dependency, no GPIO7 interrupt token, and promises
that every existing command leaves `pd=untouched`.

PD Sink voltage HIL needs a long-lived protocol state machine, exact Fixed/PPS
requests, VIN sampling, live progress, cancellation, and a mandatory recovery
to a verified USB-C default-voltage boundary. Adding those operations to the
product runtime or pretending that they are read-only I2C checks would weaken
ownership and evidence boundaries.

## Decision

The HIL boundary is a capability-scoped RAM session:

- the PD HIL image is selected and validated by a dedicated `test_pd_sink`
  capability;
- it owns FUSB302B protocol traffic only for the lifetime of the bounded test
  session;
- it never enters the product PowerCoordinator, enables heater permission, or
  writes EEPROM;
- the general RAM bring-up capabilities retain `pd=untouched`;
- exact capability decoding, request validation, RDO encoding, freshness, and
  no-replay semantics are shared with or extracted from the canonical product
  PD domain rather than redefined by the host;
- the host owns explicit-port identity, session locking, progress presentation,
  and evidence storage, while the device owns timing, cancellation checkpoints,
  and final reset verification.

The final reset result is part of the HIL terminal contract. A session is not a
successful validation if it cannot prove that the attached link is held at a
stable fixed 5V recovery contract with no pending request or stale PD message.
The recovery path preserves CC/Rd and never restarts Type-C toggle. If the RAM
image starts on an already-attached link and the initial Source Capabilities
window expires, it performs one bounded `Get_Source_Capabilities` query. If
that query receives only GoodCRC, the sink sends one numbered USB-PD Soft Reset
using the inherited next sink Message ID, then waits for `Accept` and a new
Source Capabilities advertisement. After `Accept`, both sides restart their
Message ID counters at zero. It never uses the local FUSB302B PD reset,
restarts Type-C toggle, or withdraws CC/Rd.

## Consequences

The design preserves the product runtime's power ownership and keeps unrelated
RAM operations unchanged. It adds an image boundary, a PD-specific protocol
validator, and a longer host command deadline. RAM memory budget and the
availability of a blocking FUSB302B adapter become explicit implementation
gates.

A capability-scoped RAM session uses a test-only PPS envelope that includes 5V,
without changing the product PPS minimum. That exception must remain visible in
the session policy and evidence.

The session cannot rely on host process exit alone for cleanup. Its device-side
state machine must poll for cancellation and timeout and attempt the reset
sequence before emitting a terminal result. A lost serial port may prevent
verification; it must never be reported as a passing HIL run.

The HIL refreshes Source Capabilities before every Fixed and PPS tier while
preserving the current confirmed contract. It requests each exact target
directly, including the Fixed-to-PPS transition, and does not insert a fixed 5V
request between tiers. The initial and terminal session boundaries remain
fixed 5V recovery boundaries.

## Accepted Constraints

The session uses an attached fixed 5V recovery contract at approximately 5V at
session start and completion, caps negotiated test current at 5A with a 3A
minimum, uses a source-advertised current for recovery, and retains raw 50ms
VIN samples while running the complete matrix through one device-owned
session. Each tier obtains a fresh Source Capabilities boundary before sending
its replacement request. If capability discovery times out after the initial
advertisement window, the sink performs one bounded
`Get_Source_Capabilities` query. A GoodCRC-only response triggers one explicit
wire-level Soft Reset using the next sink Message ID, followed by an
`Accept`/Source Capabilities wait. The fallback keeps CC/Rd and VBUS asserted
and never restarts Type-C toggle or uses the local FUSB302B PD reset. These
choices are recorded in
`docs/specs/pd-sink-voltage-hil/SPEC.md`.
