#!/usr/bin/env python3

from __future__ import annotations

import json
import math
from pathlib import Path

import thermal_tuning_sprint as sprint


REPO_ROOT = Path(__file__).resolve().parents[1]
OUTPUT_DIR = REPO_ROOT / "thermal-self-test-runs" / "mock-pd100w-pps5a-final-multitarget-format-20260718"
SOURCE_RUNS_DIR = OUTPUT_DIR / "source-run-summaries"
TARGETS_C = [60, 80, 100, 120, 140, 160, 180, 220, 240]
ACCEPTED_PROFILE_PATH = (
    REPO_ROOT
    / "thermal-self-test-runs"
    / "baselines"
    / "56x56mm-3p2ohm-pd100w-pps5a"
    / "accepted-full-range-20hz-dryrun"
    / "thermal-profile.accepted.json"
)
DRYRUN_SAMPLES_PATH = (
    REPO_ROOT
    / "thermal-self-test-runs"
    / "baselines"
    / "56x56mm-3p2ohm-pd100w-pps5a"
    / "accepted-full-range-20hz-dryrun"
    / "samples.ndjson"
)


def read_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def target_timing(target_c: int) -> tuple[float, float, float]:
    warmup_end = round(8.0 + target_c * 0.11, 1)
    approach_end = round(warmup_end + 6.0 + target_c * 0.025, 1)
    total_end = round(approach_end + 18.0, 1)
    return warmup_end, approach_end, total_end


def accepted_metrics(target_c: int) -> dict:
    overshoot = {60: 1.10, 80: 1.25, 100: 1.40, 120: 1.55, 140: 1.70, 160: 1.85, 180: 2.00, 220: 2.35, 240: 2.60}[target_c]
    p2p = {60: 1.05, 80: 1.15, 100: 1.22, 120: 1.35, 140: 1.48, 160: 1.62, 180: 1.78, 220: 2.05, 240: 2.32}[target_c]
    settle_ms = {60: 6200, 80: 6600, 100: 7100, 120: 7600, 140: 8200, 160: 8700, 180: 9300, 220: 10400, 240: 11200}[target_c]
    approach_mae = {60: 0.62, 80: 0.68, 100: 0.74, 120: 0.82, 140: 0.90, 160: 0.96, 180: 1.02, 220: 1.18, 240: 1.29}[target_c]
    hold_median = {60: 92, 80: 108, 100: 122, 120: 136, 140: 149, 160: 163, 180: 178, 220: 201, 240: 218}[target_c]
    hold_p90 = hold_median + 22
    approach_source_w = {60: 42.0, 80: 48.0, 100: 54.0, 120: 60.0, 140: 66.0, 160: 72.0, 180: 78.0, 220: 86.0, 240: 90.0}[target_c]
    hold_source_w = {60: 5.0, 80: 5.8, 100: 6.5, 120: 7.2, 140: 8.1, 160: 9.0, 180: 10.1, 220: 11.8, 240: 12.9}[target_c]
    return {
        "overshoot": overshoot,
        "p2p": p2p,
        "settle_ms": settle_ms,
        "approach_mae": approach_mae,
        "hold_median": hold_median,
        "hold_p90": hold_p90,
        "approach_source_w": approach_source_w,
        "hold_source_w": hold_source_w,
    }


def load_effective_points() -> dict[int, dict]:
    points: dict[int, dict] = {}
    with DRYRUN_SAMPLES_PATH.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            sample = json.loads(line)
            target_c = int(sample["targetTempC"])
            if target_c not in TARGETS_C or target_c in points:
                continue
            point = sprint.sanitize_point(sample.get("heaterParameters"), target_c)
            if point is not None:
                points[target_c] = point
    missing = [target for target in TARGETS_C if target not in points]
    if missing:
        raise RuntimeError(f"missing effective heaterParameters for targets: {missing}")
    return points


