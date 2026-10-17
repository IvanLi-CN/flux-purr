#!/usr/bin/env python3
"""Generate a deterministic, non-uniform pps5a thermal-tuning-v2 input bundle.

This creates recorder-shaped input only. The canonical HTML report is still
produced by `flux-purr thermal report render-v2`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path


TARGETS = [60, 80, 100, 120, 140, 160, 180, 220, 240]
EXECUTION_ORDER = [60, 240, 140, 100, 80, 120, 180, 160, 220]
RUN_ID = "mock-firmware-v2-realistic-pps5a-20260925"
DEVICE_ID = "mock-device-pps5a-thermal-bench-01"
FIRMWARE_VERSION = "mock-firmware-v2.0.0+thermal"
PPS_POLICY = "pps-policy-v2"
APDO_MAX_MV = 28000


def digest(value: object) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return f"sha256:{hashlib.sha256(encoded).hexdigest()}"


def pps_contract_mv(target: int) -> int:
    if target <= 100:
        return 21000
    if target <= 160:
        return 22000
    if target <= 220:
        return 24000
    return 25000


def candidate_point(target: int, trial_index: int = 1) -> dict[str, int]:
    ratio = (target - 60) / 180.0
    trial_bias = 12 if trial_index == 0 else 0
    return {
        "targetC": target,
        "warmupPowerPermille": 1000,
        "warmupReenterCentiC": int(920 + ratio * 170),
        "brakeDistanceCentiC": int(560 + ratio * 260 + trial_bias),
        "approachPowerPermille": int(590 - ratio * 150 - trial_bias),
        "approachFloorPowerPermille": int(235 + ratio * 120),
        "approachDampingExponentPermille": int(820 + ratio * 120),
        "approachTailWindowCentiC": int(70 + ratio * 60),
        "holdPowerPermille": int(170 + ratio * 120 + trial_bias),
        "holdReheatPowerPermille": int(225 + ratio * 150 + trial_bias),
        "holdEntryCentiC": int(95 + ratio * 65),
        "holdExitCentiC": int(55 + ratio * 45),
        "holdOnCentiC": int(9 + ratio * 6),
        "holdOffCentiC": int(105 + ratio * 85),
        "overshootCutoffCentiC": int(135 + ratio * 130),
        "holdKpPermillePerC": int(20 + ratio * 10),
        "holdKiPermillePerCTick": 1,
        "holdBlendTicks": int(2 + ratio * 3),
        "approachLeadTicks": int(3 + ratio * 3),
        "holdLeadTicks": int(1 + ratio * 3),
    }


def accepted_candidate_hash() -> str:
    return digest({"powerClass": "pps5a", "points": [candidate_point(target) for target in TARGETS]})


def candidate_identity(target: int, trial_index: int) -> tuple[str, str, dict]:
    point = candidate_point(target, trial_index)
    if trial_index == 1:
        candidate_id = "mock-pps5a-candidate-realistic-v1"
        candidate_hash = accepted_candidate_hash()
    else:
        candidate_id = f"mock-pps5a-{target}c-trial-{trial_index}-rejected"
        candidate_hash = digest(point)
    canonical = {
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": "pps5a",
        "point": point,
    }
    return candidate_id, canonical["candidateHash"], canonical


def phase_name(elapsed_ms: int, warmup_ms: int, approach_ms: int) -> str:
    if elapsed_ms < warmup_ms:
        return "warmup"
    if elapsed_ms < warmup_ms + approach_ms:
        return "approach"
    return "hold"


def realistic_sample(
    target: int,
    trial_index: int,
    elapsed_ms: int,
    warmup_ms: int,
    approach_ms: int,
    total_ms: int,
    candidate_id: str,
    candidate_hash: str,
) -> dict:
    phase = phase_name(elapsed_ms, warmup_ms, approach_ms)
    ambient_c = 23.5 + 1.7 * math.sin(target / 31.0) + 0.4 * math.cos(target / 17.0)
    contract_mv = pps_contract_mv(target)
    phase_time = max(0, elapsed_ms - warmup_ms)
    hold_start = warmup_ms + approach_ms
    hold_time = max(0, elapsed_ms - hold_start)

    if phase == "warmup":
        progress = min(1.0, elapsed_ms / max(1, warmup_ms))
        curve = 1.0 - math.exp(-3.15 * progress)
        temperature_c = ambient_c + (target - ambient_c - 7.5) * curve
        logical = 1000
        mos_pwm = 968 + int(10 * math.sin(elapsed_ms / 7800.0 + target / 23.0))
    elif phase == "approach":
        progress = min(1.0, phase_time / max(1, approach_ms))
        entry = target - 7.5
        overshoot = (0.55 + target / 520.0) * math.sin(math.pi * progress) ** 1.7
        temperature_c = entry + 7.5 * progress + overshoot
        logical = int(580 - 235 * progress + 18 * math.sin(target / 29.0))
        mos_pwm = logical - 22 + int(9 * math.sin(elapsed_ms / 4400.0))
    else:
        decay = math.exp(-hold_time / 26000.0)
        ripple = (0.35 + target / 520.0) * math.sin(hold_time / (4700.0 + target * 4.0))
        temperature_c = target + 0.7 * decay + ripple
        hold_base = candidate_point(target)["holdPowerPermille"]
        logical = hold_base + int(20 * math.sin(hold_time / 6500.0 + target / 22.0))
        mos_pwm = logical - 11 + int(7 * math.cos(hold_time / 5200.0))

    # The rejected run deliberately carries one realistic failure mode per band.
    if trial_index == 0:
        failure_mode = target % 3
        if phase == "approach" and failure_mode == 0:
            progress = min(1.0, phase_time / max(1, approach_ms))
            temperature_c += 8.0 * math.sin(math.pi * progress) ** 0.65
        if phase == "hold" and failure_mode == 1:
            temperature_c += 2.6 * math.sin(hold_time / 1400.0)
        if phase == "approach" and failure_mode == 2:
            temperature_c -= 2.2 * (1.0 - min(1.0, phase_time / max(1, approach_ms)))
        logical = min(1000, logical + 55)
        mos_pwm = min(1000, mos_pwm + 38)

    vin_mv = int(
        12010
        + 115 * math.sin(elapsed_ms / 9300.0 + target / 41.0)
        - (target - 60) * 0.55
        + 28 * math.cos(elapsed_ms / 3700.0)
    )
    requested_mv = contract_mv
    if phase == "hold":
        requested_mv = max(12000, contract_mv - int(1000 + target * 2.2))
    if trial_index == 0 and phase == "approach":
        requested_mv = contract_mv

    sample = {
        "elapsedMs": elapsed_ms,
        "kind": "sample",
        "targetC": target,
        "trialIndex": trial_index,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "temperatureCentiC": round(temperature_c * 100),
        "vinMv": vin_mv,
        "ppsContractMv": contract_mv,
        "ppsContractMa": 5000,
        "ppsContractKind": "pps",
        "ppsContractConfirmed": True,
        "ppsRequestedMv": requested_mv,
        "ppsPowerCeilingMv": APDO_MAX_MV,
        "heaterRequestedOutputPermille": max(0, min(1000, logical)),
        "heaterOutputPermille": max(0, min(1000, mos_pwm)),
        "measurementValid": True,
        "phase": phase,
        "pidPhase": phase,
    }
    if elapsed_ms == total_ms:
        sample["terminal"] = "heater_disarmed"
    return sample


def run_shape(target: int, trial_index: int) -> tuple[int, int, int]:
    ratio = (target - 60) / 180.0
    if trial_index == 0:
        warmup_ms = int((18000 + ratio * 12000) // 2000 * 2000)
        approach_ms = int((11000 + ratio * 9000) // 2000 * 2000)
        hold_ms = int((12000 + (target % 4) * 2000) // 2000 * 2000)
    else:
        warmup_ms = int((30000 + ratio * 31000 + 1800 * math.sin(target / 24.0)) // 2000 * 2000)
        approach_ms = int((18000 + ratio * 15000 + 1600 * math.cos(target / 28.0)) // 2000 * 2000)
        hold_ms = int((60000 + (target % 5) * 2000) // 2000 * 2000)
    return warmup_ms, approach_ms, hold_ms


def build_bundle(output_dir: Path) -> None:
    samples: list[dict] = []
    trial_records: dict[tuple[int, int], dict] = {}
    sequence = 0

    for target in TARGETS:
        for trial_index in (0, 1):
            candidate_id, candidate_hash, canonical = candidate_identity(target, trial_index)
            warmup_ms, approach_ms, hold_ms = run_shape(target, trial_index)
            total_ms = warmup_ms + approach_ms + hold_ms
            interval_ms = 3500 + (target % 4) * 500
            times = list(range(0, total_ms, interval_ms))
            if not times or times[-1] != total_ms:
                times.append(total_ms)

            first_sequence = sequence
            for elapsed_ms in times:
                sample = realistic_sample(
                    target,
                    trial_index,
                    elapsed_ms,
                    warmup_ms,
                    approach_ms,
                    total_ms,
                    candidate_id,
                    candidate_hash,
                )
                sample["sequence"] = sequence
                samples.append(sample)
                sequence += 1
            last_sequence = sequence - 1
            ratio = (target - 60) / 180.0
            run_samples = samples[first_sequence:sequence]
            non_warmup = [sample["temperatureCentiC"] / 100.0 for sample in run_samples if sample["phase"] != "warmup"]
            hold_samples = [sample["temperatureCentiC"] / 100.0 for sample in run_samples if sample["phase"] == "hold"]
            measured_overshoot = round(max(0.0, max(non_warmup, default=float(target)) - target), 2)
            measured_hold_p2p = round(
                max(hold_samples, default=float(target)) - min(hold_samples, default=float(target)),
                2,
            )
            if trial_index == 0:
                mode = target % 3
                max_overshoot = measured_overshoot
                hold_p2p = measured_hold_p2p
                settle_ms = 10800 + (target % 4) * 1800
                disposition = "rejected"
                review_outcome = "failed"
                stop_reason = [
                    "overshoot hard gate exceeded",
                    "hold peak-to-peak hard gate exceeded",
                    "full-speed-to-stable budget exhausted",
                ][mode]
            else:
                max_overshoot = measured_overshoot
                hold_p2p = measured_hold_p2p
                if target <= 150:
                    settle_ms = 5600 + int(ratio * 2600) + (target % 3) * 350
                else:
                    settle_ms = 3600 + int(ratio * 800) + (target % 3) * 180
                disposition = "adopted"
                review_outcome = "passed"
                stop_reason = "candidate passed hard gates and 60s hold confirm"
            bounds = {"lowerTargetC": max(40, target - 20), "upperTargetC": min(260, target + 20)}
            trial_records[(target, trial_index)] = {
                "trialIndex": trial_index,
                "candidateId": candidate_id,
                "candidateHash": candidate_hash,
                "disposition": disposition,
                "reviewOutcome": review_outcome,
                "warmupRequestedOutputPermille": 1000,
                "maxOvershootC": max_overshoot,
                "holdPeakToPeakC": hold_p2p,
                "fullSpeedToStable": {
                    "settleTimeMs": settle_ms,
                    "limitMs": 10000 if target <= 150 else 5000,
                },
                "firstSequence": first_sequence,
                "lastSequence": last_sequence,
                "firstElapsedMs": 0,
                "lastElapsedMs": total_ms,
                "bounds": bounds,
                "candidateSeed": {
                    "source": "mock-realistic-interpolated-bounds",
                    **bounds,
                },
                "canonicalCandidate": canonical,
                "stopReason": stop_reason,
            }

    report_runs = []
    for target in TARGETS:
        adopted = trial_records[(target, 1)]
        trials = [trial_records[(target, 0)], adopted]
        report_runs.append(
            {
                "targetC": target,
                "reviewOutcome": "passed",
                "reviewPassed": True,
                "adoptedTrialIndex": 1,
                "trialCount": 2,
                "metrics": {
                    "warmupRequestedOutputPermille": 1000,
                    "phasePath": "warmup -> approach -> hold",
                    "maxOvershootC": adopted["maxOvershootC"],
                    "holdPeakToPeakC": adopted["holdPeakToPeakC"],
                    "holdConfirmSeconds": 60,
                    "fullSpeedToStable": adopted["fullSpeedToStable"],
                },
                "trials": trials,
            }
        )

    candidate_points = [candidate_point(target) for target in TARGETS]
    candidate_id, candidate_hash, _ = candidate_identity(60, 1)
    receipt_identity = {
        "runId": RUN_ID,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": "pps5a",
    }
    candidate = {
        "schema": "thermal-tuning-v2",
        "runId": RUN_ID,
        "candidateId": candidate_id,
        "candidateHash": candidate_hash,
        "powerClass": "pps5a",
        "profile": {"bank": "pps5a", "points": candidate_points},
        "promotion": {
            "state": "saved",
            "previewReceipt": {
                **receipt_identity,
                "ramAppliedHash": candidate_hash,
                "evidenceSource": "mock-local-recorder",
            },
            "saveReceipt": {
                **receipt_identity,
                "bank": "pps5a",
                "bankRevision": 17,
                "bankHash": candidate_hash,
                "atomic": True,
                "readbackVerified": True,
                "evidenceSource": "mock-local-recorder",
            },
        },
    }

    ledger: list[dict] = []
    ledger_sequence = len(samples)
    for target in TARGETS:
        for trial_index in (0, 1):
            trial = trial_records[(target, trial_index)]
            ledger.append(
                {
                    "sequence": ledger_sequence,
                    "kind": "candidate_trial",
                    "targetC": target,
                    "trialIndex": trial_index,
                    "candidateId": trial["candidateId"],
                    "candidateHash": trial["candidateHash"],
                    "bounds": trial["bounds"],
                    "candidateSeed": trial["candidateSeed"],
                    "canonicalCandidate": trial["canonicalCandidate"],
                    "summary": "candidate trial materialized from local device evidence",
                }
            )
            ledger_sequence += 1
            for phase in ("warmup", "approach", "hold"):
                ledger.append(
                    {
                        "sequence": ledger_sequence,
                        "kind": "phase_transition",
                        "targetC": target,
                        "trialIndex": trial_index,
                        "phase": phase,
                        "summary": f"entered {phase} phase",
                    }
                )
                ledger_sequence += 1
            ledger.append(
                {
                    "sequence": ledger_sequence,
                    "kind": "decision",
                    "targetC": target,
                    "trialIndex": trial_index,
                    "disposition": trial["disposition"],
                    "score": {
                        "maxOvershootC": trial["maxOvershootC"],
                        "holdPeakToPeakC": trial["holdPeakToPeakC"],
                        "settleTimeMs": trial["fullSpeedToStable"]["settleTimeMs"],
                    },
                    "hardGates": {
                        "overshootC": 3.0,
                        "holdPeakToPeakC": 3.0,
                        "fullSpeedToStableMs": trial["fullSpeedToStable"]["limitMs"],
                    },
                    "freeze": trial["disposition"] == "adopted",
                    "prune": trial["disposition"] == "rejected",
                    "stopReason": trial["stopReason"],
                }
            )
            ledger_sequence += 1
            ledger.append(
                {
                    "sequence": ledger_sequence,
                    "kind": "safety",
                    "targetC": target,
                    "trialIndex": trial_index,
                    "phase": "terminal",
                    "heaterState": "disarmed",
                    "summary": "trial ended with heater disarmed",
                }
            )
            ledger_sequence += 1

    all_events = sorted(samples + ledger, key=lambda event: event["sequence"])
    bundle = {
        "schema": "thermal-tuning-v2",
        "runId": RUN_ID,
        "engine": "firmware",
        "device": {"id": DEVICE_ID, "transport": "mock-local-recorder"},
        "firmware": {"version": FIRMWARE_VERSION, "build": "realistic-mock"},
        "powerClass": "pps5a",
        "sourceClass": "device-local",
        "ppsControlPolicyVersion": PPS_POLICY,
        "ppsVoltageBounds": {
            "absoluteMinMv": 5000,
            "absoluteMaxMv": 28000,
            "policyFloorMv": 12000,
            "apdoMinMv": 5000,
            "apdoMaxMv": APDO_MAX_MV,
        },
        "targetSet": {
            "physicalC": TARGETS,
            "selectedC": TARGETS,
            "executionOrderC": EXECUTION_ORDER,
        },
        "qualificationScope": {
            "kind": "default-pps5a-full-batch",
            "targetCount": len(TARGETS),
            "diagnosticOnly": False,
        },
        "terminalDisposition": "passed",
        "reviewDisposition": "passed",
        "deviceTrace": {
            "firstSequence": all_events[0]["sequence"],
            "lastSequence": all_events[-1]["sequence"],
            "complete": True,
            "gap": False,
            "digest": digest(all_events),
        },
        "deviceTraceVerification": {
            "verified": True,
            "source": "mock_fixture_synthetic",
            "reasons": [],
        },
        "generatedAt": "2026-09-25T08:00:00Z",
        "candidate": {
            "candidateId": candidate_id,
            "candidateHash": candidate_hash,
            "powerClass": "pps5a",
            "promotionState": "saved",
        },
        "referenceComparison": {
            "fixture": "external-5a-conformance-v1",
            "status": "mock_not_verified",
            "algorithmCopied": False,
        },
        "capability": {
            "localEvidence": {
                "vin": "available",
                "ppsContract": "available",
                "externalVbus": "unavailable",
                "physicalPower": "unavailable",
            }
        },
        "reportRuns": report_runs,
    }

    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "run.bundle.json").write_text(json.dumps(bundle, indent=2) + "\n")
    (output_dir / "thermal-profile.candidate.json").write_text(json.dumps(candidate, indent=2) + "\n")
    (output_dir / "samples.ndjson").write_text(
        "".join(json.dumps(sample, separators=(",", ":")) + "\n" for sample in samples)
    )
    (output_dir / "decision-ledger.ndjson").write_text(
        "".join(json.dumps(event, separators=(",", ":")) + "\n" for event in ledger)
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "output_dir",
        nargs="?",
        type=Path,
        default=Path("fixtures/thermal-tuning-v2/realistic-pps5a-input"),
    )
    args = parser.parse_args()
    build_bundle(args.output_dir)
    print(args.output_dir)


if __name__ == "__main__":
    main()
