---
title: FUSB302B PD Sink stability and recovery
module: device-control
problem_type: troubleshooting
component: firmware-pd-sink
tags:
  - fusb302b
  - usb-pd
  - pps
  - scheduling
  - contract-recovery
status: active
related_specs:
  - docs/specs/fusb302b-dual-pd-sink/SPEC.md
  - docs/specs/fusb302b-dual-pd-sink/HIL.md
---

# FUSB302B PD Sink stability and recovery

## Context and symptoms

Flux Purr is powered by the VBUS negotiated by its FUSB302B Sink. A disrupted attachment or contract can remove power before the front panel reaches its main screen. A reboot loop alone does not identify whether the cause is PD scheduling, protocol state, the source, or another boot fault.

Useful distinctions are:

- Power cycles before the main screen: preserve available boot-stage and reset evidence; a CLI connection may be impossible during the loop.
- The device remains responsive but a requested PPS voltage is not held: compare a later `status` sample's confirmed contract and VIN with the request. The immediate `set` reply may still contain the preceding current limit or VIN.
- Negotiation fails after renewal or a new request: inspect receive ordering, Source MessageID tracking, and whether a stale reply crossed an abandoned-request or rediscovery boundary.
- `pps clear` removes the manual flag but voltage stays at the manual setting: check the automatic-idle ticket through confirmation, not only the flag.

## Root-cause boundaries

### Service scheduling and shared I2C

PD work competes with display, persistence, network, and control-plane work on CPU0. A sleep added after an entire service turn lengthens the start-to-start interval; raising interrupt priority alone cannot preempt an EEPROM transaction that already owns the physically shared I2C bus. Moving a deep PD call chain into interrupt context also requires a combined interrupted-task and PD-frame stack check.

The PD protocol task and power coordinator therefore run on a priority-3 software-interrupt executor, started before the display stage. Each service turn targets a `5ms` start-to-start cadence by subtracting its own elapsed time. It tries the bus immediately; a busy bus skips the turn without consuming commands or claiming a fresh observation, then retries after `1ms`. An acquired turn performs bounded command and protocol work, releases the bus, and publishes only a status-verified snapshot. EEPROM operations release the bus between bounded transactions or chunks. The Xtensa build checks nested CPU0 stack headroom, including at least `64KiB` for nested calls and guard space. These measures make PD service higher priority than application work; they do not prove a hard `5ms` worst-case deadline under every bus or interrupt condition.

### Receive ordering and message identity

Checking a request timeout before draining completed receive frames can reject an `Accept` or `PS_RDY` that is already queued. A turn consumes at most three completed frames before checking the deadline, so it remains bounded while allowing the confirmation pair to complete. A rejected request is settled before another queued frame can affect its ticket.

Source MessageID is a three-bit sequence, not an identity for an entire contract. Fresh `Accept`, `PS_RDY`, `Reject`, and `Wait` responses to PPS renewal advance its position even while the contract is ready. Duplicate, old, and ambiguous half-range IDs must not confirm a later request; interleaved Source data also advances the sequence when an exchange is active. An abandoned request cannot lend its late reply to rediscovery. A rediscovery query is treated as sent only after an actual transmit, so a timestamp seeded for retry does not falsely fence a valid Source Capabilities message. Only a fresh `PS_RDY` after `Accept` installs the new contract.

### Power-preserving recovery

For this PPS source, automatic GoodCRC and locally initiated headers use PD 3.0; PD 2.0 can prevent usable APDO advertisement. An active PPS request is renewed every five seconds. A missing reply to an unchanged renewal bounds the probe state without discarding a still-confirmed supply; real VBUS loss, transport fault, or protocol reset still revokes authorization and turns heating off. VIN measurements do not confirm or invalidate a contract: PPS current limiting can lower delivered voltage while the accepted request remains valid.

A late Source Capabilities response alone is not grounds for a Sink Soft or Hard Reset. Local `RETRY_FAIL`, request timeout, and partial receive faults clear contract authorization, preserve Rd and the CC session, flush the affected FIFO, and retry bounded capability discovery. Reinitializing the PHY or restarting CC toggling during such recovery can remove the same VBUS that powers the MCU. Any observed low VBUS immediately interlocks heating; a controller-reported transition starts a short continuous-low confirmation before entering the VBUS-restore wait. That wait retains Rd and the configured CC/RX session, then resumes bounded capability discovery after VBUS returns.

When a new heater arm is held at zero duty while the working contract is pending, that frame is not complete idle: the arm intent is active and the output is intentionally blanked by the safety gate. The runtime must keep the power-negotiation stage active, request the selected PPS/APDO ceiling, and wait for protocol-confirmed `Accept` plus `PS_RDY`; routing the same frame through the generic zero-output idle reset falls back to `5V` and prevents PPS bootstrap. The FUSB302B service snapshot is the authoritative early capability source: if it becomes valid before the UI-facing power-state view, the runtime must reselect the PPS heater backend from that snapshot before the first arm decision.

### Manual PPS release

Clearing an override is an explicit Idle operation, not a fire-and-forget request. The controller retains its ticket, retries failed admission or terminal outcomes, and waits for the automatic idle contract before releasing manual ownership and resuming automatic thermal requests. Heater PWM remains off while restoration is pending. The expected idle voltage must come from the running firmware's policy and live source capabilities; it must not be copied from a previous HIL receipt.

## Reuse and validation

1. Identify the exact running firmware through `gitSha` and `buildId` when the device is responsive. When it is rebooting before CLI access, use available boot-stage/reset evidence and static code analysis; do not repeatedly flash known images to diagnose the symptom.
2. Separate a scheduling or shared-bus delay from a protocol-state error. Review the PD turn cadence, bus ownership, bounded receive loop, timeout order, MessageID freshness, and actual rediscovery transmit boundary. Keep the heater interlock and fault paths enabled while diagnosing.
3. Exercise the focused firmware tests for queued confirmations, rejection and replacement, keepalive wraparound, stale replies, interleaved data, rediscovery, and idle restoration. Build the Xtensa image and run `scripts/check-firmware-boot-stack.sh` when changing the interrupt executor or its call chain.
4. Follow the [PD Sink HIL procedure](../../specs/fusb302b-dual-pd-sink/HIL.md) on the intended image and an authorized exact port. With heating off, confirm boot and the firmware-specific automatic idle baseline; request supported 15 V, 17.3 V, and 20 V PPS points, hold 17.3 V for at least 60 seconds under read-only CLI load, then clear and confirm restoration. Test 20 V / 5 A only when the fixture supports it. Record `pdState`, confirmed contract kind/voltage/current, VIN, uptime, manual override, and physical heater output from independent settled samples.

`pdContractCurrentMa` and `pdContractPowerMw` are negotiated limits, not measured load. Stable CLI samples show the observable result within their sampling windows; they do not measure every PD packet, prove a worst-case service interval, or establish front-panel interaction between samples. A complete HIL result records those observations separately.

## References

- [PD Sink specification](../../specs/fusb302b-dual-pd-sink/SPEC.md) and [implementation map](../../specs/fusb302b-dual-pd-sink/IMPLEMENTATION.md)
- [ESP32-S3 startup fault isolation](esp32-s3-startup-fault-isolation.md) for non-PD boot faults and display/startup ordering
- [PD service task](../../../firmware/src/bin/flux_purr/pd_service.rs), [FUSB302B runtime](../../../firmware/src/bin/flux_purr/adc.rs), and [Sink policy](../../../firmware/src/adapters/fusb302b.rs)