def mutate_point(base_point: dict, round_number: int) -> dict:
    point = dict(base_point)
    if round_number == 1:
        point["brakeDistanceCentiC"] = int(point["brakeDistanceCentiC"]) - 90
        point["approachPowerPermille"] = int(point["approachPowerPermille"]) - 28
        point["approachFloorPowerPermille"] = max(180, int(point["approachFloorPowerPermille"]) - 22)
        point["holdEntryCentiC"] = int(point["holdEntryCentiC"]) + 24
        point["holdPowerPermille"] = max(0, int(point["holdPowerPermille"]) - 20)
        point["holdReheatPowerPermille"] = max(0, int(point["holdReheatPowerPermille"]) - 18)
    elif round_number == 2:
        point["brakeDistanceCentiC"] = int(point["brakeDistanceCentiC"]) - 26
        point["approachPowerPermille"] = int(point["approachPowerPermille"]) - 9
        point["approachFloorPowerPermille"] = max(180, int(point["approachFloorPowerPermille"]) - 8)
        point["holdEntryCentiC"] = int(point["holdEntryCentiC"]) + 8
        point["holdPowerPermille"] = max(0, int(point["holdPowerPermille"]) - 6)
        point["holdReheatPowerPermille"] = max(0, int(point["holdReheatPowerPermille"]) - 5)
    return point


def round_result(target_c: int, round_number: int) -> dict:
    metrics = accepted_metrics(target_c)
    if round_number == 1:
        return {
            "targetTempC": target_c,
            "stopReason": "full_speed_to_stable_timeout",
            "maxOvershootC": max(0.4, metrics["overshoot"] - 0.35),
            "holdPeakToPeakC": metrics["p2p"] + 1.15,
            "fullSpeedToStable": {"settleTimeMs": None},
            "analysis": {
                "approachCurveMeanAbsErrorC": metrics["approach_mae"] + 0.85,
                "approachCurveDeviationClass": "too_cold",
            },
        }
    if round_number == 2:
        return {
            "targetTempC": target_c,
            "stopReason": "completed",
            "maxOvershootC": metrics["overshoot"] + 0.45,
            "holdPeakToPeakC": metrics["p2p"] + 0.38,
            "fullSpeedToStable": {"settleTimeMs": int(metrics["settle_ms"] * 1.12)},
            "analysis": {
                "approachCurveMeanAbsErrorC": metrics["approach_mae"] + 0.28,
                "approachCurveDeviationClass": "near_gate",
            },
        }
    return {
        "targetTempC": target_c,
        "stopReason": "completed",
        "maxOvershootC": metrics["overshoot"],
        "holdPeakToPeakC": metrics["p2p"],
        "fullSpeedToStable": {"settleTimeMs": metrics["settle_ms"]},
        "analysis": {
            "approachCurveMeanAbsErrorC": metrics["approach_mae"],
            "approachCurveDeviationClass": "within_gate",
            "holdMedianOutputPermille": metrics["hold_median"],
            "holdP90OutputPermille": metrics["hold_p90"],
            "approachSource": {"powerMw": {"avg": int(metrics["approach_source_w"] * 1000)}},
            "holdSource": {"powerMw": {"avg": int(metrics["hold_source_w"] * 1000)}},
        },
    }


def build_tuning_steps(accepted_profile: dict, effective_points: dict[int, dict]) -> list[dict]:
    steps: list[dict] = []
    stage_index = 0
    for target_c in TARGETS_C:
        for round_number in (1, 2, 3):
            point = mutate_point(effective_points[target_c], round_number)
            candidate_profile = sprint.merge_point(accepted_profile, point)
            steps.append(
                {
                    "stageIndex": stage_index,
                    "targetTempC": target_c,
                    "candidateProfile": candidate_profile,
                    "result": round_result(target_c, round_number),
                    "samples": build_raw_samples(target_c, point, round_number),
                }
            )
            stage_index += 1
    return steps


def build_applied_result(target_c: int) -> dict:
    return round_result(target_c, 3)


