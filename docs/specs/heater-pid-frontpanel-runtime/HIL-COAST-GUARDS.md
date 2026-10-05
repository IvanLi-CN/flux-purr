# Production coast guard correction HIL

## Artifact and operation

- Source: `0dfafa267fc07a71ef659f3ded9f087f335c68d0`.
- Device firmware: `0.27.6-dev.0dfafa2`; runtime build ID `0dfafa267fc07a71`.
- ELF SHA-256: `776714c32fa65d366c2419fbec157595654da0caf601253e6ca434e4ef097f8b`.
- Authorized device: `/dev/cu.usbmodem2111401`, device ID `288485681c38`, ESP32-S3/FUSB302B.
- Source: IsolaPurr `~v~`, `f293cc9c139e`, USB-C `port_c`.
- The automatic flash stopped at connect with Broken pipe. After owner preparation of manual download mode, the repository CLI completed the paired no-backup flash with `--keep-download-mode`. No EEPROM developer backup exists. Separate USB-C power-off/readback/power-on operations were followed by matching device firmware identity, a valid active thermal model and ready 12V/5A PPS readback.
- Software validation: both new regressions fail before the coast correction; 84 heater tests and 96 tuning/strategy/limits/protocol tests pass after it. Release build and boot-stack checks pass. PD source files are outside this correction.

## Run and outcome

Run `firmware-pps5a-1791187706371` uses the standard repository `thermal tune --engine firmware` path with the default nine-target scope. Its seed is retained data from this task; production normalization bounds the 60°C damping field to 4000‰ before candidate hashing.

| Target | Heating trials | Independent confirmation | Outcome |
| --- | --- | --- | --- |
| 60°C | 2 | 60,032ms qualified Hold; overshoot 0.80°C; peak-to-peak 1.58°C | Accepted target only |
| 240°C | 15 | None | Failed within the unchanged target budget |
| Other seven targets | 0 | None | Blocked by the unaccepted 240°C boundary |

The 60°C confirmation starts at 34.95°C against its 35°C limit. Its stable window begins 11,686ms after Warmup under a verified policy-2 extension with a real configured/effective/logical 100‰ witness and a 30,000ms cap. The 274 recorded stable samples cover 62,008ms at an average 4.40Hz, between 58.50°C and 60.80°C. The short scout separately establishes a timely window at 9,506ms with 1.07°C overshoot and 1.55°C ripple.

The 240°C candidate's explicit zero Approach lead is read back as zero while heating. The corrected 60°C candidate damping is read back as 4000‰. These observations validate the materialization corrections on the committed artifact. They do not establish complete controller conformance or successful full qualification.

The 240°C target tests only two distinct complete points. Trials 0–1 use the initial point; trials 2–14 repeat one saturated point with Approach and Hold/reheat configuration at 1000‰, Approach lead zero, brake distance 1.50°C and Hold lead 6. Trial 2 enters Hold at 240.15°C, reaches 240.73°C, then falls to 238.27°C. Its 0.73°C overshoot and 2.46°C ripple are below 3°C, but its stable-band window breaks below 238.50°C. It is correctly rejected. Later identical-point retries cannot be presented as further parameter optimization or as proof of a hardware capability limit.

## Integrity and cleanup

The device trace contains 6392 contiguous events over `0..6391`. Device-chain verification, the merged report trace and all five file digests pass. Report trace SHA-256 is `f7c89867966944e7bb109d805fb9138dc9a82f5ec80dd3cac099a8d3848da5ff`. The report retains `ownerOutcome=failed`, `reviewDisposition=complete` and the default nine-target qualification scope.

All 1312 cooldown samples have zero logical and MOS output. Runtime readbacks show full-speed fan commands during cooldown; the audit finds no physical Hold-entry or extension-policy violation. Device terminal status is failed, disarmed, unsealed, with `saveConfirmed=false` and no save receipt. Cleanup clears RAM preview. Post-run status on 2026-10-05 at 08:37:36 UTC reports 35.40°C, logical/MOS zero, fan off after post-heat cooling, HOT=LOW and ready 12V/5A PPS.

The standard five-file report and raw device records remain in this workspace:

- `firmware/target/hil-coast-0dfafa26-full-bundle/index.html`
- `firmware/target/hil-coast-0dfafa26-full-runs/firmware-pps5a-1791187706371/`
- `firmware/target/hil-coast-0dfafa26-full-audit.json`
- `firmware/target/hil-coast-0dfafa26-full-policy-audit.json`
- `firmware/target/hil-coast-0dfafa26-post-status.ndjson`

## Remaining timebase constraint

`update_coast_state` requires each of two adjacent raw observations to drop by more than 0.05°C. At the nominal 50ms control interval this implies more than 1°C/s cooling for those observations; it is not a rate-independent cooling confirmation. The added filtered-error guard retains that condition.

A deterministic host diagnostic against the same controller supplies smooth 0.5°C/s cooling at 20Hz. After 3.5s it reports actual low error 1.25°C, filtered low error 0.9044°C and filtered slope −0.4951°C/s, while retaining coast and zero output. The observed slow-cooling retention is reproducible without hardware, source negotiation or report rendering. Its source and output are retained in `firmware/target/coast-timebase-diagnostic.rs` and `firmware/target/hil-coast-timebase-diagnosis.log`; the diagnostic is not a qualification test. Device evidence lacks every adjacent control-frame observation, so it does not prove this mechanism is the sole cause of all 240°C failures.

Changing this third production condition requires its own explicit contract decision. The two approved guard corrections do not authorize silently replacing the raw-observation threshold, relaxing thermal/timing acceptance, or redesigning 5A braking around a calibrated cooling fit. Full-batch qualification and EEPROM promotion remain incomplete.
