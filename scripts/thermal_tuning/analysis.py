from .config import *


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
    target_temp_c = int(predicted.get("targetTempC") or 0)
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
        if target_temp_c >= 180:
            predicted["approachDampingExponentPermille"] = clamp_int(
                int(predicted["approachDampingExponentPermille"]) + 90,
                0,
                4000,
            )
            predicted["approachLeadTicks"] = clamp_int(int(predicted["approachLeadTicks"]) + 1, 0, 255)
            predicted["holdEntryCentiC"] = clamp_int(
                int(predicted["holdEntryCentiC"]) + max(8, correction_centi_c // 6),
                0,
                5000,
            )
            predicted["holdReheatPowerPermille"] = clamp_int(
                int(predicted["holdReheatPowerPermille"]) - max(20, correction_centi_c // 3),
                0,
                1000,
            )
    elif failure_class in {"missed_lower_band_before_limit", "stable_window_broke_low"}:
        approach_power = int(predicted["approachPowerPermille"])
        approach_floor = int(predicted["approachFloorPowerPermille"])
        if approach_power >= 1000 and (approach_floor >= 1000 or (target_temp_c >= 180 and approach_floor >= 950)):
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
    # Samples remain in the persistence module. Import lazily to keep the
    # analysis/profile dependency direction acyclic during package import.
    from .samples import samples_for_target

    stage = stage_for_target(summary, target_temp_c)
    metrics = stage_metrics(stage)
    evidence = stability_evidence_for_stage(stage, samples_for_target(summary, target_temp_c), target_temp_c)
    failure_class = str(evidence.get("failureClass") or "")
    temperature_gap = float(evidence.get("temperatureGapC") if isinstance(evidence.get("temperatureGapC"), (int, float)) else 0.0)
    first_band_at_ms = evidence.get("firstBandAtMs")
    stability_progress_penalty = {
        "within_gate": 0.0,
        "within_gate_low_margin": 0.25,
        "stable_window_broke_low": 1.0,
        "stable_window_broke_high": 1.0,
        "band_entry_not_observed": 2.0,
        "missed_lower_band_before_limit": 3.0,
        "missed_upper_band_before_limit": 3.0,
    }.get(failure_class, 4.0)
    if isinstance(first_band_at_ms, (int, float)):
        stability_progress_penalty += min(float(first_band_at_ms) / 1_000_000.0, 0.5)
    stability_progress_penalty += min(temperature_gap, 5.0)
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
        1 if failure_class == "within_gate_low_margin" else 0,
        stability_progress_penalty,
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