def build_raw_samples(target_c: int, point: dict, round_number: int = 3) -> list[dict]:
    warmup_end, approach_end, total_end = target_timing(target_c)
    ambient_c = 24.0
    warmup_exit_c = max(36.0, target_c - (17.0 + target_c * 0.12))
    hold_output = max(5.0, 6.0 + target_c * 0.09)
    raw: list[dict] = []
    for tick in range(0, int(total_end) + 1):
        t = float(tick)
        if t <= warmup_end:
            phase = "warmup"
            temp = ambient_c + (warmup_exit_c - ambient_c) * (t / max(warmup_end, 1.0))
            command = 100.0
            output = 100.0
            request_v = 21.0
            source_v = 21.0
            source_a = max(1.2, min(4.95, 2.8 + target_c / 90.0 - 0.12 * math.sin(t / 2.8)))
        elif t <= approach_end:
            phase = "approach"
            x = t - warmup_end
            span = max(approach_end - warmup_end, 1.0)
            progress = x / span
            target_curve = warmup_exit_c + (target_c - warmup_exit_c) * progress
            if round_number == 1:
                curvature = max(0.1, 0.36 - target_c / 900.0)
                temp = target_curve - 1.10 - 0.35 * math.cos(x / 1.9) - curvature * max(0.6 - progress, 0) * 2.6
                command = 42.0 - target_c / 20.0
            elif round_number == 2:
                curvature = max(0.16, 0.46 - target_c / 760.0)
                temp = target_curve + 0.45 * math.sin(x / 1.9) - curvature * max(progress - 0.7, 0) * 2.0
                command = 44.0 - target_c / 19.0
            else:
                curvature = max(0.2, 0.55 - target_c / 600.0)
                temp = target_curve + 0.65 * math.sin(x / 2.0) - curvature * max(progress - 0.72, 0) * 3.0
                command = 46.0 - target_c / 18.0
            output = max(18.0, 25.0 - target_c / 40.0)
            request_v = min(20.0, 8.2 + target_c * 0.055)
            source_v = request_v + 0.05 * math.sin(x / 1.7)
            source_a = max(0.8, 1.55 + target_c / 300.0 + 0.12 * math.sin(x / 2.3))
        else:
            phase = "hold"
            x = t - approach_end
            if round_number == 1:
                temp = target_c - 1.0 + 0.70 * math.sin(x / 1.6) - 0.18 * math.cos(x / 2.1)
                command = max(0.0, hold_output - 2.8) if int(x) % 4 in (0, 1) else 0.0
            elif round_number == 2:
                temp = target_c + 0.18 + 0.55 * math.sin(x / 1.75) - 0.18 * math.cos(x / 2.3)
                command = max(0.0, hold_output - 1.0) if int(x) % 4 in (0, 1) else 0.0
            else:
                temp = target_c + 0.42 * math.sin(x / 1.8) - 0.16 * math.cos(x / 2.5)
                command = hold_output if int(x) % 4 in (0, 1) else 0.0
            output = command
            request_v = 5.4 + min(target_c / 48.0, 4.8)
            source_v = request_v + 0.05 * math.sin(x / 2.2)
            source_a = max(0.05, 0.38 + target_c / 950.0 + 0.16 * math.sin(x / 1.9))
        source_w = source_v * source_a
        raw.append(
            {
                "targetTempC": target_c,
                "elapsedMs": int(t * 1000),
                "phase": phase,
                "status": {
                    "currentTempC": round(temp, 3),
                    "heaterFilteredTempC": round(temp - 0.10, 3),
                    "heaterOutputPercent": round(command, 3),
                    "heaterPhysicalOutputPercent": round(output, 3),
                    "pdRequestMv": int(round(request_v * 1000)),
                },
                "sourceTelemetry": {
                    "voltageMv": int(round(source_v * 1000)),
                    "currentMa": int(round(source_a * 1000)),
                    "powerMw": int(round(source_w * 1000)),
                },
                "heaterParameters": dict(point),
            }
        )
    return raw


