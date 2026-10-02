# Flux Purr PD Sink Voltage HIL Implementation

## Current Coverage

The repository contains the production FUSB302B PD Sink policy and service, a
general RAM bring-up image, and a capability-scoped PD HIL RAM image. The
general RAM image does not own the PD controller and reports `pd=untouched`.
The PD HIL image owns GPIO7 and the FUSB302B bus for one `test_pd_sink`
session, and uses the VIN ADC for formal validation. In external-source
diagnostic mode it does not read the VIN ADC during tier holds; it keeps the
heater low and EEPROM untouched.

The host workflow is `ram-run test pd-sink`. It selects the dedicated ELF,
requires an explicit local evidence directory, checks the exact USB identity
throughout the session, prints live progress, writes `events.ndjson`,
`summary.json`, and `transcript.log`, validates the terminal contract, and accepts
`external_source_pass` when the complete protocol matrix is valid. Incomplete
or failing outcomes remain non-zero.

When `--reload` loads a RAM image while stale product or prior-RAM JSONL output
is still buffered on the serial stream, the loader consumes frames until the
current `PRODUCT_BUILD_ID` is observed. This keeps a stale identity frame from
being reported as the newly loaded RAM image.

Progress transport uses a bounded RAM queue and the USB Serial/JTAG
non-blocking FIFO path. The device does not write progress synchronously while
the source is waiting for a PD response. After each tier reaches a terminal
state, the queued start and terminal events are drained before the next PD
request is sent. The live tier event carries the matrix position, mode, target,
and status; the terminal summary retains the negotiated contract, hold timing,
and sample evidence for every tier. The compact device summary retains raw
Source Capability PDO/APDO words; the host decodes them before terminal
validation and writes the decoded objects into the returned summary and
evidence. The currently accepted external-source profile emits live tier
events and session-boundary frames, while the terminal summary remains
authoritative for capabilities, recovery, and voltage evidence.
Formal calibrated-ADC acceptance
still requires the fuller capability/sample/recovery progress profile and is
not claimed by the physical receipt below.

The external source record is an operator-side observation retained beside the
PD evidence. The CLI does not accept, parse, or machine-bind that record to a
specific tier; `external_source_pass` therefore reports complete PD protocol
and recovery evidence, while the independent voltage judgment remains a
manual acceptance step. In this mode the device-side final-reset voltage is
explicitly unmeasured; the CLI records the fixed-contract and `VBUSOK` recovery
evidence without substituting a zero reading for a measurement.

When a RAM reload leaves the attached source in the previous PD message-id
session, the HIL image cannot recover the product sink's private Sink Message
ID counter from the retained FUSB302B state. It therefore issues one USB-PD
Hard Reset at the retained-session boundary, waits for the source to return to
the default contract, and starts fresh discovery from Message ID zero. A cold
Type-C attach keeps the normal initial advertisement window and bounded
`Get_Source_Capabilities` query. The image does not guess a retained Message
ID, use the local FUSB302B PD reset, or restart Type-C toggle.

Before every Fixed and PPS tier, the image refreshes Source Capabilities while
preserving the current confirmed contract, then sends the exact replacement
request directly. The session uses fixed 5V only for its initial boundary and
its terminal recovery; it does not force a 5V request between tiers.

The receive path keeps USB-PD Message ID domains separate. Source Capabilities
establish the current source-message boundary when a refresh is decoded;
control responses are accepted only when their Message ID is not a replay of
the last source control frame. GoodCRC is excluded because its Message ID
acknowledges the sink's transmitted frame and is not a source-message
sequence. Soft Reset and Hard Reset are surfaced to the negotiation and hold
state machines instead of being treated as ordinary empty receives.

## Design Gate

The design decisions in `SPEC.md` are settled. The implementation is covered
by the following source and validation surfaces:

- `firmware/ram-pd-hil/` contains the RAM-only image, policy codec, protocol,
  linker boundary, and build metadata.
- `tools/flux-purr-devd/src/bin/flux_purr/cli/ram_run.rs` contains the
  capability-specific loader selection, evidence writer, response validator,
  and human summary.
- `scripts/build-ram-pd-hil.sh` builds the Xtensa image and runs the RAM-only
  ELF gate.
- Host tests cover source capability decoding, request encoding, protocol
  framing, summary validation, and exit classification.

## Physical Acceptance Status

The authorized MCU port is `/dev/cu.usbmodem21141401`. The external-source
diagnostic uses IsolaPurr device `856a141cdbd4`, port `port_c`, as the voltage
observer. Earlier evidence under
`target/pd-hil-evidence/pd-hil-20260928-external-source-final15` reached
recovery tier 23 and observed the fixed `5/9/12/15/20V` plateaus plus the PPS
ladder through `21V`, but it did not receive a terminal summary and is not a
completed CLI acceptance.

The current-head external-source diagnostic record is kept locally under
`target/pd-hil-evidence/direct-contract-sequence-buffered-20261002-pr-ready-final-2f3ca27b-r2`.
It was generated from source SHA
`2f3ca27ba0b5cf67d4e2cd699d4e2a225de349d2`, build ID `2f3ca27ba0b5cf67`, and
the exact authorized port `/dev/cu.usbmodem21141401`. It parses as
`overall=external_source_pass`, `pd=default_verified`, `22/22` passing tiers,
and a passing fixed 5V final reset. This is local evidence rather than a
versioned PR artifact; the evidence directory records its own source SHA, USB
identity, command request, and complete file set. The independent IsolaPurr
record for device `856a141cdbd4`, `port_c`, reports
`power_enabled=true`, `data_connected=true`, `status=ok`, and `5045mV` after
the run. The external-source acceptance did not use ADC voltage validation.
