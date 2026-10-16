# Model Approach HIL

## Initial artifact evidence

Firmware `0.27.6-dev.e18b8ad`, ELF SHA-256 `a45a69c1df8119ed4e0b6274cd669bb1d7517f82945b2ec5f379b6192084bd8c`, was flashed through the repository CLI on the exact authorized serial port. Flash completed with espflash exit code zero; application identity confirmed the same source commit. The explicitly authorized backup bypass created no EEPROM archive, so EEPROM health is unknown.

The first start was safely rejected with `pps_contract_unavailable` while the sink exposed only a 3A capability. Separate USB-C off/on commands on the authorized IsolaPurr source were accepted; off readback showed the USB-C path disabled with no protocol, and subsequent device status confirmed PPS5A, the active model, HOT=LOW and zero heater output. No PD code was changed.

Diagnostic run `firmware-pps5a-1791279259300` retained two closed 60°C trials. Trial 0 overshoot/p2p were 3.80/4.27°C; trial 1 values were 3.57/3.69°C. Neither passed the unchanged thermal and stability gates. The run was canceled while cooling after reproducing the failure, with sequence `0..962` retained and no save. The standard five-file report is `firmware/target/hil-model-approach-60c-retry-bundle/index.html`; its outcome is failed, not qualification. Terminal readback confirmed 33.34°C, heater disabled, MOS zero and preview cleared.

The captured Approach request stayed at 290 permille over 48.03–55.02°C before dropping. A production-path regression reproduced identical 31% output for damping values 1000 and 4000. The cause was reuse of the 3A plant PI's fixed rise-rate cap, which ignored the 5A candidate damping. Its timing metadata also described the obsolete profile-derived floor rather than the model's actual 100-permille minimum.

## Corrected control and acceptance

The shared 5A production state machine derives Approach energy from remaining predicted temperature error and calibrated passive loss/capacity, with point-local lead and damping. Timing policy 3 binds the actual model floor and preserves the original bounded conditional-extension criteria; historical policy 2 evidence is not rewritten. Regressions cover damping influence, independence from profile floor/ceiling, a real unraised floor witness, mixed-policy rejection and Rust/Web digest parity.

Firmware `0.27.6-dev.d9dad02` was built from commit `d9dad02e`; ELF SHA-256 is `0d249202229fab620175e22de24cbce31c63b91773fd44d33a21bf61ee280f6b`. Its application size is 2,027,904 of 2,097,152 bytes. The first authorized no-backup flash attempt reached only the connection phase and failed with USB `Broken pipe`; after a second manual download-mode transition, the same exact port flashed successfully with espflash exit code zero. No EEPROM archive exists, so EEPROM health remains unknown.

The corrected artifact identity was confirmed on-device as `0.27.6-dev.d9dad02`, commit `d9dad02ee823f873d76ff0678e387313531e7d0c`; status confirmed PPS `12V/5A`, the calibrated model, `100w/pps5a`, HOT=LOW and a safe idle state before tuning.

Diagnostic run `firmware-pps5a-1791282023955` used the standard five-file renderer and complete trace `0..2077`. Four 60°C trials were rejected by the unchanged `10s` full-speed-to-stable gate; overshoot/p2p for the first two were `1.79/2.16°C` and `1.56/2.14°C`, but no continuous 10s stable window was verified. The best trial's stable-window start was `10.067s`, just beyond the `10s` limit. The predicted candidate reduced Approach parameters but did not establish a qualifying window; the engine ended with `candidate_search_exhausted_no_distinct_safe_proposal`. No 60-second confirmation or EEPROM save occurred. The report is `firmware/target/hil-model-damped-approach-60c-bundle/index.html` and correctly reports `failed`, not qualification. Final readback was `33.61°C`, idle, heater/MOS off and preview cleared. The default nine-target HIL was not started because the 60°C prerequisite failed.

This run confirms the model law no longer has the former 3A fixed-rate plateau and reduced thermal overshoot, but it does not yet prove an accepted candidate. The remaining failure is timing: the current candidate reaches the physical Hold phase and the ±1.5°C band, yet the band remains valid too late or for too short a continuous interval. The report and raw events retain that distinction; no gate was relaxed.
