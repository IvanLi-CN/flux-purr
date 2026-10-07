# PD Fan Test

## Goal

Provide a standalone ESP32-S3 RAM image and host command for testing a fan with
an owned USB-PD sink contract. The session is independent from the general RAM
bring-up image and from the full PD voltage HIL image.

## Contract

- `firmware/ram-fan-pd` is a separate ELF with `imageKind=fan_pd`, capability
  `test_fan_pd`, and its own PD, fan-output, and recovery state machine.
- `ram-run test fan-pd` selects a contract in this order: `15V Fixed`, `15V
  PPS`, `18V Fixed`, `18V PPS`, `20V Fixed`, `20V PPS`; then the highest usable
  Fixed PDO from `5V..12V`; then, only when no usable Fixed PDO exists, the
  highest representable PPS voltage in `5V..20V` rounded down to the protocol
  encoding step.
- A request uses `min(sourceAdvertisedMaxMa, 5000)` for Fixed and PPS. Zero,
  unencodable, or otherwise unusable candidates are skipped. The fan test has
  no 3A minimum-current requirement.
- After `Accept` and `PS_RDY` confirm the selected contract, the device emits a
  contract progress frame containing mode, voltage, current, selection reason,
  and the `>13V` power-gate result. The host prints that frame before stage 1.
- A confirmed voltage above `13V` passes the power gate. `13V` or below emits a
  warning, still runs the fan stages, and reports `power_degraded` with a
  non-zero CLI exit code.
- The fan output runs 50% PWM for 5 seconds, 100% PWM for 5 seconds, and 0%
  PWM for 5 seconds before safe-off. No tachometer, RPM, or speed judgement is
  part of this contract.
- Every terminal path turns the fan output off and attempts the verified Fixed
  5V recovery contract. A recovery failure overrides any earlier result with
  `recovery_failed`.
- `ram-run test fan` remains the PD-untouched general bring-up test. The host
  prints a warning recommending `fan-pd`; it never starts PD negotiation.

## Boundaries

The `flux-purr-pd-sink-core` crate contains only transport-neutral `no_std`
wire decoding, contract request encoding, message headers, and response data
types. The fan policy and execution state machine live in `ram-fan-pd`; the
full HIL matrix, 3A threshold, voltage hold, VIN acceptance, and evidence
workflow remain in `ram-pd-hil`. Neither RAM executable dispatches the other.

## Commands

```text
flux-purr ram-run test fan-pd --port <authorized-port>
flux-purr ram-run test fan --port <authorized-port>
```

## Acceptance

Formal acceptance requires deterministic policy, protocol, identity, recovery,
CLI, and ELF checks in the repository. Physical acceptance additionally
requires an owner-authorized exact serial port and a real USB-C PD source; the
host must observe the negotiated voltage immediately before stage 1.