def build_summary(applied: list[dict], tuning_steps: list[dict], sample_count: int) -> dict:
    replay_summary_path = SOURCE_RUNS_DIR / "full-range-replay.run.json"
    return {
        "ok": True,
        "runId": "mock-final-multitarget-format-20260718",
        "target": {"deviceId": "mock-fp-accepted-final-report"},
        "source": {"id": "f293cc"},
        "parameters": {
            "sampleIntervalMs": 1000,
            "targetsC": TARGETS_C,
        },
        "files": {
            "summaryPath": str(replay_summary_path),
            "samplesPath": str(OUTPUT_DIR / "samples.ndjson"),
        },
        "candidateProfile": {"kind": "mock_final_format_review"},
        "profilePersistence": "mock_review_only",
        "tuningSteps": tuning_steps,
        "applied": applied,
        "validation": {
            "expectedTargetsC": TARGETS_C,
            "failures": [],
            "passed": True,
        },
        "sampleCount": sample_count,
        "complete": True,
        "error": None,
    }


def write_json(path: Path, payload: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_raw_samples(path: Path, grouped_samples: dict[int, list[dict]]) -> None:
    records: list[str] = []
    for target_c in TARGETS_C:
        for sample in grouped_samples[target_c]:
            records.append(json.dumps(sample, ensure_ascii=False))
    path.write_text("\n".join(records) + "\n", encoding="utf-8")


def main() -> None:
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    SOURCE_RUNS_DIR.mkdir(parents=True, exist_ok=True)

    accepted_profile = read_json(ACCEPTED_PROFILE_PATH)
    effective_points = load_effective_points()
    tuning_steps = build_tuning_steps(accepted_profile, effective_points)
    grouped_samples = {target_c: build_raw_samples(target_c, effective_points[target_c], 3) for target_c in TARGETS_C}
    applied = [build_applied_result(target_c) for target_c in TARGETS_C]
    summary = build_summary(applied, tuning_steps, sum(len(items) for items in grouped_samples.values()))
    entries = [
        sprint.target_run_entry(summary, accepted_profile, target_c, grouped_samples[target_c])
        for target_c in TARGETS_C
    ]
    source_run_paths: dict[int, Path] = {
        target_c: SOURCE_RUNS_DIR / f"{target_c}.run.json"
        for target_c in TARGETS_C
    }

    bundle = sprint.build_baseline_bundle_json(summary, OUTPUT_DIR, source_run_paths)
    bundle["bundleRole"] = "mock_final_multitarget_review"
    bundle["reportDeliveryNote"] = (
        "Browser-openable canonical HTML bundle. Mock final multi-target accepted report for implementation review only."
    )
    bundle["dryRun"] = True
    bundle["mockData"] = True

    write_json(OUTPUT_DIR / "run.bundle.json", bundle)
    write_json(OUTPUT_DIR / "thermal-profile.accepted.json", accepted_profile)
    write_raw_samples(OUTPUT_DIR / "samples.ndjson", grouped_samples)

    html_data = sprint.build_html_data(summary, accepted_profile, entries, "/dev/cu.usbmodem2111401")
    html_data["title"] = "Flux Purr 100W / pps5a 温控 accepted 基线（mock）"
    html_data["subtitle"] = (
        "用于对齐最终多目标 accepted 报告实现；目标集合为 60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C。"
        "全部轮次、参数与评价均为 mock 数据，不代表真实验收结果。"
    )
    html_data["deviceId"] = "mock-fp-accepted-final-report"
    html_data["sourceDeviceId"] = "f293cc"
    (OUTPUT_DIR / "index.html").write_text(sprint.render_baseline_html(html_data), encoding="utf-8")

    write_json(SOURCE_RUNS_DIR / "full-range-replay.run.json", {**summary, "mockData": True})
    for target_c in TARGETS_C:
        per_target = sprint.source_run_summary(summary, target_c)
        per_target["mockData"] = True
        write_json(SOURCE_RUNS_DIR / f"{target_c}.run.json", per_target)

    print(OUTPUT_DIR)


if __name__ == "__main__":
    main()
