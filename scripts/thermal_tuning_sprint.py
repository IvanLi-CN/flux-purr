#!/usr/bin/env python3

from __future__ import annotations

import argparse
import copy
import datetime as dt
import json
import math
import shlex
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parent.parent
DEFAULT_ANCHOR_TARGETS = [60, 140, 220]
DEFAULT_VALIDATION_TARGETS = [60, 140, 220]
DEFAULT_TUNE_TARGETS = [60, 140, 220]
PRELIMINARY_PROFILE = REPO_ROOT / "thermal-self-test-runs/preliminary-pd100w-pps5a-60-140-220-20260717/thermal-profile.accepted.json"
FALLBACK_PROFILE = REPO_ROOT / "thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json"
DEFAULT_BASELINE_DIR = REPO_ROOT / "thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd100w-pps5a/accepted-full-range-20hz"
THERMAL_CONTROL_PROFILE_MAX_POINTS = 10
SOURCE_KIND = "isolapurr"
SOURCE_MODE = "auto-follow"
PROFILE_MODE = "100w"
EXPECTED_BANK = "pps5a"
EXPECTED_SOURCE_CLASS = "pps5a"
EXPECTED_SOURCE_POWER_WATTS = 100
EXPECTED_SOURCE_PPS_LIMIT_MA = 5_000
DEFAULT_AUTHORIZED_PORT = "/dev/cu.usbmodem2111401"
DEFAULT_PER_TARGET_BUDGET_SECONDS = 1_200
DEFAULT_MAX_TUNING_ROUNDS = 3
DEFAULT_SCOUT_HOLD_SECONDS = 12
DEFAULT_CONFIRM_HOLD_SECONDS = 60
EVALUATION_MODE_TUNING_SCOUT = "tuning-scout"
EVALUATION_MODE_HOLD_CONFIRM = "hold-confirm"
SOURCE_RECOVERY_SETTLE_SECONDS = 2.0
SOURCE_RECOVERY_POLL_INTERVAL_SECONDS = 0.5
SOURCE_RECOVERY_POLL_ATTEMPTS = 6


def utc_now() -> dt.datetime:
    return dt.datetime.now(dt.timezone.utc)


def now_iso() -> str:
    return utc_now().isoformat().replace("+00:00", "Z")


def now_slug() -> str:
    return utc_now().strftime("%Y%m%d-%H%M%S")


def today_slug() -> str:
    return dt.datetime.now().strftime("%Y%m%d")


def log(message: str) -> None:
    print(message, flush=True)


def cooldown_threshold(target_temp_c: int) -> float:
    return 35.0 if int(target_temp_c) < 80 else float(int(target_temp_c) - 40)


def budget_elapsed_seconds(start_monotonic: float, now_monotonic: float | None = None) -> int:
    now_value = time.monotonic() if now_monotonic is None else float(now_monotonic)
    return max(0, int(now_value - start_monotonic))


def budget_remaining_seconds(
    start_monotonic: float,
    budget_seconds: int,
    now_monotonic: float | None = None,
) -> int:
    return max(0, int(budget_seconds) - budget_elapsed_seconds(start_monotonic, now_monotonic))


def budget_exhausted(
    start_monotonic: float,
    budget_seconds: int,
    now_monotonic: float | None = None,
) -> bool:
    return budget_remaining_seconds(start_monotonic, budget_seconds, now_monotonic) <= 0


def step_timeouts_for_budget(remaining_seconds: int, hold_seconds: int) -> tuple[int, int] | None:
    remaining = int(remaining_seconds)
    hold = int(hold_seconds)
    if remaining <= hold + 30:
        return None
    stage_timeout = min(max(hold + 30, 90), max(hold + 5, remaining - 15))
    cooldown_timeout = max(15, remaining - stage_timeout)
    if cooldown_timeout + stage_timeout > remaining:
        cooldown_timeout = max(1, remaining - stage_timeout)
    if cooldown_timeout <= 0 or stage_timeout <= hold:
        return None
    return cooldown_timeout, stage_timeout


def parse_targets(raw: str | None, default: list[int]) -> list[int]:
    if not raw:
        return list(default)
    targets: list[int] = []
    for part in raw.split(","):
        part = part.strip()
        if not part:
            continue
        targets.append(int(part))
    if not targets:
        raise RuntimeError("target list is empty")
    return targets


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def clamp_int(value: int, lower: int, upper: int) -> int:
    return max(lower, min(upper, value))


def value_at_path(payload: dict[str, Any], *path: str) -> Any:
    current: Any = payload
    for part in path:
        if not isinstance(current, dict):
            return None
        current = current.get(part)
    return current


def source_usb_c_sample_uptime_ms(payload: dict[str, Any]) -> int | None:
    uptime = value_at_path(payload, "diagnostics", "usb_c_actual", "sample_uptime_ms")
    if isinstance(uptime, (int, float)):
        return int(uptime)
    return None


def verify_isolapurr_power_show(
    payload: dict[str, Any],
    *,
    expect_usb_c_enabled: bool,
    previous_sample_uptime_ms: int | None = None,
) -> int | None:
    usb_c_enabled = value_at_path(payload, "diagnostics", "usb_c_power_enabled")
    if usb_c_enabled is not expect_usb_c_enabled:
        raise RuntimeError(
            f"isolapurr usb_c_power_enabled mismatch: expected {expect_usb_c_enabled}, got {usb_c_enabled}"
        )
    if not expect_usb_c_enabled:
        return source_usb_c_sample_uptime_ms(payload)

    tps_mode = value_at_path(payload, "config", "tps_mode")
    if tps_mode not in {"auto_follow", "autoFollow"}:
        raise RuntimeError(f"isolapurr tps_mode mismatch after recovery: expected auto_follow, got {tps_mode}")
    output_enabled = value_at_path(payload, "config", "runtime", "output_enabled")
    if output_enabled is not True:
        raise RuntimeError("isolapurr runtime output is not enabled after recovery")
    power_watts = value_at_path(payload, "config", "capability", "power_watts")
    if int(power_watts or 0) != EXPECTED_SOURCE_POWER_WATTS:
        raise RuntimeError(
            f"isolapurr capability mismatch after recovery: expected {EXPECTED_SOURCE_POWER_WATTS}W, got {power_watts}"
        )
    pd_enabled = value_at_path(payload, "config", "capability", "protocols", "pd")
    pps_enabled = value_at_path(payload, "config", "capability", "pd", "pps")
    if pd_enabled is not True or pps_enabled is not True:
        raise RuntimeError("isolapurr capability mismatch after recovery: PD/PPS is not enabled")
    pps_limit_ma = value_at_path(payload, "config", "capability", "current", "pps3_limit_ma")
    if int(pps_limit_ma or 0) < EXPECTED_SOURCE_PPS_LIMIT_MA:
        raise RuntimeError(
            f"isolapurr capability mismatch after recovery: PPS current limit {pps_limit_ma}mA is below {EXPECTED_SOURCE_PPS_LIMIT_MA}mA"
        )
    pd_pps_5a = value_at_path(payload, "config", "capability", "current", "pd_pps_5a")
    if pd_pps_5a is not True:
        raise RuntimeError("isolapurr capability mismatch after recovery: pd_pps_5a is not enabled")
    usb_status = value_at_path(payload, "diagnostics", "usb_c_actual", "status")
    if usb_status != "ok":
        raise RuntimeError(f"isolapurr usb_c_actual status is not ok after recovery: {usb_status}")
    uptime_ms = source_usb_c_sample_uptime_ms(payload)
    if uptime_ms is None or uptime_ms <= 0:
        raise RuntimeError("isolapurr usb_c_actual sample uptime is missing after recovery")
    if previous_sample_uptime_ms is not None and uptime_ms <= previous_sample_uptime_ms:
        raise RuntimeError(
            f"isolapurr usb_c_actual sample uptime did not advance after recovery: previous={previous_sample_uptime_ms} current={uptime_ms}"
        )
    return uptime_ms


def profile_points(profile: dict[str, Any]) -> list[dict[str, Any]]:
    points = profile.get("points") or []
    return [dict(point) for point in points if isinstance(point, dict)]


def point_map(profile: dict[str, Any]) -> dict[int, dict[str, Any]]:
    return {
        int(point["targetTempC"]): dict(point)
        for point in profile_points(profile)
        if "targetTempC" in point
    }


def explicit_point(profile: dict[str, Any], target_temp_c: int) -> dict[str, Any] | None:
    return point_map(profile).get(int(target_temp_c))


def pad_profile_points(points: list[dict[str, Any]]) -> list[Any]:
    values: list[Any] = [dict(point) for point in sorted(points, key=lambda item: int(item["targetTempC"]))]
    while len(values) < THERMAL_CONTROL_PROFILE_MAX_POINTS:
        values.append(None)
    return values


def sparse_profile(settings: dict[str, Any], points: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "settings": dict(settings),
        "points": pad_profile_points(points),
    }


def repo_display_path(path: Path) -> str:
    try:
        return str(path.relative_to(REPO_ROOT))
    except ValueError:
        return str(path)


def cli_arg_path(path: Path) -> str:
    if path.is_absolute():
        return repo_display_path(path)
    return str(path)


def merge_point(profile: dict[str, Any], point: dict[str, Any]) -> dict[str, Any]:
    target = int(point["targetTempC"])
    merged = point_map(profile)
    merged[target] = dict(point)
    return sparse_profile(dict(profile.get("settings") or {}), list(merged.values()))


def pick_profile_settings(*profiles: dict[str, Any]) -> dict[str, Any]:
    for profile in profiles:
        settings = profile.get("settings")
        if isinstance(settings, dict):
            return dict(settings)
    raise RuntimeError("no profile settings available")


