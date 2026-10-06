# Braking-first Approach HIL

## Artifact and boundaries

Firmware `0.27.6-dev.7cf64a9` was built from signed commit `7cf64a98ad39b4a51b8210e9e954ecc804d8fb8c`. ELF SHA-256 is `6e6744cccc06f08a94af5c91a6911345abb35a37bced51585650cc6c268b2f4d`; application size is 2,028,240 of 2,097,152 bytes. The repository CLI reported a successful flash with espflash exit code zero on the exact authorized serial port. Separate USB-C off/on commands on the authorized IsolaPurr source exited manual download mode. The original serial path returned, and application identity confirmed the same version and source SHA. Readback confirmed PPS 12V/5A, an active calibrated model, `100w/pps5a`, HOT=LOW and idle zero output.

The explicitly authorized backup bypass created no EEPROM archive; EEPROM health is unknown. No PD code changed and no tuning candidate was saved. Warmup, physical Hold admission, original deadlines, finite conditional caps, stable-window duration and independent confirmation gates retain their existing rules.

## 60°C diagnostic evidence

Run `firmware-pps5a-1791287848345` produced the standard report at `firmware/target/hil-braking-approach-60c-bundle/index.html`. Trial 0 scout and trial 1 current-point comparison failed the stable-window gate at overshoot 1.62/1.61°C. Trial 2 predicted point passed the short trial with a stable-window start 6,629ms after Warmup, overshoot 1.34°C and Hold peak-to-peak 1.83°C. Trial 3 independently confirmed that same candidate: stable-window start 6,957ms against the unchanged 10,000ms limit, full 60-second Hold, overshoot 1.28°C and peak-to-peak 1.75°C. The stable window was independently verified from raw samples; no conditional extension was used.

All four admission temperatures were at or below 35°C: 33.31, 34.95, 34.92 and 34.97°C. Each first Approach sample requested 50 permille. All 132 Approach samples were positive, with requests 50..360 permille and no zero request. Physical Hold entries were 59.63, 59.73, 59.85 and 59.53°C, meeting the fixed 59.5°C lower entrance. All 1,808 cooldown samples had zero logical/MOS heater output and the full-voltage fan command (enabled, high level, PWM 0 permille).

The accepted 60°C point uses brake distance 11.8°C, Approach lead 7s and damping 4000 permille. Its retained configuration floor/ceiling are 135/370 permille, while the effective model floor is the independent 50 permille. Identity evidence preserves those separate facts. The last 50 seconds of confirmed Hold have a time-weighted logical-request mean of 56.5574 permille, including off periods. This mean is diagnostic evidence and does not introduce an additional acceptance or persistence gate; the approved Approach minimum remains 5%.

Device events `0..2426` are complete and gap-free; the verified device digest is `a62586974876b1c59bfe2c48a61a51e6549257750ee2766b297d087cdaeddcf7`. The merged report trace digest is `sha256:5a7c4e7412d32b8dea8a6fd25daa670ca1832005257b887f3a4c5a705ee646c7`, and all five standard file digests verify. Candidate identity is `sha256:36a748e2b27a97afafa32b7f2117bd94c52623cf131a7657cd3dcb4ecce7230a`.

The 60°C target review is passed. The diagnostic report's overall owner outcome remains failed because a single-target run cannot establish the nine-target qualification or authorize persistence.

## Full-scope verification

The first full-scope start, `firmware-pps5a-1791288454516`, was safely rejected before heating with `calibration_conflict`: the previous diagnostic RAM preview was still present. Standard CLI `thermal profile clear-preview` removed it. Full-scope run `firmware-pps5a-1791288579769` imported the diagnostic candidate through the canonical seed/hash checks and completed the default execution order `60,240,140,100,80,120,180,160,220°C`. The standard report is `firmware/target/hil-braking-approach-full-retry-bundle/index.html`.

All nine adopted candidates passed an independent 60-second confirmation. Each stable-window start met the original deadline; none used conditional extension. Raw-sample verification confirms each full continuous 10-second ±1.5°C window. Failed comparisons and confirmations remain in the report, including the rejected 240°C confirmation at 3.28°C peak-to-peak and the rejected 140°C confirmation with a late stable-window start.

| Target | Trials | Adopted trial | Stable-window start / original limit | Overshoot | Hold p2p | Independent Hold |
| --- | ---: | ---: | --- | ---: | ---: | ---: |
| 60°C | 2 | 1 | 6.722 / 10.0s | 1.26°C | 1.69°C | 60s |
| 80°C | 7 | 6 | 7.331 / 10.0s | 1.12°C | 1.62°C | 60s |
| 100°C | 7 | 6 | 6.130 / 10.0s | 1.08°C | 1.75°C | 60s |
| 120°C | 4 | 3 | 5.398 / 10.0s | 1.17°C | 1.43°C | 60s |
| 140°C | 4 | 3 | 4.415 / 10.0s | 1.00°C | 1.37°C | 60s |
| 160°C | 2 | 1 | 4.487 / 5.0s | 1.33°C | 2.22°C | 60s |
| 180°C | 2 | 1 | 2.668 / 5.0s | 0.62°C | 1.52°C | 60s |
| 220°C | 2 | 1 | 1.904 / 5.0s | 0.46°C | 1.90°C | 60s |
| 240°C | 6 | 5 | 1.664 / 5.0s | 0.73°C | 2.67°C | 60s |

The 36 trials contain 1,114 Approach samples: logical request and applied MOS output both remain at or above 50 permille, with no zero samples. All 36 Warmup-to-Approach transitions start at 50 permille. All 37 observed Approach-to-Hold entries meet the fixed target−0.5°C / target+3°C physical admission gate. All 6,903 cooldown samples retain zero logical/MOS heater output and the full-voltage fan command. These are reported applied commands, not independent fan-RPM or oscilloscope measurements. Every Approach sample records a confirmed 5A contract. Trial admission records preserve the actual temperatures and the approved 35°C / target−40°C / target−30°C / target−20°C boundaries.

The full-scope 60°C confirmation's final 50 seconds have a time-weighted mean request of 56.1516 permille, including off periods. This mean is diagnostic evidence, without an additional floor-to-mean qualification condition. The approved Approach minimum remains 5%.

The complete event sequence is `0..15233`, with 14,969 samples and no gap. Device digest `53079f1da4aa0f84233eaabd6567ac86fffec948179be3fc30162ba375f8533b` and report trace digest `sha256:16c9f0b1a05272e6be44c83ad6be510853475ab3be4fdc7dbdede79470c83afa` verify. All five file digests verify; self-bundle verification nulls only its own digest while preserving the renderer's numeric spellings.

Candidate `firmware-pps5a-1791288579769-candidate` has hash `sha256:0e948394efffce83b4fadf0edf8f046c0a2f31aea79bd1b9083feadb0efe9280` and a matching RAM preview receipt. The device terminal is `candidate_ready`, review is complete, and every target review passes. Overall `ownerOutcome=failed` remains accurate under the locked report contract because there is no EEPROM save/readback receipt. Persistence requires the separate, identity-bound second confirmation; no candidate was automatically saved.

Post-run readback confirms idle mode, heater disabled, zero logical/physical output, retained candidate RAM preview and 12V/5A fallback. At 47.84°C the full-voltage fan command remains active. The thermal control-loop computation takes 43ms while the measured update interval is 70ms; those values describe different quantities.

Final readback at 36.28°C confirms zero heater output and the fan released after crossing the terminal 40°C threshold. Uptime advances to 4,527 seconds with no observed restart; the matching candidate remains in RAM awaiting its separate save confirmation.
