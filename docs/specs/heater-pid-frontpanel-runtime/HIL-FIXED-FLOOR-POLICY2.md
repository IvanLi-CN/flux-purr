# Fixed-floor timing policy 2 HIL

## Artifact and scope

The exact-port no-backup flash completed successfully and application identity independently read back `0.27.6-dev.2655786`, source `2655786c6fbdf6eb8e84d6dc00bbec585775a036`, build ID `2655786c6fbdf6eb`, on device `288485681c38`. The ELF SHA-256 is `ddab99719961f601045ed41243d1caad9810020b964e94095d270112e253c333`. A controlled USB-C off/readback/on sequence preceded application verification; the original authorized serial path restored. No alternate MCU port was selected. EEPROM backup was explicitly bypassed, no developer backup archive exists, and this operation does not establish EEPROM health.

Run `firmware-pps5a-1791172512185` uses the default nine-target `pps5a` scope through the repository CLI and devd. Preflight verified a usable thermal model, `100w/pps5a`, HOT=LOW, 12V/5A PPS and zero heater output. The tuning caller subsequently established its 21V/5A ceiling without changing the PD module. The standard renderer produced the five report files from the recorded device inputs; no report-specific presentation or fabricated samples were used.

## Result

| Target | Observed result | Qualification limit |
| --- | --- | --- |
| 60°C | One rejected scout; the subsequent retry never started before the unchanged 20-minute target budget expired during cooldown | No confirmation or accepted endpoint |
| 240°C | Prediction passed the short trial, then trial 3 passed a separate 60-second confirmation | Independent endpoint acceptance only |
| 80/100/120/140/160/180/220°C | No trials; pruned because the accepted 60°C endpoint was missing | Cannot supply thermal qualification |

The report owner outcome is `failed`, review disposition is `complete`, and the final candidate promotion is `rejected`. No candidate EEPROM save or save receipt occurred. A valid 240°C endpoint cannot establish complete default-scope qualification.

The 60°C scout began at 34.94°C after 339,832ms of initial heater-off cooling. It ended at elapsed 355,350ms with 5.18°C overshoot, 5.23°C qualified-Hold ripple and no verified stable window. Both thermal metrics exceed the unchanged 3°C gates. Its configuration/effective floor was 160‰, and the first original-deadline check was in Hold, so it did not extend. The next cooldown continued for 844,858ms, ending at elapsed 1,200,208ms with a 35.95°C reading and zero logical/MOS output. It had not met the ≤35°C retry gate; the runtime recorded `target_budget_exhausted_during_cooldown` and advanced to the independent 240°C endpoint. These facts do not establish the physical cause of the cooling plateau, a temperature-conversion defect or an unavoidable hardware limit.

The adopted 240°C confirmation began at 198.98°C against its 200°C start gate. It records 0.77°C overshoot, 1.80°C qualified-Hold ripple and 60,164ms of qualified Hold. Its stable-window start was 2,049ms after the first Warmup exit, within the unchanged 5,000ms deadline. The retained verified interval contains 274 samples over 61,706ms, averaging 4.424Hz, with temperatures 238.62–240.77°C inside the 238.50–241.50°C band. The configured/effective floor was 1000‰; acceptance used the ordinary path and did not obtain an extension. The standard host projection verifies its timing directly from the trial samples.

## Integrity and safety evidence

- All 7,126 device events cover `0..7125`, split into 7,081 samples and 45 ledger events. Device digest recomputation, merged report trace and all five file digests match.
- Report trace digest: `sha256:b4ee779e2bec9ad68f527fff25a657cb3902600d0501817e39203b97e2e31219`.
- Five closed trials use timing policy 2; none activated an extension. The host verifies the two successful short/confirmation trial windows and rejects the other three unverified windows. Positive 100‰ conditional-extension HIL remains unexercised.
- Observed Hold entries satisfy the shared target −0.5°C / ±3°C physical gate. All 5,370 cooldown samples record zero logical and MOS output.
- Of 207 runtime readbacks correlated with a cooldown event-page cursor, 206 retain full-fan command endpoints. One readback follows cursor 6201 but already observes the next Warmup with heater enabled, zero startup MOS output and fan off. Events 6203–6206 establish the intervening cooldown completion and Warmup transition. The snapshot and event-page cursor are not simultaneous; that boundary fact is retained separately and is not presented as a cooldown fan violation or as proof of electrical fan speed.
- Post-terminal readback at 143.15°C verifies heater disabled, both outputs zero, preview cleared, HOT=LOW, 12V/5A fallback and full post-heat fan. The device subsequently reached 39.47°C, released the default-scope <40°C terminal cooling gate, and read back heater disabled, MOS zero, preview cleared and fan off. The last pre-disarm trial sample is not used to prove terminal actuator state.

## Evidence locations and remaining gates

Raw inputs remain under `firmware/target/hil-fixed-floor-policy2-runs/firmware-pps5a-1791172512185/`; the unmodified standard report is under `firmware/target/hil-fixed-floor-policy2-bundle/`. Flash, preflight, integrity, timing, Hold-entry and terminal-cooling evidence use the `firmware/target/hil-fixed-floor-policy2-*` prefix. These ignored local artifacts are distinct from checked-in software fixtures.

This run establishes exact-artifact startup, ordinary 240°C acceptance, ineligible-extension refusal, complete failure recording and terminal disarm/cooling. It does not establish a passing nine-target run, the real eligible-extension path, induced-fault coverage, candidate persistence, UI visual acceptance or merge-ready delivery. The 60°C cooling prerequisite and its rejected candidate remain explicit unresolved facts; qualification gates are not relaxed to hide them.