def stage_for_target(summary: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    for stage in summary.get("applied") or []:
        if isinstance(stage, dict) and int(stage.get("targetTempC", -1)) == int(target_temp_c):
            return dict(stage)
    raise RuntimeError(f"missing applied stage for {target_temp_c}°C")


def validation_failures_for_target(summary: dict[str, Any], target_temp_c: int) -> list[dict[str, Any]]:
    failures = []
    validation = summary.get("validation")
    raw_failures = validation.get("failures") if isinstance(validation, dict) else []
    for failure in raw_failures or []:
        if isinstance(failure, dict) and int(failure.get("targetTempC", -1)) == int(target_temp_c):
            failures.append(dict(failure))
    return failures


def stage_metrics(stage: dict[str, Any]) -> dict[str, Any]:
    analysis = stage.get("analysis") if isinstance(stage.get("analysis"), dict) else {}
    stable = stage.get("fullSpeedToStable") if isinstance(stage.get("fullSpeedToStable"), dict) else {}
    return {
        "stopReason": stage.get("stopReason"),
        "maxOvershootC": stage.get("maxOvershootC"),
        "holdPeakToPeakC": stage.get("holdPeakToPeakC"),
        "approachCurveMeanAbsErrorC": analysis.get("approachCurveMeanAbsErrorC"),
        "approachReferenceDurationDeltaMs": analysis.get("approachReferenceDurationDeltaMs"),
        "approachReferencePeakDeltaC": analysis.get("approachReferencePeakDeltaC"),
        "approachReferenceClass": analysis.get("approachReferenceClass"),
        "approachPreferredMs": analysis.get("approachCurvePreferredMs"),
        "approachLimitMs": analysis.get("approachCurveLimitMs"),
        "approachClass": analysis.get("approachCurveDeviationClass"),
        "settleTimeMs": stable.get("settleTimeMs"),
        "fullSpeedLimitMs": stable.get("limitMs"),
        "failureReason": stable.get("failureReason"),
        "holdMedianOutputPermille": analysis.get("holdMedianOutputPermille"),
        "holdP90OutputPermille": analysis.get("holdP90OutputPermille"),
    }


def classify_stability_evidence(
    samples: list[dict[str, Any]],
    *,
    target_temp_c: int,
    warmup_exited_at_ms: int | float | None,
    limit_ms: int | float | None,
) -> dict[str, Any]:
    """Classify the observed stability failure without conflating opposite corrections."""
    if not isinstance(warmup_exited_at_ms, (int, float)) or not isinstance(limit_ms, (int, float)):
        return {"failureClass": "insufficient_evidence"}

    lower = float(target_temp_c) - 1.5
    upper = float(target_temp_c) + 1.5
    exit_ms = float(warmup_exited_at_ms)
    deadline_ms = exit_ms + float(limit_ms)
    observed: list[tuple[float, float]] = []
    for sample in samples:
        elapsed = sample.get("t")
        temp = sample.get("temp")
        if not isinstance(elapsed, (int, float)) or not isinstance(temp, (int, float)):
            continue
        elapsed_ms = float(elapsed) * 1000.0
        if elapsed_ms >= exit_ms:
            observed.append((elapsed_ms, float(temp)))
    if not observed:
        return {"failureClass": "insufficient_evidence"}

    deadline_sample = min(observed, key=lambda item: abs(item[0] - deadline_ms))
    first_band_index = next(
        (index for index, (elapsed_ms, temp) in enumerate(observed) if elapsed_ms <= deadline_ms and lower <= temp <= upper),
        None,
    )
    evidence: dict[str, Any] = {
        "failureClass": "within_gate",
        "lowerBandC": lower,
        "upperBandC": upper,
        "deadlineAtMs": int(round(deadline_ms)),
        "deadlineTempC": deadline_sample[1],
        "firstBandAtMs": None,
        "bandExitTempC": None,
        "temperatureGapC": 0.0,
    }
    if first_band_index is None:
        deadline_temp = deadline_sample[1]
        if deadline_temp < lower:
            evidence["failureClass"] = "missed_lower_band_before_limit"
            evidence["temperatureGapC"] = lower - deadline_temp
        elif deadline_temp > upper:
            evidence["failureClass"] = "missed_upper_band_before_limit"
            evidence["temperatureGapC"] = deadline_temp - upper
        else:
            evidence["failureClass"] = "band_entry_not_observed"
        return evidence

    evidence["firstBandAtMs"] = int(round(observed[first_band_index][0]))
    for _, temp in observed[first_band_index + 1 :]:
        if temp > upper:
            evidence["failureClass"] = "stable_window_broke_high"
            evidence["bandExitTempC"] = temp
            evidence["temperatureGapC"] = temp - upper
            break
        if temp < lower:
            evidence["failureClass"] = "stable_window_broke_low"
            evidence["bandExitTempC"] = temp
            evidence["temperatureGapC"] = lower - temp
            break
    return evidence


def stability_evidence_for_stage(
    stage: dict[str, Any],
    samples: list[dict[str, Any]],
    target_temp_c: int,
) -> dict[str, Any]:
    stable = stage.get("fullSpeedToStable") if isinstance(stage.get("fullSpeedToStable"), dict) else {}
    evidence = classify_stability_evidence(
        samples,
        target_temp_c=target_temp_c,
        warmup_exited_at_ms=stable.get("warmupExitedAtMs"),
        limit_ms=stable.get("limitMs"),
    )
    settle_ms = stable.get("settleTimeMs")
    limit_ms = stable.get("limitMs")
    required_margin_ms = 500 if int(target_temp_c) > 150 else 1000
    if stage.get("stopReason") == "completed" and isinstance(settle_ms, (int, float)):
        evidence["failureClass"] = "within_gate"
        if isinstance(limit_ms, (int, float)) and float(limit_ms) - float(settle_ms) < required_margin_ms:
            evidence["failureClass"] = "within_gate_low_margin"
            evidence["timeMarginMs"] = float(limit_ms) - float(settle_ms)
            evidence["requiredTimeMarginMs"] = required_margin_ms
    return evidence


def predict_next_point(point: dict[str, Any], evidence: dict[str, Any]) -> dict[str, Any]:
    """Apply one bounded, evidence-specific correction to a target-local point."""
    predicted = dict(point)
    failure_class = str(evidence.get("failureClass") or "")
    gap_c = float(evidence.get("temperatureGapC") or 0.0)
    correction_centi_c = clamp_int(int(math.ceil((gap_c + 0.4) * 100.0)), 30, 120)

    if failure_class in {"stable_window_broke_high", "missed_upper_band_before_limit"}:
        predicted["brakeDistanceCentiC"] = clamp_int(
            int(predicted["brakeDistanceCentiC"]) + correction_centi_c,
            0,
            5000,
        )
        predicted["approachFloorPowerPermille"] = clamp_int(
            int(predicted["approachFloorPowerPermille"]) - max(20, correction_centi_c // 2),
            0,
            1000,
        )
    elif failure_class in {"missed_lower_band_before_limit", "stable_window_broke_low"}:
        if int(predicted["approachPowerPermille"]) >= 1000 and int(predicted["approachFloorPowerPermille"]) >= 1000:
            predicted["brakeDistanceCentiC"] = clamp_int(
                int(predicted["brakeDistanceCentiC"]) - correction_centi_c,
                0,
                5000,
            )
        else:
            predicted["approachFloorPowerPermille"] = clamp_int(
                int(predicted["approachFloorPowerPermille"]) + max(20, correction_centi_c // 2),
                0,
                1000,
            )
    elif failure_class == "within_gate_low_margin":
        if int(predicted["approachPowerPermille"]) >= 1000 and int(predicted["approachFloorPowerPermille"]) >= 1000:
            predicted["brakeDistanceCentiC"] = clamp_int(
                int(predicted["brakeDistanceCentiC"]) - 40,
                0,
                5000,
            )
        else:
            predicted["approachFloorPowerPermille"] = clamp_int(
                int(predicted["approachFloorPowerPermille"]) + 20,
                0,
                1000,
            )
    return predicted


def ensure_expected_source(summary: dict[str, Any]) -> None:
    source = summary.get("source")
    if not isinstance(source, dict):
        raise RuntimeError("thermal summary missing source payload")
    selected_mode = source.get("selectedMode")
    resolved_bank = source.get("resolvedBank")
    detected_source_class = source.get("detectedSourceClass")
    if selected_mode != PROFILE_MODE:
        raise RuntimeError(f"selectedMode mismatch: expected {PROFILE_MODE}, got {selected_mode}")
    if resolved_bank != EXPECTED_BANK:
        raise RuntimeError(f"resolvedBank mismatch: expected {EXPECTED_BANK}, got {resolved_bank}")
    if detected_source_class != EXPECTED_SOURCE_CLASS:
        raise RuntimeError(
            f"detectedSourceClass mismatch: expected {EXPECTED_SOURCE_CLASS}, got {detected_source_class}"
        )


def ensure_batch_source(batch_summary: dict[str, Any]) -> None:
    for run in batch_summary.get("runs") or []:
        if isinstance(run, dict):
            ensure_expected_source(run)


def choose_first_sample_points(samples_path: Path) -> dict[int, dict[str, Any]]:
    points: dict[int, dict[str, Any]] = {}
    with samples_path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            sample = json.loads(line)
            target_temp_c = int(sample["targetTempC"])
            if target_temp_c not in points:
                heater_parameters = sample.get("heaterParameters")
                if isinstance(heater_parameters, dict):
                    points[target_temp_c] = dict(heater_parameters)
    return points


def dry_run_materialized_points(
    runner: "FluxPurrRunner",
    seed_profile: dict[str, Any],
    targets_c: list[int],
    output_dir: Path,
    tag: str,
) -> dict[int, dict[str, Any]]:
    seed_path = output_dir / f"{tag}.seed.json"
    write_json(seed_path, seed_profile)
    run = runner.self_test(
        seed_profile_file=seed_path,
        targets_c=targets_c,
        hold_seconds=12,
        output_dir=output_dir / f"{tag}-materialize",
        dry_run_override=True,
    )
    return choose_first_sample_points(run.samples_path)


def build_initial_sparse_seed(
    runner: "FluxPurrRunner",
    preliminary_profile: dict[str, Any],
    fallback_profile: dict[str, Any],
    anchors_c: list[int],
    output_dir: Path,
) -> dict[str, Any]:
    settings = pick_profile_settings(preliminary_profile, fallback_profile)
    preliminary_points = point_map(preliminary_profile)
    fallback_points = point_map(fallback_profile)
    base_targets = [60, 140, 220]
    base_points: list[dict[str, Any]] = []
    for target_temp_c in base_targets:
        point = preliminary_points.get(target_temp_c) or fallback_points.get(target_temp_c)
        if point is None:
            raise RuntimeError(f"missing required base point {target_temp_c}°C for sparse seed")
        base_points.append(dict(point))

    point_240 = preliminary_points.get(240)
    if point_240 is None:
        materialized_240 = dry_run_materialized_points(
            runner,
            sparse_profile(settings, [fallback_points[220], fallback_points[250]]),
            [240],
            output_dir,
            "materialize-240",
        )
        point_240 = materialized_240.get(240)
    if point_240 is None:
        raise RuntimeError("unable to derive initial 240°C anchor")

    scaffold = sparse_profile(settings, [*base_points, dict(point_240)])
    derived = dry_run_materialized_points(runner, scaffold, [100, 180], output_dir, "materialize-100-180")
    points = [
        dict(base_points[0]),
        dict(derived[100]),
        dict(base_points[1]),
        dict(derived[180]),
        dict(base_points[2]),
        dict(point_240),
    ]
    by_target = {int(point["targetTempC"]): point for point in points}
    for target_temp_c in anchors_c:
        if target_temp_c not in by_target:
            raise RuntimeError(f"initial sparse seed missing anchor {target_temp_c}°C")
    return sparse_profile(settings, [by_target[target] for target in anchors_c])


def normalize_sparse_profile(
    runner: "FluxPurrRunner",
    profile: dict[str, Any],
    anchors_c: list[int],
    output_dir: Path,
    tag: str,
) -> dict[str, Any]:
    settings = pick_profile_settings(profile)
    explicit = point_map(profile)
    missing = [target for target in anchors_c if target not in explicit]
    materialized: dict[int, dict[str, Any]] = {}
    if missing:
        materialized = dry_run_materialized_points(runner, profile, missing, output_dir, tag)
    points: list[dict[str, Any]] = []
    for target_temp_c in anchors_c:
        point = explicit.get(target_temp_c) or materialized.get(target_temp_c)
        if point is None:
            raise RuntimeError(f"sparse normalization could not materialize {target_temp_c}°C")
        points.append(dict(point))
    return sparse_profile(settings, points)


def mutate_more_heat(point: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    mutated = dict(point)
    scale = 2 if target_temp_c >= 220 else 1 if target_temp_c <= 100 else 1.5
    # `holdEntryCentiC` is an error band below target. Lowering it delays Hold entry
    # until the controller is closer to target, which is the "more heat before Hold"
    # direction. Raising it would enter Hold earlier and worsen low-temperature
    # full-speed-to-stable failures.
    mutated["holdEntryCentiC"] = clamp_int(int(mutated["holdEntryCentiC"]) - int(25 * scale), 0, 5000)
    mutated["approachFloorPowerPermille"] = clamp_int(
        int(mutated["approachFloorPowerPermille"]) + int(30 * scale),
        0,
        1000,
    )
    mutated["holdPowerPermille"] = clamp_int(int(mutated["holdPowerPermille"]) + int(25 * scale), 0, 1000)
    mutated["holdReheatPowerPermille"] = clamp_int(
        int(mutated["holdReheatPowerPermille"]) + int(40 * scale),
        0,
        1000,
    )
    if target_temp_c >= 180:
        mutated["approachPowerPermille"] = clamp_int(int(mutated["approachPowerPermille"]) + 20, 0, 1000)
    return mutated


def mutate_more_brake(point: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    mutated = dict(point)
    brake = int(mutated["brakeDistanceCentiC"])
    mutated["brakeDistanceCentiC"] = clamp_int(brake + max(40, int(round(brake * 0.12))), 0, 5000)
    mutated["approachDampingExponentPermille"] = clamp_int(
        int(mutated["approachDampingExponentPermille"]) + (180 if target_temp_c <= 140 else 120),
        0,
        4000,
    )
    mutated["approachLeadTicks"] = clamp_int(int(mutated["approachLeadTicks"]) + 1, 0, 255)
    mutated["holdEntryCentiC"] = clamp_int(int(mutated["holdEntryCentiC"]) - (15 if target_temp_c <= 140 else 8), 0, 5000)
    mutated["holdReheatPowerPermille"] = clamp_int(int(mutated["holdReheatPowerPermille"]) - 30, 0, 1000)
    return mutated


def mutate_hold_ripple(point: dict[str, Any], metrics: dict[str, Any]) -> dict[str, Any]:
    mutated = dict(point)
    median = metrics.get("holdMedianOutputPermille")
    p90 = metrics.get("holdP90OutputPermille")
    if isinstance(median, (int, float)):
        hold_power = clamp_int(int(round(float(median))), 0, 1000)
        mutated["holdPowerPermille"] = max(hold_power, clamp_int(int(mutated["holdPowerPermille"]) - 20, 0, 1000))
    if isinstance(p90, (int, float)):
        mutated["holdReheatPowerPermille"] = clamp_int(
            max(int(round(float(p90))) + 20, int(mutated["holdPowerPermille"]) + 20),
            0,
            1000,
        )
    mutated["holdBlendTicks"] = clamp_int(int(mutated["holdBlendTicks"]) + 1, 1, 255)
    hold_on = int(mutated["holdOnCentiC"])
    hold_off = int(mutated["holdOffCentiC"])
    mutated["holdOffCentiC"] = clamp_int(max(hold_on + 20, hold_off - 10), 0, 5000)
    return mutated


@dataclass
class CandidateVariant:
    name: str
    profile: dict[str, Any]
    path: Path | None = None


def build_candidate_variants(
    current_profile: dict[str, Any],
    retuned_profile: dict[str, Any],
    target_temp_c: int,
    scout_stage: dict[str, Any],
    scout_samples: list[dict[str, Any]] | None = None,
) -> list[CandidateVariant]:
    current_point = explicit_point(current_profile, target_temp_c)
    retuned_point = explicit_point(retuned_profile, target_temp_c)
    if current_point is None or retuned_point is None:
        raise RuntimeError(f"missing {target_temp_c}°C anchor while building variants")
    metrics = stage_metrics(scout_stage)
    evidence = stability_evidence_for_stage(scout_stage, scout_samples or [], target_temp_c)
    predicted_point = predict_next_point(current_point, evidence)

    variants = [CandidateVariant("current", current_profile)]
    if predicted_point != current_point:
        variants.append(CandidateVariant(str(evidence["failureClass"]), merge_point(current_profile, predicted_point)))

    hold_p2p = metrics.get("holdPeakToPeakC")
    if isinstance(hold_p2p, (int, float)) and float(hold_p2p) > 3.0:
        ripple_point = mutate_hold_ripple(current_point, metrics)
        if ripple_point != current_point:
            variants.append(CandidateVariant("hold_ripple", merge_point(current_profile, ripple_point)))

    if len(variants) == 1 and retuned_point != current_point:
        variants.append(CandidateVariant("retuned_fallback", retuned_profile))
    return variants[:4]


def run_is_disqualified(summary: dict[str, Any], target_temp_c: int) -> bool:
    stage = stage_for_target(summary, target_temp_c)
    stop_reason = str(stage.get("stopReason") or "")
    if stop_reason in {
        "heater_disarmed",
        "target_mismatch",
        "profile_target_mismatch",
        "runtime_reset",
        "sample_rate_below_minimum",
        "source_telemetry_stale",
        "source_fault",
    }:
        return True
    terminal_reason = stage.get("terminalRuntimeDropReason")
    if terminal_reason not in (None, ""):
        return True
    for failure in validation_failures_for_target(summary, target_temp_c):
        reason = str(failure.get("reason") or failure.get("failureReason") or failure.get("stopReason") or "")
        if "source" in reason or "sample_rate" in reason or "target_mismatch" in reason or "heater_disarmed" in reason:
            return True
    return False


def candidate_score(summary: dict[str, Any], target_temp_c: int) -> tuple[Any, ...]:
    stage = stage_for_target(summary, target_temp_c)
    metrics = stage_metrics(stage)
    evidence = stability_evidence_for_stage(stage, samples_for_target(summary, target_temp_c), target_temp_c)
    hold_p2p = metrics.get("holdPeakToPeakC")
    settle_time_ms = metrics.get("settleTimeMs")
    full_speed_limit_ms = metrics.get("fullSpeedLimitMs")
    settle_penalty = (
        float(settle_time_ms)
        if isinstance(settle_time_ms, (int, float))
        else math.inf
    )
    full_speed_margin_penalty = (
        max(
            0.0,
            float(settle_time_ms)
            - (float(full_speed_limit_ms) - (500.0 if int(target_temp_c) > 150 else 1000.0)),
        )
        if isinstance(settle_time_ms, (int, float)) and isinstance(full_speed_limit_ms, (int, float))
        else math.inf
    )
    hold_median = metrics.get("holdMedianOutputPermille")
    hold_p90 = metrics.get("holdP90OutputPermille")
    hold_balance_penalty = (
        abs(float(hold_p90) - float(hold_median))
        if isinstance(hold_median, (int, float)) and isinstance(hold_p90, (int, float))
        else math.inf
    )
    return (
        1 if run_is_disqualified(summary, target_temp_c) else 0,
        0 if metrics.get("stopReason") == "completed" else 1,
        1 if evidence.get("failureClass") == "within_gate_low_margin" else 0,
        full_speed_margin_penalty,
        settle_penalty,
        float(metrics.get("maxOvershootC") if isinstance(metrics.get("maxOvershootC"), (int, float)) else math.inf),
        float(hold_p2p if isinstance(hold_p2p, (int, float)) else math.inf),
        hold_balance_penalty,
        float(
            metrics.get("approachCurveMeanAbsErrorC")
            if isinstance(metrics.get("approachCurveMeanAbsErrorC"), (int, float))
            else math.inf
        ),
    )


def choose_best_batch_run(batch_summary: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    runs = [dict(run) for run in batch_summary.get("runs") or [] if isinstance(run, dict)]
    if not runs:
        raise RuntimeError(f"thermal batch summary has no runs: {batch_summary.get('error')}")
    ensure_batch_source(batch_summary)
    ranked = sorted(
        (
            {
                "summary": run,
                "score": candidate_score(run, target_temp_c),
                "candidateProfileFile": Path(run["parameters"]["candidateProfileFile"]),
            }
            for run in runs
        ),
        key=lambda item: item["score"],
    )
    best = ranked[0]
    if best["score"][0] == 1:
        raise RuntimeError(
            f"all batch candidates for {target_temp_c}°C are disqualified by source/runtime/sample-rate faults"
        )
    return best


def grouped_samples(samples_path: Path) -> dict[int, list[dict[str, Any]]]:
    groups: dict[int, list[dict[str, Any]]] = {}
    with samples_path.open("r", encoding="utf-8") as handle:
        for line in handle:
            if not line.strip():
                continue
            sample = json.loads(line)
            target_temp_c = int(sample["targetTempC"])
            groups.setdefault(target_temp_c, []).append(sample)
    return groups


def normalized_sample(sample: dict[str, Any]) -> dict[str, Any]:
    status = sample.get("status") if isinstance(sample.get("status"), dict) else {}
    heater = sample.get("heaterTelemetry") if isinstance(sample.get("heaterTelemetry"), dict) else {}
    source_telemetry = sample.get("sourceTelemetry") if isinstance(sample.get("sourceTelemetry"), dict) else {}
    request_mv = (
        status.get("pdRequestMv")
        or heater.get("ppsRequestMv")
        or status.get("voltageMv")
        or heater.get("hotplateVoltageMv")
        or 0
    )
    return {
        "t": round(float(sample.get("elapsedMs", 0)) / 1000.0, 3),
        "temp": status.get("currentTempC", heater.get("currentTempC")),
        "filtered": status.get("heaterFilteredTempC", heater.get("heaterFilteredTempC")),
        "command": status.get("heaterOutputPercent", heater.get("heaterOutputPercent")),
        "output": status.get("heaterPhysicalOutputPercent", heater.get("heaterPhysicalOutputPercent")),
        "requestV": round(float(request_mv) / 1000.0, 3),
        "phase": sample.get("phase"),
        "sourceVoltageV": round(float(source_telemetry["voltageMv"]) / 1000.0, 3)
        if isinstance(source_telemetry.get("voltageMv"), (int, float))
        else None,
        "sourceCurrentA": round(float(source_telemetry["currentMa"]) / 1000.0, 3)
        if isinstance(source_telemetry.get("currentMa"), (int, float))
        else None,
        "sourcePowerW": round(float(source_telemetry["powerMw"]) / 1000.0, 3)
        if isinstance(source_telemetry.get("powerMw"), (int, float))
        else None,
        "parameters": sample.get("heaterParameters"),
    }


POINT_FIELDS = [
    "targetTempC",
    "brakeDistanceCentiC",
    "warmupPowerPermille",
    "approachPowerPermille",
    "approachFloorPowerPermille",
    "approachDampingExponentPermille",
    "approachTailWindowCentiC",
    "holdPowerPermille",
    "holdReheatPowerPermille",
    "holdEntryCentiC",
    "holdExitCentiC",
    "holdOnCentiC",
    "holdOffCentiC",
    "overshootCutoffCentiC",
    "holdKpPermillePerC",
    "holdKiPermillePerCTick",
    "holdBlendTicks",
    "approachLeadTicks",
    "holdLeadTicks",
]


def sanitize_point(point: dict[str, Any] | None, target_temp_c: int | None = None) -> dict[str, Any] | None:
    if not isinstance(point, dict):
        return None
    sanitized = {key: point.get(key) for key in POINT_FIELDS if key in point}
    if target_temp_c is not None:
        sanitized["targetTempC"] = int(target_temp_c)
    if not sanitized:
        return None
    return sanitized


def effective_point_from_samples(samples: list[dict[str, Any]], target_temp_c: int) -> dict[str, Any] | None:
    for sample in samples:
        if not isinstance(sample, dict):
            continue
        params = sample.get("heaterParameters")
        sanitized = sanitize_point(params, target_temp_c)
        if sanitized is not None:
            return sanitized
    return None


def normalize_round_step_samples(samples: Any) -> list[dict[str, Any]]:
    normalized: list[dict[str, Any]] = []
    if not isinstance(samples, list):
        return normalized
    for sample in samples:
        if not isinstance(sample, dict):
            continue
        if "elapsedMs" in sample or "status" in sample or "heaterTelemetry" in sample:
            normalized.append(normalized_sample(sample))
            continue
        if "t" in sample and ("temp" in sample or "filtered" in sample):
            normalized.append(dict(sample))
    return normalized


def summary_run_path(summary: dict[str, Any]) -> Path:
    files = summary.get("files") if isinstance(summary.get("files"), dict) else {}
    summary_path = files.get("summaryPath")
    if not isinstance(summary_path, str):
        raise RuntimeError("summary missing files.summaryPath")
    return Path(summary_path)


def samples_for_target(summary: dict[str, Any], target_temp_c: int) -> list[dict[str, Any]]:
    files = summary.get("files") if isinstance(summary.get("files"), dict) else {}
    samples_path = files.get("samplesPath")
    if not isinstance(samples_path, str):
        return []
    samples_file = Path(samples_path)
    if not samples_file.exists():
        return []
    grouped = grouped_samples(samples_file)
    return [normalized_sample(sample) for sample in grouped.get(int(target_temp_c), [])]


def tuning_rounds_for_target(summary: dict[str, Any], target_temp_c: int) -> list[dict[str, Any]]:
    rounds: list[dict[str, Any]] = []
    for index, step in enumerate(summary.get("tuningSteps") or [], start=1):
        if not isinstance(step, dict):
            continue
        if int(step.get("targetTempC", -1)) != int(target_temp_c):
            continue
        result = step.get("result") if isinstance(step.get("result"), dict) else {}
        analysis = result.get("analysis") if isinstance(result.get("analysis"), dict) else {}
        stable = result.get("fullSpeedToStable") if isinstance(result.get("fullSpeedToStable"), dict) else {}
        candidate_profile = step.get("candidateProfile") if isinstance(step.get("candidateProfile"), dict) else {}
        point = sanitize_point(explicit_point(candidate_profile, target_temp_c), target_temp_c)
        samples = normalize_round_step_samples(step.get("samples"))
        rounds.append(
            {
                "round": len(rounds) + 1,
                "stageIndex": step.get("stageIndex", index - 1),
                "label": f"tuning step {index}",
                "attemptType": "tuning_step",
                "tuningRound": index,
                "candidateName": f"step-{index}",
                "selected": False,
                "evidenceValid": True,
                "point": point,
                "samples": samples,
                "result": {
                    "stopReason": result.get("stopReason"),
                    "maxOvershootC": result.get("maxOvershootC"),
                    "holdPeakToPeakC": result.get("holdPeakToPeakC"),
                    "settleTimeMs": stable.get("settleTimeMs"),
                    "fullSpeedLimitMs": stable.get("limitMs"),
                    "stabilityEvidence": stability_evidence_for_stage(result, samples, target_temp_c),
                    "approachCurveMeanAbsErrorC": analysis.get("approachCurveMeanAbsErrorC"),
                    "approachCurveDeviationClass": analysis.get("approachCurveDeviationClass"),
                },
            }
        )
    if rounds:
        rounds[-1]["selected"] = True
    return rounds


def target_run_entry(
    summary: dict[str, Any],
    accepted_profile: dict[str, Any],
    target_temp_c: int,
    samples: list[dict[str, Any]],
) -> dict[str, Any]:
    stage = stage_for_target(summary, target_temp_c)
    failures = validation_failures_for_target(summary, target_temp_c)
    persisted = str(summary.get("profilePersistence") or "")
    truth_point = sanitize_point(explicit_point(accepted_profile, target_temp_c), target_temp_c)
    effective_point = truth_point or effective_point_from_samples(samples, target_temp_c)
    return {
        "runId": summary.get("runId"),
        "target": target_temp_c,
        "ok": not failures and stage.get("stopReason") == "completed",
        "saved": persisted == "saved_tuned_candidate",
        "evidence": "accepted_20hz_hil",
        "point": effective_point,
        "truthPoint": truth_point,
        "pointSource": "accepted_profile" if truth_point is not None else "sample_parameters",
        "rounds": tuning_rounds_for_target(summary, target_temp_c),
        "result": stage,
        "failures": failures,
        "samples": [normalized_sample(sample) for sample in samples],
    }


def build_history(entries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    history = []
    for entry in entries:
        stable = entry["result"].get("fullSpeedToStable") if isinstance(entry["result"].get("fullSpeedToStable"), dict) else {}
        settle_time_ms = stable.get("settleTimeMs")
        history.append(
            {
                "runId": entry["runId"],
                "target": entry["target"],
                "ok": entry["ok"],
                "overshoot": entry["result"].get("maxOvershootC"),
                "p2p": entry["result"].get("holdPeakToPeakC"),
                "settle": round(float(settle_time_ms) / 1000.0, 3)
                if isinstance(settle_time_ms, (int, float))
                else None,
            }
        )
    return history


def source_run_summary(summary: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    clone = copy.deepcopy(summary)
    clone["applied"] = [stage_for_target(summary, target_temp_c)]
    clone["tuningSteps"] = [
        step
        for step in clone.get("tuningSteps") or []
        if isinstance(step, dict) and int(step.get("targetTempC", -1)) == int(target_temp_c)
    ]
    clone["parameters"] = dict(clone.get("parameters") or {})
    clone["parameters"]["targetsC"] = [target_temp_c]
    failures = validation_failures_for_target(summary, target_temp_c)
    clone["validation"] = {
        "expectedTargetsC": [target_temp_c],
        "failures": failures,
        "passed": not failures,
    }
    return clone


def build_baseline_bundle_json(
    summary: dict[str, Any],
    bundle_dir: Path,
    source_run_paths: dict[int, Path],
) -> dict[str, Any]:
    bundle = copy.deepcopy(summary)
    bundle["kind"] = "thermal_self_test_baseline_bundle"
    bundle["canonicalReportFormat"] = "html_bundle"
    bundle["reportDeliveryNote"] = "Browser-openable canonical HTML bundle."
    bundle["bundleRole"] = "accepted_real_hil_baseline"
    bundle["baselineHardware"] = {
        "heaterPlate": {"widthMm": 56, "heightMm": 56},
        "heaterResistanceOhms": 3.2,
        "source": {
            "kind": "usb_pd_pps",
            "powerEnvelopeW": {"min": 95, "max": 100},
            "ppsVoltageRangeV": {"min": 5, "max": 21},
            "currentLimitA": 5.0,
            "capabilityClass": EXPECTED_SOURCE_CLASS,
            "displayClass": "5A (100W)",
        },
    }
    bundle["baselineCadence"] = {
        "controlLoopHz": 20,
        "rtdConversionsPerCycle": 64,
        "hostSampleIntervalMs": summary.get("parameters", {}).get("sampleIntervalMs"),
    }
    bundle["baselineUse"] = "accepted_reference_for_future_thermal_control_iterations_on_same_hardware_class"
    bundle["selectedMode"] = PROFILE_MODE
    bundle["resolvedBank"] = EXPECTED_BANK
    bundle["detectedSourceClass"] = EXPECTED_SOURCE_CLASS
    bundle["sourceRuns"] = {
        str(target): repo_display_path(path)
        for target, path in sorted(source_run_paths.items())
    }
    bundle["originSourceRuns"] = {
        str(target): repo_display_path(Path(summary["files"]["summaryPath"]))
        for target in sorted(source_run_paths)
    }
    bundle["files"] = {
        "bundleDir": repo_display_path(bundle_dir),
        "indexHtml": repo_display_path(bundle_dir / "index.html"),
        "bundleJson": repo_display_path(bundle_dir / "run.bundle.json"),
        "samplesPath": repo_display_path(bundle_dir / "samples.ndjson"),
        "acceptedProfilePath": repo_display_path(bundle_dir / "thermal-profile.accepted.json"),
    }
    return bundle


def build_html_data(
    summary: dict[str, Any],
    accepted_profile: dict[str, Any],
    entries: list[dict[str, Any]],
    port_path: str,
) -> dict[str, Any]:
    source = summary.get("source") if isinstance(summary.get("source"), dict) else {}
    return {
        "generatedAt": now_iso(),
        "title": "Flux Purr 100W / pps5a 温控 accepted 基线",
        "subtitle": "accepted full-range 20Hz baseline for 60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C. 中间温区来自 six-anchor 稀疏真相源的现有 Rust 插值，不额外维护第二套公式。",
        "selectedMode": PROFILE_MODE,
        "resolvedBank": EXPECTED_BANK,
        "detectedSourceClass": EXPECTED_SOURCE_CLASS,
        "sourcePreset": "21V / 5.0A",
        "provider": "IsolaPurr",
        "sourceDeviceId": source.get("id"),
        "deviceId": (summary.get("target") or {}).get("deviceId"),
        "port": port_path,
        "hostSampleIntervalMs": (summary.get("parameters") or {}).get("sampleIntervalMs"),
        "acceptedProfile": accepted_profile,
        "runs": entries,
        "history": build_history(entries),
    }


def render_baseline_html(data: dict[str, Any]) -> str:
    data_json = json.dumps(data, ensure_ascii=False, separators=(",", ":"))
    return f"""<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width,initial-scale=1">
  <title>Flux Purr 温控报告</title>
  <style>
    :root{{--bg:#f4f5f6;--paper:#fff;--ink:#182026;--muted:#66717a;--line:#d9dee2;--grid:#e8ebed;--green:#18794e;--green-bg:#e7f5ed;--amber:#9a6700;--amber-bg:#fff4ce;--red:#c23b32;--blue:#1261a0;--cyan:#157a82;--radius:6px}}
    *{{box-sizing:border-box}} body{{margin:0;background:var(--bg);color:var(--ink);font:14px/1.45 ui-sans-serif,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}}
    .shell{{max-width:1460px;margin:auto;padding:24px}}
    .topbar{{display:flex;justify-content:space-between;gap:24px;align-items:flex-start;margin-bottom:18px}}
    .eyebrow{{font:600 12px/1.2 ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted);text-transform:uppercase}}
    h1{{font-size:25px;line-height:1.2;margin:6px 0 7px}}
    .subtitle{{color:var(--muted);max-width:78ch}}
    .meta{{display:flex;flex-wrap:wrap;gap:6px 16px;margin-top:10px;color:var(--ink);font:600 12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}}
    .meta span{{white-space:nowrap}} .meta b{{color:var(--blue)}}
    .stamp{{text-align:right;color:var(--muted);font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;white-space:nowrap}}
    .summary{{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-bottom:18px}}
    .status{{background:var(--paper);border:1px solid var(--line);border-left:4px solid var(--amber);border-radius:var(--radius);padding:14px 16px}}
    .status.pass{{border-left-color:var(--green)}} .status-head{{display:flex;align-items:center;justify-content:space-between;gap:12px}}
    .temp-label{{font-size:20px;font-weight:700}}
    .badge{{font-size:12px;font-weight:700;border:1px solid currentColor;border-radius:999px;padding:2px 8px;color:var(--amber);background:var(--amber-bg)}}
    .pass .badge{{color:var(--green);background:var(--green-bg)}} .status-metric{{font-size:13px;color:var(--muted);margin-top:9px}}
    .status-metric strong{{color:var(--ink)}}
    .section-head{{display:flex;align-items:end;justify-content:space-between;gap:16px;margin:24px 0 9px}}
    .section-head h2{{font-size:17px;margin:0}} .section-head p{{margin:0;color:var(--muted);font-size:12px}}
    .panel{{background:var(--paper);border:1px solid var(--line);border-radius:var(--radius);padding:14px}}
    .panel-title{{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:10px}}
    .panel-title h3{{font-size:14px;margin:0}} .panel-title span{{font-size:12px;color:var(--muted)}}
    .results-grid{{display:grid;grid-template-columns:1fr 1fr;gap:10px}}
    .wide{{grid-column:1/-1}}
    .segmented{{display:flex;border:1px solid var(--line);border-radius:5px;overflow:hidden;background:#fff}}
    .segmented button{{border:0;border-right:1px solid var(--line);background:#fff;color:var(--muted);min-height:34px;padding:0 13px;font:600 13px inherit;cursor:pointer}}
    .segmented button:last-child{{border-right:0}} .segmented button.active{{background:var(--ink);color:#fff}}
    .facts{{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;background:var(--line);border:1px solid var(--line);border-radius:var(--radius);overflow:hidden}}
    .fact{{background:#fff;padding:12px}} .fact label{{display:block;color:var(--muted);font-size:11px;margin-bottom:3px}}
    .fact strong{{font:600 14px ui-monospace,SFMono-Regular,Menlo,monospace}}
    .metric-grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(140px,1fr));gap:8px}}
    .detail-metric-grid{{display:grid;grid-template-columns:repeat(auto-fit,minmax(132px,1fr));gap:8px}}
    .metric-card{{border:1px solid var(--line);border-radius:8px;background:#fff;padding:10px 12px}}
    .metric-card label{{display:block;color:var(--muted);font-size:11px;margin-bottom:4px}}
    .metric-card strong{{font:600 14px ui-monospace,SFMono-Regular,Menlo,monospace}}
    .round-list{{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:8px;margin-top:10px}}
    .round-chip{{border:1px solid var(--line);border-radius:10px;background:#fff;padding:10px 12px;text-align:left;cursor:pointer}}
    .round-chip.active{{border-color:var(--blue);box-shadow:0 0 0 2px #1261a01a;background:#f7fbff}}
    .round-chip.fail{{border-color:#f0c8c4;background:#fff9f8}}
    .round-chip-top{{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px}}
    .round-chip-title{{font:700 13px/1.2 ui-monospace,SFMono-Regular,Menlo,monospace}}
    .round-chip-badge{{font:700 11px/1 ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted)}}
    .round-chip-metrics{{display:grid;grid-template-columns:repeat(2,1fr);gap:6px 10px}}
    .round-chip-metric label{{display:block;color:var(--muted);font-size:10px;margin-bottom:2px}}
    .round-chip-metric strong{{font:600 12px ui-monospace,SFMono-Regular,Menlo,monospace}}
    .table-wrap{{overflow:auto}}
    .table-wrap.compact{{max-height:340px}}
    table{{width:100%;border-collapse:collapse;font-size:12px}}
    th,td{{padding:8px 10px;border-bottom:1px solid var(--line);text-align:left;vertical-align:top}}
    th{{color:var(--muted);font-weight:600;background:#fbfcfd;position:sticky;top:0}}
    td.mono{{font:12px/1.45 ui-monospace,SFMono-Regular,Menlo,monospace}}
    tr.selectable{{cursor:pointer}}
    tr.active-row td{{background:#1261a00d}}
    .note{{font-size:12px;color:var(--muted)}}
    .chart-wrap{{height:320px;position:relative}} .chart-wrap.compact{{height:260px}} canvas{{width:100%;height:100%;display:block}}
    .chart-wrap.short{{height:220px}}
    .chart-tip{{display:none;position:absolute;pointer-events:none;background:#111d24;color:#fff;border-radius:4px;padding:7px 9px;font:12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace;box-shadow:0 4px 16px #0003;white-space:nowrap;z-index:5;min-width:150px}}
    .chart-tip .tip-title{{font-weight:700;margin-bottom:2px}}
    .chart-tip .tip-row{{display:flex;align-items:center;justify-content:space-between;gap:14px}}
    .chart-tip .tip-dot{{font-size:13px;line-height:1}}
    .detail-grid{{display:grid;grid-template-columns:1fr 1fr;gap:16px 20px}}
    .detail-column{{display:grid;gap:10px}}
    .subpanel{{border:1px solid var(--line);border-radius:var(--radius);background:#fff;padding:12px}}
    .subpanel-title{{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px}}
    .subpanel-title h4{{margin:0;font-size:13px}}
    .subpanel-title span{{color:var(--muted);font-size:12px}}
    .round-detail-shell{{margin-top:16px;padding-top:14px;border-top:1px solid var(--line)}}
    .round-detail-head{{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:10px}}
    .round-detail-head h4{{margin:0;font-size:14px}}
    .round-detail-head span{{color:var(--muted);font-size:12px}}
    .detail-section{{min-width:0;padding-top:10px;border-top:1px solid var(--line)}}
    .detail-section-head{{display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:8px}}
    .detail-section-head h4{{margin:0;font-size:13px}}
    .detail-section-head span{{color:var(--muted);font-size:12px}}
    .detail-span-2{{grid-column:1 / -1}}
    .bullet-list{{display:grid;gap:10px}}
    .bullet-item{{padding:0}}
    .bullet-item strong{{display:block;font:600 12px ui-monospace,SFMono-Regular,Menlo,monospace;margin-bottom:3px}}
    .bullet-item span{{color:var(--muted);font-size:12px;line-height:1.5}}
    .provenance{{margin-top:18px;color:var(--muted);font-size:12px}} .provenance code{{color:var(--ink)}}
    @media(max-width:900px){{.shell{{padding:14px}}.topbar{{display:block}}.stamp{{text-align:left;margin-top:8px}}.summary,.results-grid,.detail-grid{{grid-template-columns:1fr}}.wide{{grid-column:auto}}.facts{{grid-template-columns:1fr 1fr}}.round-list{{grid-template-columns:1fr}}.chart-wrap{{height:280px}}.detail-span-2{{grid-column:auto}}}}
  </style>
</head>
<body>
  <main class="shell">
    <header class="topbar">
      <div>
        <div class="eyebrow">Flux Purr / 5A Flagship Preliminary Review</div>
        <h1 id="title"></h1>
        <div class="subtitle" id="subtitle"></div>
        <div class="meta" id="meta"></div>
      </div>
      <div class="stamp" id="stamp"></div>
    </header>
    <section class="summary" id="summary"></section>
    <div class="section-head">
      <div>
        <h2>测试结果</h2>
        <p>只展示 60 / 140 / 220°C 旗舰三点；full-speed-to-stable 按目标温度使用动态门槛：≤150°C 为 10s，>150°C 为 5s。</p>
      </div>
      <div class="segmented" id="targetTabs"></div>
    </div>
    <section class="results-grid">
      <article class="panel wide">
        <div class="panel-title"><h3>温度响应 + 加热</h3><span>背景：预热 / 接近 / 保温</span></div>
        <div class="chart-wrap"><canvas id="temperature"></canvas><div class="chart-tip"></div></div>
      </article>
      <article class="panel">
        <div class="panel-title"><h3>控制输出</h3><span>加热输出 / PD 请求电压；hover 显示全部曲线</span></div>
        <div class="chart-wrap compact"><canvas id="control"></canvas><div class="chart-tip"></div></div>
      </article>
      <article class="panel">
        <div class="panel-title"><h3>Source telemetry</h3><span>电压 / 电流 / 功率；hover 显示全部曲线</span></div>
        <div class="chart-wrap compact"><canvas id="source"></canvas><div class="chart-tip"></div></div>
      </article>
      <article class="panel wide">
        <div class="panel-title"><h3>验收事实</h3><span>包含 source-aware 指标</span></div>
        <div class="facts" id="facts"></div>
      </article>
      <article class="panel wide">
        <div class="panel-title"><h3>当前生效参数</h3><span>显式锚点 / 实际生效参数</span></div>
        <div class="metric-grid" id="pointFacts"></div>
        <div class="note" id="pointNote"></div>
      </article>
      <article class="panel wide">
        <div class="panel-title"><h3>轮次参数与评价</h3><span>点击表格行切换；下方详情包含图表</span></div>
        <div class="table-wrap compact"><table id="roundsTable"></table></div>
        <div class="note" id="roundsNote"></div>
        <section class="round-detail-shell">
          <div class="round-detail-head"><h4>当前测试详情</h4><span id="selectedRoundLabel"></span></div>
          <div class="detail-grid">
            <section class="detail-section">
              <div class="detail-section-head"><h4>单次温度响应</h4><span>温度曲线</span></div>
              <div class="chart-wrap short"><canvas id="roundTemperature"></canvas><div class="chart-tip"></div></div>
            </section>
            <section class="detail-section">
              <div class="detail-section-head"><h4>单次控制 + Source</h4><span>加热 / 请求电压 / source 功率；hover 显示全部曲线</span></div>
              <div class="chart-wrap short"><canvas id="roundControl"></canvas><div class="chart-tip"></div></div>
            </section>
            <section class="detail-section">
              <div class="detail-section-head"><h4>测试参数</h4><span id="roundPointSource"></span></div>
              <div class="detail-metric-grid" id="roundPointFacts"></div>
            </section>
            <section class="detail-section">
              <div class="detail-section-head"><h4>测试评价</h4><span id="roundStatusLabel"></span></div>
              <div class="facts" id="selectedRoundFacts"></div>
            </section>
            <section class="detail-section">
              <div class="detail-section-head"><h4>分阶段统计</h4><span>warmup / approach / hold</span></div>
              <div class="facts" id="roundPhaseFacts"></div>
            </section>
            <section class="detail-section">
              <div class="detail-section-head"><h4>Source 聚合</h4><span>按选中 round 样本聚合</span></div>
              <div class="facts" id="roundSourceFacts"></div>
            </section>
            <section class="detail-section detail-span-2">
              <div class="detail-section-head"><h4>自动判定说明</h4><span id="roundDecisionLabel"></span></div>
              <div class="bullet-list" id="roundNarrative"></div>
            </section>
          </div>
        </section>
      </article>
    </section>
    <footer class="provenance">Bundle 文件：<code>index.html</code> / <code>run.bundle.json</code> / <code>samples.ndjson</code> / <code>thermal-profile.accepted.json</code></footer>
  </main>
  <script>
  const DATA={data_json};
  const COLORS={{60:'#18794e',80:'#c07800',100:'#6f4ba8',120:'#586f7c',140:'#1261a0',160:'#2f7d32',180:'#c23b32',220:'#157a82',240:'#a35f00'}};
  const PHASE={{warmup:'#dcebf4',approach:'#fff0c2',hold:'#dff1e7'}};
  const fmt=(n,d=2)=>n==null?'—':Number(n).toFixed(d);
  const fmtMaybe=(n,d=2,suffix='')=>n==null?'—':fmt(n,d)+suffix;
  const avg=values=>{{const nums=values.map(value=>Number(value)).filter(value=>Number.isFinite(value));return nums.length?nums.reduce((sum,value)=>sum+value,0)/nums.length:null;}};
  const maxOf=values=>{{const nums=values.map(value=>Number(value)).filter(value=>Number.isFinite(value));return nums.length?Math.max(...nums):null;}};
  const minOf=values=>{{const nums=values.map(value=>Number(value)).filter(value=>Number.isFinite(value));return nums.length?Math.min(...nums):null;}};
  const pointFields=[
    ['targetTempC','target'],
    ['brakeDistanceCentiC','brake'],
    ['warmupPowerPermille','warmup'],
    ['approachPowerPermille','approachPower'],
    ['approachFloorPowerPermille','approachFloor'],
    ['approachLeadTicks','approachLead'],
    ['holdEntryCentiC','holdEntry'],
    ['holdPowerPermille','holdPower'],
    ['holdReheatPowerPermille','holdReheat'],
    ['holdOffCentiC','holdOff'],
  ];
  const byTarget=new Map(DATA.runs.map(run=>[run.target,run]));
  const activeRoundByTarget=new Map();
  let active=DATA.runs[0]?.target??60;
  document.querySelector('#title').textContent=DATA.title;
  document.querySelector('#subtitle').textContent=DATA.subtitle;
  document.querySelector('#meta').innerHTML=[
    `选择模式 <b>${{DATA.selectedMode}}</b>`,
    `EEPROM bank <b>${{DATA.resolvedBank}}</b>`,
    `检测能力 <b>${{DATA.detectedSourceClass}}</b>`,
    `Source preset <b>${{DATA.sourcePreset}}</b>`,
    `Provider <b>${{DATA.provider}}</b>`
  ].map(item=>`<span>${{item}}</span>`).join('');
  document.querySelector('#stamp').innerHTML=`DEVICE ${{DATA.deviceId||'—'}}<br>PORT ${{DATA.port||'—'}}<br>SOURCE ${{DATA.sourceDeviceId||'—'}}<br>REPORT ${{new Date(DATA.generatedAt).toLocaleString('zh-CN',{{hour12:false}})}}`;
  document.querySelector('#summary').innerHTML=DATA.runs.map(run=>{{const result=run.result||{{}};const ref=run.approachReference||{{}};const stable=result.fullSpeedToStable||{{}};const gateMs=stable.limitMs??ref.limitMs??null;const settleS=stable.settleTimeMs==null?null:stable.settleTimeMs/1000;const fail=run.failures.map(f=>f.reason||f.failureReason||f.stopReason).join('、');return `<article class="status ${{run.ok?'pass':''}}"><div class="status-head"><span class="temp-label">${{run.target}}°C</span><span class="badge">${{run.budgetOutcome||'pending'}}</span></div><div class="status-metric">耗时 <strong>${{fmtMaybe(run.timeSpentSeconds,0,'s')}}</strong> · 有效测试 <strong>${{run.validTestCount??run.roundCount??0}}</strong> · 门槛 <strong>${{gateMs==null?'—':fmtMaybe(gateMs/1000,0,'s')}}</strong></div><div class="status-metric">full-speed <strong>${{fmtMaybe(settleS,3,'s')}}</strong> · 过冲 <strong>${{fmtMaybe(result.maxOvershootC,2,'°C')}}</strong> · 峰峰值 <strong>${{fmtMaybe(result.holdPeakToPeakC,2,'°C')}}</strong> · ${{run.ok?'通过 preliminary review':(fail||'存在失败')}}</div></article>`}}).join('');
  const tabs=document.querySelector('#targetTabs');
  tabs.innerHTML=DATA.runs.map(run=>`<button data-target="${{run.target}}" class="${{run.target===active?'active':''}}">${{run.target}}°C</button>`).join('');
  function currentRun(){{return byTarget.get(active);}}
  function roundsForRun(run){{return Array.isArray(run?.rounds)?run.rounds:[];}}
  function phaseStats(samples,phase){{const list=(samples||[]).filter(sample=>sample.phase===phase);if(!list.length)return null;return{{count:list.length,startS:list[0].t,endS:list.at(-1).t,durationS:list.at(-1).t-list[0].t,startTempC:list[0].temp,endTempC:list.at(-1).temp,avgCommand:avg(list.map(sample=>sample.command)),avgRequestV:avg(list.map(sample=>sample.requestV)),avgSourcePowerW:avg(list.map(sample=>sample.sourcePowerW))}};}}
  function ensureActiveRound(){{const run=currentRun();const rounds=roundsForRun(run);if(!rounds.length)return;const selected=activeRoundByTarget.get(run.target);if(!rounds.some(round=>round.round===selected))activeRoundByTarget.set(run.target,rounds.at(-1).round);}}
  function currentRound(){{const run=currentRun();const rounds=roundsForRun(run);if(!rounds.length)return null;ensureActiveRound();const selected=activeRoundByTarget.get(run.target);return rounds.find(round=>round.round===selected) || rounds.at(-1) || null;}}
  tabs.onclick=event=>{{const button=event.target.closest('button[data-target]');if(!button)return;active=Number(button.dataset.target);tabs.querySelectorAll('button').forEach(node=>node.classList.toggle('active',Number(node.dataset.target)===active));ensureActiveRound();renderAll();}};
  const views={{}};
  function setupCanvas(id,draw){{const canvas=document.querySelector('#'+id),wrap=canvas.parentElement,tip=wrap.querySelector('.chart-tip');function render(){{const dpr=devicePixelRatio||1,rect=canvas.getBoundingClientRect();canvas.width=rect.width*dpr;canvas.height=rect.height*dpr;const c=canvas.getContext('2d');c.scale(dpr,dpr);draw(c,rect.width,rect.height,views[id]||{{}},null,false);}}canvas.onmousemove=event=>{{const rect=canvas.getBoundingClientRect(),point={{x:event.clientX-rect.left,y:event.clientY-rect.top}};const ctx=canvas.getContext('2d');const info=draw(ctx,rect.width,rect.height,views[id]||{{}},point,true);if(info){{tip.style.display='block';tip.style.left=Math.min(point.x+12,rect.width-220)+'px';tip.style.top=Math.max(4,point.y-60)+'px';tip.innerHTML=info;}}else tip.style.display='none';}};canvas.onmouseleave=()=>tip.style.display='none';canvas.onwheel=event=>{{event.preventDefault();const view=views[id]||(views[id]={{zoom:1,offset:0}}),rect=canvas.getBoundingClientRect(),plotLeft=54,plotRight=18,plotWidth=Math.max(1,rect.width-plotLeft-plotRight),cursorRatio=Math.max(0,Math.min(1,(event.clientX-rect.left-plotLeft)/plotWidth)),oldZoom=view.zoom||1,anchor=(view.offset||0)+cursorRatio/oldZoom,factor=event.deltaY<0?1.25:0.8,newZoom=Math.max(1,Math.min(8,oldZoom*factor));view.zoom=newZoom;view.offset=Math.max(0,Math.min(1-1/newZoom,anchor-cursorRatio/newZoom));render();}};new ResizeObserver(render).observe(wrap);return render;}}
  function frame(c,w,h){{const m={{l:54,r:18,t:18,b:34}},pw=w-m.l-m.r,ph=h-m.t-m.b;c.clearRect(0,0,w,h);c.font='11px ui-monospace, monospace';return{{m,pw,ph}};}}
  function tooltipValue(point,item,options){{const value=point.rawY??point.y;const decimals=point.rawDecimals??item.decimals??options.decimals??2;const unit=point.rawUnit??item.unit??options.unit??'';return fmt(value,decimals)+unit;}}
  function seriesTooltip(anchor,items,options){{const rows=items.map(point=>`<div class="tip-row"><span><span class="tip-dot" style="color:${{point.color}}">●</span> ${{point.name}}</span><strong>${{tooltipValue(point,point.item,options)}}</strong></div>`).join('');return `<div class="tip-title">${{fmt(anchor.x,2)}} s</div>${{rows}}`;}}
  function lineChart(id,seriesFactory,options={{}}){{return setupCanvas(id,(c,w,h,view,hover,hit)=>{{const {{m,pw,ph}}=frame(c,w,h),series=seriesFactory(),all=series.flatMap(item=>item.data).filter(point=>Number.isFinite(point.y));if(!all.length)return null;const minX=Math.min(...all.map(point=>point.x)),maxX=Math.max(...all.map(point=>point.x)),zoom=view.zoom||1,start=minX+(maxX-minX)*(view.offset||0),end=start+(maxX-minX)/zoom,visible=all.filter(point=>point.x>=start&&point.x<=end),minY=options.yMin??Math.min(...visible.map(point=>point.y)),maxY=options.yMax??Math.max(...visible.map(point=>point.y)),pad=(maxY-minY||1)*0.08,Y0=minY-pad,Y1=maxY+pad,x=value=>m.l+(value-start)/(end-start)*pw,y=value=>m.t+(Y1-value)/(Y1-Y0)*ph;if(options.context){{for(const sample of options.context().filter(item=>item.t>=start&&item.t<=end)){{const xx=x(sample.t);c.fillStyle=PHASE[sample.phase]??'#f5f6f7';c.globalAlpha=0.45;c.fillRect(xx,m.t,2,ph);c.globalAlpha=1;}}}}for(let i=0;i<=4;i++){{const yy=m.t+ph*i/4;c.beginPath();c.moveTo(m.l,yy);c.lineTo(w-m.r,yy);c.strokeStyle='#e8ebed';c.stroke();const value=Y1-(Y1-Y0)*i/4;c.fillStyle='#66717a';c.fillText(fmt(value,options.decimals??1),5,yy+4);}}if(options.band){{const band=typeof options.band==='function'?options.band():options.band;c.fillStyle='rgba(24,121,78,.09)';c.fillRect(m.l,y(band[1]),pw,y(band[0])-y(band[1]));}}let nearest=null;const painted=[];for(const item of series){{c.beginPath();let begun=false;const points=[];for(const point of item.data){{if(point.x<start||point.x>end||!Number.isFinite(point.y))continue;const xx=x(point.x),yy=y(point.y),paintedPoint={{...point,item,dist:Infinity,name:item.name,color:item.color,xx,yy}};points.push(paintedPoint);begun?c.lineTo(xx,yy):(c.moveTo(xx,yy),begun=true);if(hover){{const dist=Math.abs(xx-hover.x);if(!nearest||dist<nearest.dist)nearest={{...paintedPoint,dist}};}}}}painted.push({{item,points}});c.strokeStyle=item.color;c.lineWidth=item.width||2;c.setLineDash(item.dash||[]);c.stroke();c.setLineDash([]);}}for(let i=0;i<=5;i++){{const xx=m.l+pw*i/5,val=start+(end-start)*i/5;c.fillStyle='#66717a';c.fillText(fmt(val,0)+'s',xx-10,h-10);}}if(hit&&nearest&&nearest.dist<18){{const items=painted.map(group=>group.points.reduce((best,point)=>{{const dist=Math.abs(point.xx-nearest.xx);return !best||dist<best.dist?{{...point,dist}}:best;}},null)).filter(point=>point&&point.dist<18);c.strokeStyle='#66717a';c.beginPath();c.moveTo(nearest.xx,m.t);c.lineTo(nearest.xx,m.t+ph);c.stroke();return options.tooltip?options.tooltip(nearest,items):seriesTooltip(nearest,items,options);}}return null;}});}}
  const temperatureRender=lineChart('temperature',()=>{{const run=currentRun();return [{{name:'实测温度',color:COLORS[active],data:run.samples.map(sample=>({{x:sample.t,y:sample.temp,...sample}}))}},{{name:'目标温度',color:'#182026',dash:[5,4],width:1,data:[{{x:0,y:active}},{{x:run.samples.at(-1)?.t||0,y:active}}]}}];}},{{unit:'°C',band:()=>[active-1.5,active+1.5],context:()=>currentRun()?.samples||[],tooltip:sample=>`<strong>${{sample.phase?.toUpperCase()||'SAMPLE'}}</strong><br>${{fmt(sample.x,2)}} s · 温度 ${{fmt(sample.temp,2)}}°C<br>滤波 ${{fmt(sample.filtered,2)}}°C · 请求 ${{fmt(sample.requestV,2)}}V<br>电源 ${{fmt(sample.sourceVoltageV,2)}}V / ${{fmt(sample.sourceCurrentA,2)}}A / ${{fmt(sample.sourcePowerW,2)}}W`}});
  const controlRender=lineChart('control',()=>{{const run=currentRun();return [{{name:'加热命令',color:'#c23b32',unit:'%',decimals:0,data:run.samples.map(sample=>({{x:sample.t,y:sample.command}}))}},{{name:'PD 请求电压',color:'#1261a0',unit:'V',decimals:2,data:run.samples.map(sample=>({{x:sample.t,y:(sample.requestV||0)*5,rawY:sample.requestV,rawUnit:'V',rawDecimals:2}}))}}];}},{{unit:'%',yMin:0,yMax:105,decimals:0}});
  const sourceRender=lineChart('source',()=>{{const run=currentRun();return [{{name:'电压',color:'#1261a0',unit:'V',decimals:2,data:run.samples.map(sample=>({{x:sample.t,y:sample.sourceVoltageV,rawY:sample.sourceVoltageV,rawUnit:'V',rawDecimals:2}})).filter(item=>item.y!=null)}},{{name:'电流',color:'#157a82',unit:'A',decimals:2,data:run.samples.map(sample=>({{x:sample.t,y:(sample.sourceCurrentA==null?null:sample.sourceCurrentA*4),rawY:sample.sourceCurrentA,rawUnit:'A',rawDecimals:2}})).filter(item=>item.y!=null)}},{{name:'功率',color:'#c23b32',unit:'W',decimals:2,data:run.samples.map(sample=>({{x:sample.t,y:(sample.sourcePowerW==null?null:sample.sourcePowerW/4),rawY:sample.sourcePowerW,rawUnit:'W',rawDecimals:2}})).filter(item=>item.y!=null)}}];}},{{decimals:1}});
  const roundTemperatureRender=lineChart('roundTemperature',()=>{{const round=currentRound();const samples=round?.samples||[];return [{{name:'单轮温度',color:COLORS[active],data:samples.map(sample=>({{x:sample.t,y:sample.temp,...sample}}))}},{{name:'目标温度',color:'#182026',dash:[5,4],width:1,data:samples.length?[{{x:0,y:active}},{{x:samples.at(-1)?.t||0,y:active}}]:[]}}];}},{{unit:'°C',band:()=>[active-1.5,active+1.5],context:()=>currentRound()?.samples||[],tooltip:sample=>`<strong>ROUND ${{currentRound()?.round??'—'}} / ${{sample.phase?.toUpperCase()||'SAMPLE'}}</strong><br>${{fmt(sample.x,2)}} s · 温度 ${{fmt(sample.temp,2)}}°C<br>滤波 ${{fmt(sample.filtered,2)}}°C · 请求 ${{fmt(sample.requestV,2)}}V<br>电源 ${{fmt(sample.sourceVoltageV,2)}}V / ${{fmt(sample.sourceCurrentA,2)}}A / ${{fmt(sample.sourcePowerW,2)}}W`}});
  const roundControlRender=lineChart('roundControl',()=>{{const round=currentRound();const samples=round?.samples||[];return [{{name:'加热命令',color:'#c23b32',unit:'%',decimals:0,data:samples.map(sample=>({{x:sample.t,y:sample.command}}))}},{{name:'PD 请求电压',color:'#1261a0',unit:'V',decimals:2,data:samples.map(sample=>({{x:sample.t,y:(sample.requestV||0)*5,rawY:sample.requestV,rawUnit:'V',rawDecimals:2}}))}},{{name:'source 功率',color:'#157a82',unit:'W',decimals:2,data:samples.map(sample=>({{x:sample.t,y:(sample.sourcePowerW==null?null:sample.sourcePowerW/2),rawY:sample.sourcePowerW,rawUnit:'W',rawDecimals:2}})).filter(item=>item.y!=null)}}];}},{{decimals:1,yMin:0,yMax:110}});
  function renderFacts(){{const run=currentRun();const result=run.result||{{}};const analysis=result.analysis||{{}};const stable=result.fullSpeedToStable||{{}};const ref=run.approachReference||{{}};const gateMs=stable.limitMs??ref.limitMs??null;const facts=[['预算结果',run.budgetOutcome||'—'],['目标耗时',fmtMaybe(run.timeSpentSeconds,0,' s')],['有效测试数',String(run.validTestCount??run.roundCount??0)],['无效测试数',String(run.invalidTestCount??0)],['full-speed 门槛',gateMs==null?'—':fmtMaybe(gateMs/1000,0,' s')],['full-speed 实测',stable.settleTimeMs==null?'未建立':fmtMaybe(stable.settleTimeMs/1000,3,' s')],['full-speed 失败原因',stable.failureReason||'—'],['maxOvershootC',fmtMaybe(result.maxOvershootC,2,' °C')],['holdPeakToPeakC',fmtMaybe(result.holdPeakToPeakC,2,' °C')],['holdMedianOutputPermille',analysis.holdMedianOutputPermille==null?'—':fmt(analysis.holdMedianOutputPermille,0)+' ‰'],['holdP90OutputPermille',analysis.holdP90OutputPermille==null?'—':fmt(analysis.holdP90OutputPermille,0)+' ‰'],['approachSourceAvg',analysis.approachSource?.powerMw?.avg==null?'—':fmt(analysis.approachSource.powerMw.avg/1000,2)+' W'],['holdSourceAvg',analysis.holdSource?.powerMw?.avg==null?'—':fmt(analysis.holdSource.powerMw.avg/1000,2)+' W'],['样本数',String(run.samples.length)],['失败原因',run.failures.map(f=>f.reason||f.failureReason||f.stopReason).join('、')||'—']];document.querySelector('#facts').innerHTML=facts.map(item=>`<div class="fact"><label>${{item[0]}}</label><strong>${{item[1]}}</strong></div>`).join('');}}
  function renderPoint(){{const run=currentRun();const point=run.point||{{}};document.querySelector('#pointFacts').innerHTML=pointFields.filter(([key])=>point[key]!=null).map(([key,label])=>`<div class="metric-card"><label>${{label}}</label><strong>${{point[key]}}</strong></div>`).join('');document.querySelector('#pointNote').textContent=run.pointSource==='review_candidate_snapshot'?'当前目标是 preliminary review candidate 的显式快照，不代表 EEPROM saved bank。':'当前目标不是显式快照；这里展示的是样本中实际生效的 heaterParameters。';}}
  function renderRoundSummary(){{const run=currentRun();const rounds=roundsForRun(run);const selected=currentRound();document.querySelector('#selectedRoundLabel').textContent=selected?`测试 ${{selected.round}} / 共 ${{rounds.length}} 次`:'无测试';}}
  function renderRoundDetail(){{
    const run=currentRun();
    const rounds=roundsForRun(run);
    const round=currentRound();
    if(!round){{
      document.querySelector('#selectedRoundLabel').textContent='无测试';
      document.querySelector('#roundPointSource').textContent='';
      document.querySelector('#roundStatusLabel').textContent='';
      document.querySelector('#selectedRoundFacts').innerHTML='';
      document.querySelector('#roundPointFacts').innerHTML='';
      document.querySelector('#roundsTable').innerHTML='';
      document.querySelector('#roundsNote').textContent='这个目标当前没有记录到独立测试；报告仍会显示最终生效参数。';
      document.querySelector('#roundPhaseFacts').innerHTML='';
      document.querySelector('#roundDecisionLabel').textContent='';
      document.querySelector('#roundNarrative').innerHTML='';
      document.querySelector('#roundSourceFacts').innerHTML='';
      return;
    }}
    const point=round.point||{{}};
    const samples=round.samples||[];
    const warmup=phaseStats(samples,'warmup');
    const approach=phaseStats(samples,'approach');
    const hold=phaseStats(samples,'hold');
    const peakTemp=maxOf(samples.map(sample=>sample.temp));
    const finalTemp=samples.at(-1)?.temp??null;
    const fullSpeedLimitMs=round.result?.fullSpeedLimitMs??(run.approachReference?.limitMs??null);
    const fullSpeedS=round.result?.settleTimeMs==null?null:round.result.settleTimeMs/1000;
    document.querySelector('#roundPointSource').textContent=`${{round.attemptType||'test'}}${{round.candidateName?' / '+round.candidateName:''}}${{round.selected?' / selected':''}}`;
    document.querySelector('#roundStatusLabel').textContent=round.result?.stopReason||'—';
    document.querySelector('#selectedRoundFacts').innerHTML=[
      ['测试类型',round.attemptType||'—'],
      ['候选',round.candidateName||'—'],
      ['是否采用',round.selected?'是':'否'],
      ['证据有效',round.evidenceValid===false?'否':'是'],
      ['stopReason',round.result?.stopReason||'—'],
      ['失败分类',round.result?.stabilityEvidence?.failureClass||'—'],
      ['full-speed 门槛',fullSpeedLimitMs==null?'—':fmtMaybe(fullSpeedLimitMs/1000,0,' s')],
      ['full-speed 实测',fmtMaybe(fullSpeedS,3,' s')],
      ['overshoot',fmtMaybe(round.result?.maxOvershootC,2,' °C')],
      ['p2p',fmtMaybe(round.result?.holdPeakToPeakC,2,' °C')],
      ['fullSpeed诊断',round.result?.settleTimeMs==null?'—':fmtMaybe(round.result.settleTimeMs/1000,3,' s')],
      ['sample count',String(samples.length)]
    ].map(([label,value])=>`<div class="fact"><label>${{label}}</label><strong>${{value}}</strong></div>`).join('');
    document.querySelector('#roundPointFacts').innerHTML=pointFields.filter(([key])=>point[key]!=null).map(([key,label])=>`<div class="metric-card"><label>${{label}}</label><strong>${{point[key]}}</strong></div>`).join('');
    const head='<tr><th>测试</th><th>类型</th><th>候选</th><th>采用</th><th>失败分类</th><th>stopReason</th><th>full-speed</th><th>limit</th><th>overshoot</th><th>p2p</th><th>brake</th><th>approachPower</th><th>approachFloor</th><th>holdEntry</th><th>holdPower</th><th>holdReheat</th></tr>';
    const rows=rounds.map(item=>{{const p=item.point||{{}};const result=item.result||{{}};const activeClass=item.round===round.round?'active-row':'';const limit=result.fullSpeedLimitMs??(run.approachReference?.limitMs??null);const settle=result.settleTimeMs==null?'—':fmtMaybe(result.settleTimeMs/1000,3,'s');const failureClass=result.stabilityEvidence?.failureClass||'—';return `<tr class="selectable ${{activeClass}}" data-round="${{item.round}}"><td class="mono">${{item.round}}</td><td class="mono">${{item.attemptType||'—'}}</td><td class="mono">${{item.candidateName||'—'}}</td><td class="mono">${{item.selected?'✓':'—'}}</td><td class="mono">${{failureClass}}</td><td class="mono">${{result.stopReason||'—'}}</td><td class="mono">${{settle}}</td><td class="mono">${{limit==null?'—':fmtMaybe(limit/1000,0,'s')}}</td><td class="mono">${{fmtMaybe(result.maxOvershootC,2,'°C')}}</td><td class="mono">${{fmtMaybe(result.holdPeakToPeakC,2,'°C')}}</td><td class="mono">${{p.brakeDistanceCentiC??'—'}}</td><td class="mono">${{p.approachPowerPermille??'—'}}</td><td class="mono">${{p.approachFloorPowerPermille??'—'}}</td><td class="mono">${{p.holdEntryCentiC??'—'}}</td><td class="mono">${{p.holdPowerPermille??'—'}}</td><td class="mono">${{p.holdReheatPowerPermille??'—'}}</td></tr>`;}});
    document.querySelector('#roundsTable').innerHTML=head+rows.join('');
    document.querySelector('#roundsTable').querySelectorAll('tr[data-round]').forEach(row=>row.onclick=()=>{{activeRoundByTarget.set(run.target,Number(row.dataset.round));renderAll();}});
    document.querySelector('#roundsNote').textContent=`共 ${{rounds.length}} 次有效测试；当前选中测试 ${{round.round}}。scout、全部 batch candidate 与 confirm 均按实际执行顺序保留。`;
    document.querySelector('#roundPhaseFacts').innerHTML=[
      ['warmup end',warmup?fmtMaybe(warmup.endTempC,2,' °C')+' @ '+fmtMaybe(warmup.endS,2,' s'):'—'],
      ['approach duration',approach?fmtMaybe(approach.durationS,2,' s'):'—'],
      ['hold duration',hold?fmtMaybe(hold.durationS,2,' s'):'—'],
      ['peak temp',fmtMaybe(peakTemp,2,' °C')],
      ['final temp',fmtMaybe(finalTemp,2,' °C')],
      ['avg command',fmtMaybe(avg(samples.map(sample=>sample.command)),1,' %')],
      ['warmup avg power',fmtMaybe(warmup?.avgSourcePowerW,2,' W')],
      ['approach avg power',fmtMaybe(approach?.avgSourcePowerW,2,' W')],
      ['hold avg power',fmtMaybe(hold?.avgSourcePowerW,2,' W')],
      ['warmup avg request',fmtMaybe(warmup?.avgRequestV,2,' V')],
      ['approach avg request',fmtMaybe(approach?.avgRequestV,2,' V')],
      ['hold avg request',fmtMaybe(hold?.avgRequestV,2,' V')]
    ].map(([label,value])=>`<div class="fact"><label>${{label}}</label><strong>${{value}}</strong></div>`).join('');
    document.querySelector('#roundDecisionLabel').textContent=round.result?.stopReason==='completed'?'completed':'needs review';
    document.querySelector('#roundNarrative').innerHTML=[
      ['测试结论',round.result?.stopReason==='completed'?'该次测试完成，可进入候选比较。':'该次测试没有完成，需要结合失败分类与曲线继续看。'],
      ['调参方向',`分类 ${{round.result?.stabilityEvidence?.failureClass||'—'}}；${{round.selected?'该候选被采用。':'该候选未被采用。'}}`],
      ['full-speed 门槛',`≤150°C 为 10s，>150°C 为 5s；本轮实测 ${{fmtMaybe(fullSpeedS,3,'s')}}，门槛 ${{fullSpeedLimitMs==null?'—':fmtMaybe(fullSpeedLimitMs/1000,0,'s')}}。`],
      ['保温表现',`overshoot ${{fmtMaybe(round.result?.maxOvershootC,2,'°C')}}，p2p ${{fmtMaybe(round.result?.holdPeakToPeakC,2,'°C')}}，最终温度 ${{fmtMaybe(finalTemp,2,'°C')}}。`],
      ['当前参数',`brake ${{point.brakeDistanceCentiC??'—'}} · approachPower ${{point.approachPowerPermille??'—'}} · holdPower ${{point.holdPowerPermille??'—'}} · holdReheat ${{point.holdReheatPowerPermille??'—'}}。`]
    ].map(([title,body])=>`<div class="bullet-item"><strong>${{title}}</strong><span>${{body}}</span></div>`).join('');
    document.querySelector('#roundSourceFacts').innerHTML=[
      ['source V avg',fmtMaybe(avg(samples.map(sample=>sample.sourceVoltageV)),2,' V')],
      ['source V max',fmtMaybe(maxOf(samples.map(sample=>sample.sourceVoltageV)),2,' V')],
      ['source I avg',fmtMaybe(avg(samples.map(sample=>sample.sourceCurrentA)),2,' A')],
      ['source I max',fmtMaybe(maxOf(samples.map(sample=>sample.sourceCurrentA)),2,' A')],
      ['source P avg',fmtMaybe(avg(samples.map(sample=>sample.sourcePowerW)),2,' W')],
      ['source P max',fmtMaybe(maxOf(samples.map(sample=>sample.sourcePowerW)),2,' W')],
      ['request V avg',fmtMaybe(avg(samples.map(sample=>sample.requestV)),2,' V')],
      ['request V min',fmtMaybe(minOf(samples.map(sample=>sample.requestV)),2,' V')]
    ].map(([label,value])=>`<div class="fact"><label>${{label}}</label><strong>${{value}}</strong></div>`).join('');
  }}
  function renderAll(){{ensureActiveRound();temperatureRender();controlRender();sourceRender();renderFacts();renderPoint();renderRoundSummary();renderRoundDetail();roundTemperatureRender();roundControlRender();}}
  ensureActiveRound();
  renderAll();
  </script>
</body>
</html>
"""


@dataclass
class SelfTestRun:
    output: dict[str, Any]
    summary: dict[str, Any]
    summary_path: Path
    run_dir: Path
    samples_path: Path


class FluxPurrRunner:
    def __init__(
        self,
        flux_purr_bin: Path,
        devd_url: str,
        authorized_port: str,
        source_id: str,
        source_url: str,
        dry_run: bool,
        auto_recover_source: bool,
    ):
        self.flux_purr_bin = flux_purr_bin
        self.devd_url = devd_url.rstrip("/")
        self.authorized_port = authorized_port
        self.source_id = source_id
        self.source_url = source_url
        self.dry_run = dry_run
        self.auto_recover_source = auto_recover_source
        self._resolved_device_id: str | None = None

    def run_subprocess_json(self, cmd: list[str]) -> dict[str, Any]:
        log(f"$ {shlex.join(cmd)}")
        proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
        if proc.returncode != 0:
            raise RuntimeError(
                f"command failed ({proc.returncode}): {shlex.join(cmd)}\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
            )
        if not proc.stdout.strip():
            raise RuntimeError(f"command produced empty stdout: {shlex.join(cmd)}")
        return json.loads(proc.stdout)

    def resolve_device_id(self, dry_run_override: bool = False) -> str:
        if self.dry_run or dry_run_override:
            return "mock-fp-lab-01"
        if self._resolved_device_id is not None:
            return self._resolved_device_id
        url = f"{self.devd_url}/api/v1/devices"
        req = urllib.request.Request(url, method="GET")
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                payload = json.loads(resp.read().decode())
        except urllib.error.URLError as exc:
            raise RuntimeError(f"failed to query devd devices at {url}: {exc}") from exc
        devices = payload.get("devices") if isinstance(payload, dict) else None
        if not isinstance(devices, list):
            raise RuntimeError("unexpected devd /devices payload")
        matches = [device for device in devices if isinstance(device, dict) and device.get("portPath") == self.authorized_port]
        if len(matches) != 1:
            raise RuntimeError(
                f"expected exactly one device on authorized port {self.authorized_port}, got {len(matches)}"
            )
        self._resolved_device_id = str(matches[0]["id"])
        return self._resolved_device_id

    def recover_source_output(self) -> None:
        if self.dry_run:
            return
        log("source recovery: restart IsolaPurr USB-C output on the same authorized source")
        self.run_subprocess_json(
            [
                "isolapurr",
                "power",
                "output",
                "manual",
                "--url",
                self.source_url,
                "--usb-c-path",
                "disconnected",
                "--json",
            ]
        )
        disconnect_error: RuntimeError | None = None
        for _ in range(SOURCE_RECOVERY_POLL_ATTEMPTS):
            disconnected = self.run_subprocess_json(
                ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
            )
            try:
                verify_isolapurr_power_show(disconnected, expect_usb_c_enabled=False)
                disconnect_error = None
                break
            except RuntimeError as exc:
                disconnect_error = exc
                time.sleep(SOURCE_RECOVERY_POLL_INTERVAL_SECONDS)
        if disconnect_error is not None:
            raise disconnect_error
        time.sleep(SOURCE_RECOVERY_SETTLE_SECONDS)
        self.run_subprocess_json(
            ["isolapurr", "power", "output", "auto", "--url", self.source_url, "--json"]
        )
        baseline = self.run_subprocess_json(
            ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
        )
        previous_sample_uptime_ms = verify_isolapurr_power_show(
            baseline,
            expect_usb_c_enabled=True,
        )
        last_error: RuntimeError | None = None
        for _ in range(SOURCE_RECOVERY_POLL_ATTEMPTS):
            time.sleep(SOURCE_RECOVERY_POLL_INTERVAL_SECONDS)
            current = self.run_subprocess_json(
                ["isolapurr", "power", "show", "--url", self.source_url, "--json"]
            )
            try:
                verify_isolapurr_power_show(
                    current,
                    expect_usb_c_enabled=True,
                    previous_sample_uptime_ms=previous_sample_uptime_ms,
                )
                if not Path(self.authorized_port).exists():
                    raise RuntimeError(
                        f"authorized port disappeared after source recovery: {self.authorized_port}"
                    )
                return
            except RuntimeError as exc:
                last_error = exc
                previous = source_usb_c_sample_uptime_ms(current)
                if previous is not None:
                    previous_sample_uptime_ms = previous
        raise last_error if last_error is not None else RuntimeError("source recovery did not restore live telemetry")

    def run_json_command(
        self,
        cmd: list[str],
        *,
        retry_with_source_recovery: bool = False,
    ) -> dict[str, Any]:
        attempts = 2 if retry_with_source_recovery and self.auto_recover_source and not self.dry_run else 1
        last_error: RuntimeError | None = None
        for attempt in range(attempts):
            log(f"$ {shlex.join(cmd)}")
            proc = subprocess.run(cmd, cwd=REPO_ROOT, capture_output=True, text=True)
            if proc.returncode == 0:
                if not proc.stdout.strip():
                    raise RuntimeError(f"command produced empty stdout: {shlex.join(cmd)}")
                return json.loads(proc.stdout)
            last_error = RuntimeError(
                f"command failed ({proc.returncode}): {shlex.join(cmd)}\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
            )
            if attempt + 1 < attempts:
                self.recover_source_output()
        raise last_error if last_error is not None else RuntimeError(f"command failed: {shlex.join(cmd)}")

    def self_test(
        self,
        *,
        seed_profile_file: Path | None = None,
        candidate_profile_files: list[Path] | None = None,
        targets_c: list[int],
        hold_seconds: int,
        output_dir: Path,
        evaluation_mode: str = EVALUATION_MODE_HOLD_CONFIRM,
        cooldown_temp_c: float | None = None,
        stage_timeout_seconds: int | None = None,
        cooldown_timeout_seconds: int | None = None,
        dry_run_override: bool = False,
    ) -> SelfTestRun:
        dry_run = self.dry_run or dry_run_override
        attempt_dirs = [output_dir, output_dir.with_name(f"{output_dir.name}-rerun1")]
        last_result: SelfTestRun | None = None
        for attempt_index, attempt_dir in enumerate(attempt_dirs):
            attempt_dir.mkdir(parents=True, exist_ok=True)
            cmd = [
                str(self.flux_purr_bin),
                "--devd",
                self.devd_url,
                "--json",
                "thermal",
                "self-test",
                "--source-kind",
                SOURCE_KIND,
                "--source-id",
                self.source_id,
                "--source-url",
                self.source_url,
                "--profile-mode",
                PROFILE_MODE,
                "--source-mode",
                SOURCE_MODE,
                "--skip-optimize",
                "--evaluation-mode",
                evaluation_mode,
                "--hold-seconds",
                str(int(hold_seconds)),
                "--targets-c",
                ",".join(str(target) for target in targets_c),
                "--output-dir",
                cli_arg_path(attempt_dir),
            ]
            if cooldown_temp_c is not None:
                cmd.extend(["--cooldown-temp-c", f"{float(cooldown_temp_c):.1f}"])
            if stage_timeout_seconds is not None:
                cmd.extend(["--stage-timeout-seconds", str(int(stage_timeout_seconds))])
            if cooldown_timeout_seconds is not None:
                cmd.extend(["--cooldown-timeout-seconds", str(int(cooldown_timeout_seconds))])
            if dry_run:
                cmd.extend(["--dry-run", "--device", "mock-fp-lab-01"])
            else:
                cmd.extend(["--device", self.resolve_device_id(False)])
            if seed_profile_file is not None:
                cmd.extend(["--seed-profile-file", cli_arg_path(seed_profile_file)])
            for candidate in candidate_profile_files or []:
                cmd.extend(["--candidate-profile-file", cli_arg_path(candidate)])
            output = self.run_json_command(cmd, retry_with_source_recovery=not dry_run)
            if output.get("kind") == "thermal_self_test_batch":
                batch_id = output["batchId"]
                summary_path = attempt_dir / batch_id / "batch.json"
                summary = read_json(summary_path)
                last_result = SelfTestRun(
                    output=output,
                    summary=summary,
                    summary_path=summary_path,
                    run_dir=summary_path.parent,
                    samples_path=summary_path.parent,
                )
            else:
                summary_path = Path(output["files"]["summaryPath"])
                summary = read_json(summary_path)
                last_result = SelfTestRun(
                    output=output,
                    summary=summary,
                    summary_path=summary_path,
                    run_dir=Path(output["files"]["runDir"]),
                    samples_path=Path(output["files"]["samplesPath"]),
                )
            error_text = str(last_result.summary.get("error") or "")
            no_applied = not (last_result.summary.get("applied") or [])
            no_runs = not (last_result.summary.get("runs") or [])
            if (
                attempt_index == 0
                and "heater runtime readback enable mismatch" in error_text
                and (no_applied or no_runs)
            ):
                log(f"thermal self-test retrying after runtime readback mismatch: {error_text}")
                continue
            return last_result
        if last_result is not None:
            return last_result
        raise RuntimeError("thermal self-test produced no result")

    def retune(self, run_dir: Path, target_temp_c: int) -> tuple[dict[str, Any], Path]:
        cmd = [
            str(self.flux_purr_bin),
            "--devd",
            self.devd_url,
            "--json",
            "thermal",
            "retune",
            "--run-dir",
            cli_arg_path(run_dir),
            "--optimize-targets-c",
            str(target_temp_c),
        ]
        self.run_json_command(cmd)
        candidate_path = run_dir / "thermal-profile.replayed.candidate.json"
        if not candidate_path.exists():
            raise RuntimeError(f"retune did not produce {candidate_path}")
        return read_json(candidate_path), candidate_path


def stage_reference_gate_satisfied(summary: dict[str, Any], target_temp_c: int) -> bool:
    stage = stage_for_target(summary, target_temp_c)
    metrics = stage_metrics(stage)
    evidence = stability_evidence_for_stage(
        stage,
        samples_for_target(summary, target_temp_c),
        target_temp_c,
    )
    return (
        not run_is_disqualified(summary, target_temp_c)
        and metrics.get("stopReason") == "completed"
        and evidence.get("failureClass") != "within_gate_low_margin"
    )


def round_record_from_summary(
    summary: dict[str, Any],
    target_temp_c: int,
    round_number: int,
    label: str,
    point: dict[str, Any] | None,
    *,
    attempt_type: str = "tuning",
    tuning_round: int | None = None,
    candidate_name: str | None = None,
    selected: bool = False,
    score: list[Any] | None = None,
    budget_elapsed_seconds_value: int | None = None,
) -> dict[str, Any]:
    stage = stage_for_target(summary, target_temp_c)
    analysis = stage.get("analysis") if isinstance(stage.get("analysis"), dict) else {}
    stable = stage.get("fullSpeedToStable") if isinstance(stage.get("fullSpeedToStable"), dict) else {}
    failures = validation_failures_for_target(summary, target_temp_c)
    samples = samples_for_target(summary, target_temp_c)
    stability_evidence = stability_evidence_for_stage(stage, samples, target_temp_c)
    return {
        "round": int(round_number),
        "label": label,
        "attemptType": attempt_type,
        "tuningRound": tuning_round,
        "candidateName": candidate_name,
        "selected": bool(selected),
        "evidenceValid": not run_is_disqualified(summary, target_temp_c),
        "summaryPath": repo_display_path(summary_run_path(summary)),
        "point": sanitize_point(point, target_temp_c),
        "samples": samples,
        "failures": failures,
        "result": {
            "stopReason": stage.get("stopReason"),
            "maxOvershootC": stage.get("maxOvershootC"),
            "holdPeakToPeakC": stage.get("holdPeakToPeakC"),
            "settleTimeMs": stable.get("settleTimeMs"),
            "fullSpeedLimitMs": stable.get("limitMs"),
            "approachCurveMeanAbsErrorC": analysis.get("approachCurveMeanAbsErrorC"),
            "approachCurveDeviationClass": analysis.get("approachCurveDeviationClass"),
            "approachReferenceDurationDeltaMs": analysis.get("approachReferenceDurationDeltaMs"),
            "approachReferencePeakDeltaC": analysis.get("approachReferencePeakDeltaC"),
            "approachReferenceClass": analysis.get("approachReferenceClass"),
            "stabilityEvidence": stability_evidence,
            "budgetElapsedSeconds": budget_elapsed_seconds_value,
            "score": score,
        },
    }


def batch_attempt_records(
    batch_summary: dict[str, Any],
    target_temp_c: int,
    *,
    first_round_number: int,
    tuning_round: int,
    selected_run_id: str | None,
    score_by_run_id: dict[str, list[Any]] | None = None,
    budget_elapsed_seconds_value: int | None = None,
) -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []
    for run in batch_summary.get("runs") or []:
        if not isinstance(run, dict):
            continue
        candidate_profile = run.get("candidateProfile") if isinstance(run.get("candidateProfile"), dict) else {}
        parameters = run.get("parameters") if isinstance(run.get("parameters"), dict) else {}
        candidate_path = parameters.get("candidateProfileFile")
        candidate_name = Path(candidate_path).stem if isinstance(candidate_path, str) else None
        run_id = str(run.get("runId") or "")
        records.append(
            round_record_from_summary(
                run,
                target_temp_c,
                first_round_number + len(records),
                f"tuning {tuning_round} / {candidate_name or 'candidate'}",
                explicit_point(candidate_profile, target_temp_c),
                attempt_type="batch_candidate",
                tuning_round=tuning_round,
                candidate_name=candidate_name,
                selected=bool(selected_run_id and run_id == selected_run_id),
                score=(score_by_run_id or {}).get(run_id),
                budget_elapsed_seconds_value=budget_elapsed_seconds_value,
            )
        )
    return records


def target_failure_summary(summary: dict[str, Any], target_temp_c: int) -> list[dict[str, Any]]:
    failures = validation_failures_for_target(summary, target_temp_c)
    if failures:
        return failures
    stage = stage_for_target(summary, target_temp_c)
    if stage.get("stopReason") != "completed":
        return [{"targetTempC": target_temp_c, "reason": stage.get("stopReason")}]
    return []


def review_target_entry(
    *,
    target_temp_c: int,
    run_id: str,
    budget_outcome: str,
    time_spent_seconds: int,
    approach_reference: dict[str, Any],
    rounds: list[dict[str, Any]],
    final_summary: dict[str, Any],
    accepted_profile: dict[str, Any],
) -> dict[str, Any]:
    stage = stage_for_target(final_summary, target_temp_c)
    failures = target_failure_summary(final_summary, target_temp_c)
    truth_point = sanitize_point(explicit_point(accepted_profile, target_temp_c), target_temp_c)
    effective_point = truth_point or effective_point_from_samples(
        samples_for_target(final_summary, target_temp_c), target_temp_c
    )
    return {
        "runId": run_id,
        "target": int(target_temp_c),
        "targetTempC": int(target_temp_c),
        "ok": budget_outcome == "completed",
        "saved": False,
        "evidence": "preliminary_review",
        "budgetOutcome": budget_outcome,
        "timeSpentSeconds": int(time_spent_seconds),
        "roundCount": len(rounds),
        "validTestCount": sum(1 for item in rounds if item.get("evidenceValid") is not False),
        "invalidTestCount": sum(1 for item in rounds if item.get("evidenceValid") is False),
        "approachReference": dict(approach_reference),
        "point": effective_point,
        "truthPoint": truth_point,
        "pointSource": "review_candidate_snapshot" if truth_point is not None else "sample_parameters",
        "rounds": rounds,
        "result": stage,
        "failures": failures,
        "samples": samples_for_target(final_summary, target_temp_c),
    }


def default_preliminary_bundle_dir() -> Path:
    return REPO_ROOT / f"thermal-self-test-runs/preliminary-pd100w-pps5a-60-140-220-{today_slug()}"


def write_preliminary_review_bundle(
    *,
    bundle_dir: Path,
    accepted_profile: dict[str, Any],
    entries: list[dict[str, Any]],
    source_id: str,
    device_id: str,
    port_path: str,
    tuning_budget_seconds: int,
) -> dict[str, Any]:
    bundle_dir.mkdir(parents=True, exist_ok=True)
    samples_path = bundle_dir / "samples.ndjson"
    sample_lines: list[str] = []
    for entry in entries:
        attempts = entry.get("rounds") if isinstance(entry.get("rounds"), list) else []
        if attempts:
            for attempt in attempts:
                if not isinstance(attempt, dict) or attempt.get("evidenceValid") is False:
                    continue
                for sample in attempt.get("samples") or []:
                    enriched = dict(sample)
                    enriched.update(
                        {
                            "targetTempC": int(entry["target"]),
                            "attemptNumber": attempt.get("round"),
                            "attemptType": attempt.get("attemptType"),
                            "candidateName": attempt.get("candidateName"),
                            "selected": bool(attempt.get("selected")),
                        }
                    )
                    sample_lines.append(json.dumps(enriched, ensure_ascii=False) + "\n")
            continue
        for sample in entry.get("samples") or []:
            enriched = dict(sample)
            enriched["targetTempC"] = int(entry["target"])
            sample_lines.append(json.dumps(enriched, ensure_ascii=False) + "\n")
    samples_path.write_text("".join(sample_lines), encoding="utf-8")
    write_json(bundle_dir / "thermal-profile.accepted.json", accepted_profile)
    bundle = {
        "kind": "thermal_self_test_preliminary_bundle",
        "canonicalReportFormat": "html_bundle",
        "bundleDisposition": "preliminary_review",
        "acceptedProfileRole": "review_candidate_snapshot",
        "generatedAt": now_iso(),
        "selectedMode": PROFILE_MODE,
        "resolvedBank": EXPECTED_BANK,
        "detectedSourceClass": EXPECTED_SOURCE_CLASS,
        "tuningBudgetSeconds": int(tuning_budget_seconds),
        "flagshipTargetsC": [entry["target"] for entry in entries],
        "sourcePreset": "21V / 5.0A",
        "provider": "IsolaPurr",
        "sourceDeviceId": source_id,
        "deviceId": device_id,
        "port": port_path,
        "targets": entries,
        "runs": entries,
        "files": {
            "bundleDir": repo_display_path(bundle_dir),
            "indexHtml": repo_display_path(bundle_dir / "index.html"),
            "bundleJson": repo_display_path(bundle_dir / "run.bundle.json"),
            "samplesPath": repo_display_path(samples_path),
            "acceptedProfilePath": repo_display_path(bundle_dir / "thermal-profile.accepted.json"),
        },
    }
    write_json(bundle_dir / "run.bundle.json", bundle)
    html_data = {
        "generatedAt": bundle["generatedAt"],
        "title": "Flux Purr 100W / pps5a 旗舰三点 preliminary review",
        "subtitle": "当前只收口 60 / 140 / 220°C 三个旗舰目标。full-speed-to-stable 按目标温度使用动态门槛：≤150°C 为 10s，>150°C 为 5s；轮次详情展示真实调参轮次、预算结果与 hold confirm。",
        "bundleDisposition": bundle["bundleDisposition"],
        "acceptedProfileRole": bundle["acceptedProfileRole"],
        "selectedMode": bundle["selectedMode"],
        "resolvedBank": bundle["resolvedBank"],
        "detectedSourceClass": bundle["detectedSourceClass"],
        "sourcePreset": bundle["sourcePreset"],
        "provider": bundle["provider"],
        "sourceDeviceId": bundle["sourceDeviceId"],
        "deviceId": bundle["deviceId"],
        "port": bundle["port"],
        "tuningBudgetSeconds": bundle["tuningBudgetSeconds"],
        "runs": entries,
        "history": build_history(entries),
    }
    (bundle_dir / "index.html").write_text(render_baseline_html(html_data), encoding="utf-8")
    return bundle


def synthetic_failure_summary(target_temp_c: int, reason: str) -> dict[str, Any]:
    return {
        "kind": "thermal_self_test",
        "runId": f"synthetic-{target_temp_c}-{reason}",
        "source": {
            "selectedMode": PROFILE_MODE,
            "resolvedBank": EXPECTED_BANK,
            "detectedSourceClass": EXPECTED_SOURCE_CLASS,
        },
        "parameters": {
            "targetsC": [target_temp_c],
            "evaluationMode": EVALUATION_MODE_TUNING_SCOUT,
        },
        "files": {
            "summaryPath": str(REPO_ROOT / f"thermal-self-test-runs/synthetic-{target_temp_c}-{reason}.json"),
            "samplesPath": str(REPO_ROOT / f"thermal-self-test-runs/synthetic-{target_temp_c}-{reason}.ndjson"),
        },
        "applied": [
            {
                "targetTempC": int(target_temp_c),
                "stopReason": reason,
                "maxOvershootC": None,
                "holdPeakToPeakC": None,
                "analysis": {},
                "fullSpeedToStable": {},
                "guard": {},
            }
        ],
        "validation": {
            "passed": False,
            "expectedTargetsC": [int(target_temp_c)],
            "failures": [{"targetTempC": int(target_temp_c), "reason": reason}],
        },
        "tuningSteps": [],
    }


def run_budgeted_self_test(
    runner: FluxPurrRunner,
    *,
    seed_profile_file: Path | None = None,
    candidate_profile_files: list[Path] | None = None,
    targets_c: list[int],
    hold_seconds: int,
    output_dir: Path,
    evaluation_mode: str,
    cooldown_temp_c: float,
    budget_started_at: float,
    budget_seconds: int,
) -> SelfTestRun:
    remaining = budget_remaining_seconds(budget_started_at, budget_seconds)
    timeouts = step_timeouts_for_budget(remaining, hold_seconds)
    if timeouts is None:
        raise RuntimeError("target_budget_exhausted")
    cooldown_timeout_seconds, stage_timeout_seconds = timeouts
    return runner.self_test(
        seed_profile_file=seed_profile_file,
        candidate_profile_files=candidate_profile_files,
        targets_c=targets_c,
        hold_seconds=hold_seconds,
        output_dir=output_dir,
        evaluation_mode=evaluation_mode,
        cooldown_temp_c=cooldown_temp_c,
        stage_timeout_seconds=stage_timeout_seconds,
        cooldown_timeout_seconds=cooldown_timeout_seconds,
    )


def tune_flagship_target(
    runner: FluxPurrRunner,
    current_profile: dict[str, Any],
    target_temp_c: int,
    anchors_c: list[int],
    workspace_dir: Path,
    *,
    per_target_budget_seconds: int,
    max_tuning_rounds: int,
    scout_hold_seconds: int,
    confirm_hold_seconds: int,
) -> tuple[dict[str, Any], dict[str, Any]]:
    workspace_dir.mkdir(parents=True, exist_ok=True)
    budget_started_at = time.monotonic()
    cooldown_temp = cooldown_threshold(target_temp_c)
    updated_profile = copy.deepcopy(current_profile)
    reference = {
        "targetTempC": int(target_temp_c),
        "variantId": "full_speed_to_stable_gate",
        "passed": True,
        "limitMs": 5_000 if int(target_temp_c) > 150 else 10_000,
        "failureReason": None,
    }

    rounds: list[dict[str, Any]] = []
    last_summary = synthetic_failure_summary(target_temp_c, "no_round_completed")
    budget_outcome = "not_converged"

    for round_index in range(max_tuning_rounds):
        if budget_exhausted(budget_started_at, per_target_budget_seconds):
            break
        round_dir = workspace_dir / f"round-{round_index + 1}"
        round_seed = round_dir / "current-sparse.json"
        write_json(round_seed, updated_profile)
        try:
            scout = run_budgeted_self_test(
                runner,
                seed_profile_file=round_seed,
                targets_c=[target_temp_c],
                hold_seconds=scout_hold_seconds,
                output_dir=round_dir / "scout",
                evaluation_mode=EVALUATION_MODE_TUNING_SCOUT,
                cooldown_temp_c=cooldown_temp,
                budget_started_at=budget_started_at,
                budget_seconds=per_target_budget_seconds,
            )
            ensure_expected_source(scout.summary)
            last_summary = scout.summary
            rounds.append(
                round_record_from_summary(
                    scout.summary,
                    target_temp_c,
                    len(rounds) + 1,
                    f"tuning {round_index + 1} / scout",
                    explicit_point(updated_profile, target_temp_c),
                    attempt_type="scout",
                    tuning_round=round_index + 1,
                    selected=False,
                    budget_elapsed_seconds_value=budget_elapsed_seconds(budget_started_at),
                )
            )
            if run_is_disqualified(scout.summary, target_temp_c):
                budget_outcome = "environment_blocked"
                break

            retuned_profile_raw, retuned_candidate_path = runner.retune(scout.run_dir, target_temp_c)
            retuned_profile = normalize_sparse_profile(
                runner,
                retuned_profile_raw,
                anchors_c,
                round_dir / "materialized",
                f"normalize-retune-{target_temp_c}-{round_index + 1}",
            )
            scout_stage = stage_for_target(scout.summary, target_temp_c)
            variants = build_candidate_variants(
                updated_profile,
                retuned_profile,
                target_temp_c,
                scout_stage,
                samples_for_target(scout.summary, target_temp_c),
            )
            candidates_dir = round_dir / "candidates"
            candidate_paths: list[Path] = []
            for variant in variants:
                variant_path = candidates_dir / f"{variant.name}.json"
                write_json(variant_path, variant.profile)
                variant.path = variant_path
                candidate_paths.append(variant_path)

            batch = run_budgeted_self_test(
                runner,
                candidate_profile_files=candidate_paths,
                targets_c=[target_temp_c],
                hold_seconds=scout_hold_seconds,
                output_dir=round_dir / "batch",
                evaluation_mode=EVALUATION_MODE_TUNING_SCOUT,
                cooldown_temp_c=cooldown_temp,
                budget_started_at=budget_started_at,
                budget_seconds=per_target_budget_seconds,
            )
            best = choose_best_batch_run(batch.summary, target_temp_c)
            selected_run_id = str(best["summary"].get("runId") or "")
            score_by_run_id = {
                str(run.get("runId") or ""): list(candidate_score(run, target_temp_c))
                for run in batch.summary.get("runs") or []
                if isinstance(run, dict)
            }
            rounds.extend(
                batch_attempt_records(
                    batch.summary,
                    target_temp_c,
                    first_round_number=len(rounds) + 1,
                    tuning_round=round_index + 1,
                    selected_run_id=selected_run_id,
                    score_by_run_id=score_by_run_id,
                    budget_elapsed_seconds_value=budget_elapsed_seconds(budget_started_at),
                )
            )
            chosen_profile = read_json(best["candidateProfileFile"])
            updated_profile = normalize_sparse_profile(
                runner,
                chosen_profile,
                anchors_c,
                round_dir / "materialized",
                f"normalize-best-{target_temp_c}-{round_index + 1}",
            )
            write_json(round_dir / "accepted-sparse-profile.json", updated_profile)
            chosen_summary = best["summary"]
            last_summary = chosen_summary
            if stage_reference_gate_satisfied(chosen_summary, target_temp_c):
                break
        except Exception as exc:
            budget_outcome = (
                "budget_exhausted" if "target_budget_exhausted" in str(exc) else "environment_blocked"
            )
            last_summary = synthetic_failure_summary(
                target_temp_c,
                "target_budget_exhausted" if budget_outcome == "budget_exhausted" else "round_execution_failed",
            )
            break

    if budget_outcome != "environment_blocked":
        for confirm_attempt in range(2):
            if budget_exhausted(budget_started_at, per_target_budget_seconds):
                budget_outcome = "budget_exhausted"
                break
            hold_seed = workspace_dir / f"hold-confirm-{confirm_attempt + 1}-seed.json"
            write_json(hold_seed, updated_profile)
            try:
                confirm = run_budgeted_self_test(
                    runner,
                    seed_profile_file=hold_seed,
                    targets_c=[target_temp_c],
                    hold_seconds=confirm_hold_seconds,
                    output_dir=workspace_dir / f"hold-confirm-{confirm_attempt + 1}",
                    evaluation_mode=EVALUATION_MODE_HOLD_CONFIRM,
                    cooldown_temp_c=cooldown_temp,
                    budget_started_at=budget_started_at,
                    budget_seconds=per_target_budget_seconds,
                )
                ensure_expected_source(confirm.summary)
                last_summary = confirm.summary
                rounds.append(
                    round_record_from_summary(
                        confirm.summary,
                        target_temp_c,
                        len(rounds) + 1,
                        f"hold confirm {confirm_attempt + 1}",
                        explicit_point(updated_profile, target_temp_c),
                        attempt_type="hold_confirm",
                        selected=True,
                        budget_elapsed_seconds_value=budget_elapsed_seconds(budget_started_at),
                    )
                )
                if confirm.summary.get("validation", {}).get("passed") is True:
                    budget_outcome = "completed"
                    break
                if run_is_disqualified(confirm.summary, target_temp_c):
                    budget_outcome = "environment_blocked"
                    break
                budget_outcome = "not_converged"
                if confirm_attempt > 0:
                    break

                confirm_stage = stage_for_target(confirm.summary, target_temp_c)
                evidence = stability_evidence_for_stage(
                    confirm_stage,
                    samples_for_target(confirm.summary, target_temp_c),
                    target_temp_c,
                )
                current_point = explicit_point(updated_profile, target_temp_c)
                if current_point is None:
                    break
                predicted_point = predict_next_point(current_point, evidence)
                if predicted_point == current_point:
                    break
                updated_profile = merge_point(updated_profile, predicted_point)
                recovery_seed = workspace_dir / "confirm-recovery-scout-seed.json"
                write_json(recovery_seed, updated_profile)
                recovery = run_budgeted_self_test(
                    runner,
                    seed_profile_file=recovery_seed,
                    targets_c=[target_temp_c],
                    hold_seconds=scout_hold_seconds,
                    output_dir=workspace_dir / "confirm-recovery-scout",
                    evaluation_mode=EVALUATION_MODE_TUNING_SCOUT,
                    cooldown_temp_c=cooldown_temp,
                    budget_started_at=budget_started_at,
                    budget_seconds=per_target_budget_seconds,
                )
                ensure_expected_source(recovery.summary)
                last_summary = recovery.summary
                rounds.append(
                    round_record_from_summary(
                        recovery.summary,
                        target_temp_c,
                        len(rounds) + 1,
                        "confirm recovery / predicted",
                        predicted_point,
                        attempt_type="confirm_recovery_scout",
                        candidate_name=str(evidence.get("failureClass") or "predicted"),
                        selected=True,
                        budget_elapsed_seconds_value=budget_elapsed_seconds(budget_started_at),
                    )
                )
                if run_is_disqualified(recovery.summary, target_temp_c):
                    budget_outcome = "environment_blocked"
                    break
                if not stage_reference_gate_satisfied(recovery.summary, target_temp_c):
                    budget_outcome = "not_converged"
                    break
            except Exception as exc:
                budget_outcome = (
                    "budget_exhausted" if "target_budget_exhausted" in str(exc) else "environment_blocked"
                )
                last_summary = synthetic_failure_summary(
                    target_temp_c,
                    "target_budget_exhausted" if budget_outcome == "budget_exhausted" else "hold_confirm_failed",
                )
                break

    entry = review_target_entry(
        target_temp_c=target_temp_c,
        run_id=str(last_summary.get("runId") or f"target-{target_temp_c}"),
        budget_outcome=budget_outcome,
        time_spent_seconds=budget_elapsed_seconds(budget_started_at),
        approach_reference=reference,
        rounds=rounds,
        final_summary=last_summary,
        accepted_profile=updated_profile,
    )
    return updated_profile, entry


def tune_anchor_target(
    runner: FluxPurrRunner,
    current_profile: dict[str, Any],
    target_temp_c: int,
    anchors_c: list[int],
    workspace_dir: Path,
) -> tuple[dict[str, Any], dict[str, Any]]:
    workspace_dir.mkdir(parents=True, exist_ok=True)
    current_path = workspace_dir / "current-sparse.json"
    write_json(current_path, current_profile)

    scout = runner.self_test(
        seed_profile_file=current_path,
        targets_c=[target_temp_c],
        hold_seconds=12,
        output_dir=workspace_dir / "scout",
    )
    ensure_expected_source(scout.summary)
    scout_stage = stage_for_target(scout.summary, target_temp_c)
    retuned_profile_raw, retuned_candidate_path = runner.retune(scout.run_dir, target_temp_c)
    retuned_profile = normalize_sparse_profile(
        runner,
        retuned_profile_raw,
        anchors_c,
        workspace_dir / "materialized",
        f"normalize-retune-{target_temp_c}",
    )
    variants = build_candidate_variants(
        current_profile,
        retuned_profile,
        target_temp_c,
        scout_stage,
        samples_for_target(scout.summary, target_temp_c),
    )
    candidates_dir = workspace_dir / "candidates"
    candidate_paths: list[Path] = []
    for variant in variants:
        variant_path = candidates_dir / f"{variant.name}.json"
        write_json(variant_path, variant.profile)
        variant.path = variant_path
        candidate_paths.append(variant_path)

    batch = runner.self_test(
        candidate_profile_files=candidate_paths,
        targets_c=[target_temp_c],
        hold_seconds=12,
        output_dir=workspace_dir / "batch",
    )
    if not (batch.summary.get("runs") or []):
        log(
            "thermal batch produced no candidate runs"
            f" for {target_temp_c}°C ({batch.summary.get('error')}); retrying once"
        )
        batch = runner.self_test(
            candidate_profile_files=candidate_paths,
            targets_c=[target_temp_c],
            hold_seconds=12,
            output_dir=workspace_dir / "batch-rerun1",
        )
    best = choose_best_batch_run(batch.summary, target_temp_c)
    best_profile_path = Path(best["candidateProfileFile"])
    chosen_profile = read_json(best_profile_path)
    normalized = normalize_sparse_profile(
        runner,
        chosen_profile,
        anchors_c,
        workspace_dir / "materialized",
        f"normalize-best-{target_temp_c}",
    )
    return normalized, {
        "targetTempC": target_temp_c,
        "scoutRun": repo_display_path(scout.summary_path),
        "retunedCandidate": repo_display_path(retuned_candidate_path),
        "batchSummary": repo_display_path(batch.summary_path),
        "chosenCandidate": repo_display_path(best_profile_path),
        "chosenScore": list(best["score"]),
    }


def failed_validation_targets(summary: dict[str, Any]) -> list[int]:
    validation = summary.get("validation")
    raw_failures = validation.get("failures") if isinstance(validation, dict) else []
    targets: list[int] = []
    for failure in raw_failures or []:
        if isinstance(failure, dict) and "targetTempC" in failure:
            target_temp_c = int(failure["targetTempC"])
            if target_temp_c not in targets:
                targets.append(target_temp_c)
    return targets


def remedial_anchor_targets(failed_targets: list[int]) -> list[int]:
    remedies: list[int] = []
    for target in failed_targets:
        if target in DEFAULT_ANCHOR_TARGETS and target not in remedies:
            remedies.append(target)
    if any(target in failed_targets for target in (80, 120)) and 100 not in remedies:
        remedies.append(100)
    if 160 in failed_targets and 180 not in remedies:
        remedies.append(180)
    return remedies


def freeze_baseline_bundle(
    summary: dict[str, Any],
    accepted_profile: dict[str, Any],
    port_path: str,
    bundle_dir: Path,
) -> None:
    bundle_dir.mkdir(parents=True, exist_ok=True)
    samples_path = Path(summary["files"]["samplesPath"])
    grouped = grouped_samples(samples_path)
    entries = [
        target_run_entry(summary, accepted_profile, int(stage["targetTempC"]), grouped.get(int(stage["targetTempC"]), []))
        for stage in summary.get("applied") or []
        if isinstance(stage, dict)
    ]
    entries.sort(key=lambda entry: entry["target"])
    source_runs_dir = bundle_dir / "source-run-summaries"
    source_runs_dir.mkdir(parents=True, exist_ok=True)
    source_run_paths: dict[int, Path] = {}
    for entry in entries:
        target_temp_c = entry["target"]
        target_summary_path = source_runs_dir / f"{target_temp_c}.run.json"
        write_json(target_summary_path, source_run_summary(summary, target_temp_c))
        source_run_paths[target_temp_c] = target_summary_path

    write_json(bundle_dir / "thermal-profile.accepted.json", accepted_profile)
    bundle_json = build_baseline_bundle_json(summary, bundle_dir, source_run_paths)
    write_json(bundle_dir / "run.bundle.json", bundle_json)
    (bundle_dir / "samples.ndjson").write_text(samples_path.read_text(encoding="utf-8"), encoding="utf-8")
    html = render_baseline_html(build_html_data(summary, accepted_profile, entries, port_path))
    (bundle_dir / "index.html").write_text(html, encoding="utf-8")


def default_output_root() -> Path:
    return REPO_ROOT / f"thermal-self-test-runs/flagship-pps5a-sprint-{now_slug()}"


def build_plan_payload(
    *,
    source_id: str,
    source_url: str,
    authorized_port: str,
    output_root: Path,
    initial_sparse_profile: Path,
    bundle_dir: Path,
    anchors_c: list[int],
    validation_targets_c: list[int],
    tune_targets_c: list[int],
    per_target_budget_seconds: int,
    max_tuning_rounds: int,
    scout_hold_seconds: int,
    confirm_hold_seconds: int,
    dry_run: bool,
) -> dict[str, Any]:
    return {
        "kind": "flagship_thermal_test_plan",
        "generatedAt": now_iso(),
        "mode": "dry_run" if dry_run else "real_hil",
        "scope": {
            "profileMode": PROFILE_MODE,
            "resolvedBank": EXPECTED_BANK,
            "detectedSourceClass": EXPECTED_SOURCE_CLASS,
            "flagshipTargetsC": tune_targets_c,
            "executionOrder": tune_targets_c,
            "perTargetBudgetSeconds": per_target_budget_seconds,
        },
        "deviceConnection": {
            "fluxPurr": {
                "authorizedPort": authorized_port,
                "devdUrl": "repo-local explicit bind required",
                "portSwitchPolicy": "stop_if_missing_or_reenumerated",
            },
            "isolaPurr": {
                "sourceId": source_id,
                "sourceUrl": source_url,
                "usbCPathRole": "power-cycle only for the heater power path",
                "requiredConfig": {
                    "powerWatts": EXPECTED_SOURCE_POWER_WATTS,
                    "pdEnabled": True,
                    "ppsEnabled": True,
                    "pps5aEnabled": True,
                    "ppsCurrentLimitMa": EXPECTED_SOURCE_PPS_LIMIT_MA,
                    "tpsMode": "auto_follow",
                },
            },
        },
        "executionWhitelist": {
            "preflight": [
                "Run existing host unit tests for tuning/report/dynamic gate logic.",
                "Start repo-local flux-purr-devd with the exact authorized serial port only.",
                "Query Flux Purr identity/status and confirm heater is off with continuous temperature updates.",
                "Query IsolaPurr power status and confirm 100W + PPS 5A + auto_follow with advancing telemetry.",
                "Confirm selectedMode=100w, resolvedBank=pps5a, detectedSourceClass=pps5a before heating.",
            ],
            "perTargetWorkflow": [
                f"Wait for cooldown gate, then run one {scout_hold_seconds}s tuning scout.",
                "Classify the failure, then compare the current point with one evidence-specific predicted point; add a hold-ripple point only when hold p2p is over limit.",
                f"Allow at most {max_tuning_rounds} targeted tuning rounds while the per-target budget remains.",
                f"Run one {confirm_hold_seconds}s hold confirm; after a target-local thermal failure, allow one predicted short scout and one final confirm while budget remains.",
                "Modify only the current target profile point; keep warmupPowerPermille fixed at 1000 and require 100% warmup output.",
            ],
            "dynamicApproachGate": [
                "60°C and 140°C: warmup exit to stable-window start must be <= 10s.",
                "220°C: warmup exit to stable-window start must be <= 5s.",
                "Stable window means 10s continuous sampling at >=3Hz with abs(temp-target) <= 1.5°C.",
            ],
            "allowedResults": ["completed", "not_converged", "budget_exhausted", "environment_blocked"],
        },
        "powerCycleRecovery": {
            "onlyWhen": [
                "source telemetry stale for more than 2s",
                "source output is stuck at low voltage",
                "temperature sampling shows a clear discontinuity",
                "RTD/ADC fault",
                "runtime reset or hardware stops responding",
            ],
            "steps": [
                "Stop heating immediately and mark the current attempt invalid.",
                f"Run `isolapurr power output manual --url {source_url} --usb-c-path disconnected --json`.",
                f"Run `isolapurr power show --url {source_url} --json` and confirm usb_c_power_enabled=false, then wait 2s.",
                f"Run `isolapurr power output auto --url {source_url} --json`.",
                f"Run `isolapurr power show --url {source_url} --json` until telemetry advances and confirm output enabled, auto_follow, 100W, PPS, PPS 5A, and 5000mA capability readback.",
                f"Confirm the exact authorized port `{authorized_port}` still exists before retrying the failed substep.",
                "Retry the same failed substep once; if the environment fails again, stop that target as environment_blocked.",
            ],
            "budgetPolicy": "Recovery time counts toward the same per-target 20-minute budget.",
        },
        "acceptance": [
            "Pass the target-dependent full-speed-to-stable gate.",
            "maxOvershootC <= 3.0",
            "60s holdPeakToPeakC <= 3.0",
            "No runtime reset, heater disarm, source fault, measurement fault, mode mismatch, or target mismatch.",
            "Source voltage/current/power telemetry must be recorded alongside request voltage and remain continuous.",
        ],
        "forbiddenOperations": [
            "Do not test 80 / 100 / 120 / 160 / 180 / 240 / 250°C in this sprint.",
            "Do not run the full temperature ladder.",
            "Do not run default 0% / 25% / 50% approach-only characterization.",
            "Do not modify the core control algorithm, temperature filtering path, or warmup power.",
            "Do not flash firmware, reset the MCU, change selector, or switch to another serial port.",
            "Do not save the pps5a EEPROM bank or freeze a final accepted baseline.",
            "Do not restart from 60°C after a later target fails; only retry the same failed substep.",
        ],
        "artifacts": {
            "outputRoot": repo_display_path(output_root),
            "initialSparseProfile": repo_display_path(initial_sparse_profile),
            "preliminaryBundleDir": repo_display_path(bundle_dir),
            "canonicalBundleFiles": [
                "index.html",
                "run.bundle.json",
                "samples.ndjson",
                "thermal-profile.accepted.json",
            ],
            "anchorTargetsC": anchors_c,
            "validationTargetsC": validation_targets_c,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run the 100W / pps5a flagship thermal tuning sprint with target-dependent full-speed gates and per-target budgets")
    parser.add_argument("--flux-purr-bin", type=Path, default=REPO_ROOT / "target/debug/flux-purr")
    parser.add_argument("--devd-url", default="http://127.0.0.1:62610")
    parser.add_argument("--authorized-port", default=DEFAULT_AUTHORIZED_PORT)
    parser.add_argument("--source-id", required=True)
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--output-root", type=Path, default=default_output_root())
    parser.add_argument("--preliminary-profile-file", type=Path, default=PRELIMINARY_PROFILE)
    parser.add_argument("--fallback-profile-file", type=Path, default=FALLBACK_PROFILE)
    parser.add_argument("--bundle-dir", type=Path, default=default_preliminary_bundle_dir())
    parser.add_argument("--anchor-targets-c")
    parser.add_argument("--validation-targets-c")
    parser.add_argument("--tune-targets-c")
    parser.add_argument("--per-target-budget-seconds", type=int, default=DEFAULT_PER_TARGET_BUDGET_SECONDS)
    parser.add_argument("--max-tuning-rounds", type=int, default=DEFAULT_MAX_TUNING_ROUNDS)
    parser.add_argument("--scout-hold-seconds", type=int, default=DEFAULT_SCOUT_HOLD_SECONDS)
    parser.add_argument("--confirm-hold-seconds", type=int, default=DEFAULT_CONFIRM_HOLD_SECONDS)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--plan-only", action="store_true")
    parser.add_argument("--no-source-recovery", action="store_true")
    args = parser.parse_args()

    if not args.flux_purr_bin.exists():
        raise RuntimeError(f"missing flux-purr binary: {args.flux_purr_bin}")

    output_root = args.output_root if args.output_root.is_absolute() else REPO_ROOT / args.output_root
    output_root.mkdir(parents=True, exist_ok=True)
    anchors_c = parse_targets(args.anchor_targets_c, DEFAULT_ANCHOR_TARGETS)
    validation_targets_c = parse_targets(args.validation_targets_c, DEFAULT_VALIDATION_TARGETS)
    tune_targets_c = parse_targets(args.tune_targets_c, DEFAULT_TUNE_TARGETS)

    runner = FluxPurrRunner(
        flux_purr_bin=args.flux_purr_bin,
        devd_url=args.devd_url,
        authorized_port=args.authorized_port,
        source_id=args.source_id,
        source_url=args.source_url,
        dry_run=args.dry_run,
        auto_recover_source=not args.no_source_recovery,
    )

    preliminary_profile = read_json(args.preliminary_profile_file)
    fallback_profile = read_json(args.fallback_profile_file)
    current_profile = build_initial_sparse_seed(
        runner,
        preliminary_profile,
        fallback_profile,
        anchors_c,
        output_root / "seed",
    )
    write_json(output_root / "seed" / "initial-sparse-profile.json", current_profile)

    if args.plan_only:
        planned = build_plan_payload(
            source_id=args.source_id,
            source_url=args.source_url,
            authorized_port=args.authorized_port,
            output_root=output_root,
            initial_sparse_profile=output_root / "seed" / "initial-sparse-profile.json",
            bundle_dir=args.bundle_dir if args.bundle_dir.is_absolute() else REPO_ROOT / args.bundle_dir,
            anchors_c=anchors_c,
            validation_targets_c=validation_targets_c,
            tune_targets_c=tune_targets_c,
            per_target_budget_seconds=args.per_target_budget_seconds,
            max_tuning_rounds=args.max_tuning_rounds,
            scout_hold_seconds=args.scout_hold_seconds,
            confirm_hold_seconds=args.confirm_hold_seconds,
            dry_run=args.dry_run,
        )
        print(json.dumps(planned, ensure_ascii=False, indent=2))
        return 0

    review_entries: list[dict[str, Any]] = []
    for target_temp_c in tune_targets_c:
        current_profile, entry = tune_flagship_target(
            runner,
            current_profile,
            target_temp_c,
            anchors_c,
            output_root / f"target-{target_temp_c}",
            per_target_budget_seconds=args.per_target_budget_seconds,
            max_tuning_rounds=args.max_tuning_rounds,
            scout_hold_seconds=args.scout_hold_seconds,
            confirm_hold_seconds=args.confirm_hold_seconds,
        )
        write_json(output_root / f"target-{target_temp_c}" / "review-entry.json", entry)
        write_json(output_root / f"target-{target_temp_c}" / "accepted-sparse-profile.json", current_profile)
        review_entries.append(entry)
        write_json(output_root / "review-entries.json", review_entries)

    accepted_profile_path = output_root / "review-candidate-profile.json"
    write_json(accepted_profile_path, current_profile)
    bundle_dir = args.bundle_dir if args.bundle_dir.is_absolute() else REPO_ROOT / args.bundle_dir
    bundle = write_preliminary_review_bundle(
        bundle_dir=bundle_dir,
        accepted_profile=current_profile,
        entries=review_entries,
        source_id=args.source_id,
        device_id=runner.resolve_device_id(args.dry_run),
        port_path=args.authorized_port,
        tuning_budget_seconds=args.per_target_budget_seconds,
    )

    result = {
        "ok": True,
        "generatedAt": now_iso(),
        "mode": "dry_run" if args.dry_run else "real_hil",
        "anchorsC": anchors_c,
        "validationTargetsC": validation_targets_c,
        "tuneTargetsC": tune_targets_c,
        "perTargetBudgetSeconds": args.per_target_budget_seconds,
        "maxTuningRounds": args.max_tuning_rounds,
        "scoutHoldSeconds": args.scout_hold_seconds,
        "confirmHoldSeconds": args.confirm_hold_seconds,
        "acceptedProfilePath": repo_display_path(accepted_profile_path),
        "bundleDir": repo_display_path(bundle_dir),
        "bundleJson": bundle["files"]["bundleJson"],
        "bundleIndexHtml": bundle["files"]["indexHtml"],
        "reviewOutcomes": {
            str(entry["target"]): entry["budgetOutcome"]
            for entry in review_entries
        },
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
