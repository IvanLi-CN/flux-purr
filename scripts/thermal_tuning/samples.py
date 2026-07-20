from .config import *
from .analysis import stage_for_target, stability_evidence_for_stage, validation_failures_for_target
from .profile import explicit_point, repo_display_path


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


def sort_samples_by_time(samples: list[dict[str, Any]]) -> list[dict[str, Any]]:
    def sample_key(item: tuple[int, dict[str, Any]]) -> tuple[int, float | int, int]:
        index, sample = item
        try:
            t = float(sample.get("t"))
        except (TypeError, ValueError):
            return (1, index, index)
        if not math.isfinite(t):
            return (1, index, index)
        return (0, t, index)

    return [sample for _, sample in sorted(enumerate(samples), key=sample_key)]


def normalized_sorted_samples(samples: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return sort_samples_by_time([normalized_sample(sample) for sample in samples if isinstance(sample, dict)])


def split_samples_on_time_reset(samples: list[dict[str, Any]]) -> list[list[dict[str, Any]]]:
    segments: list[list[dict[str, Any]]] = []
    current: list[dict[str, Any]] = []
    last_t: float | None = None
    for sample in samples:
        if not isinstance(sample, dict):
            continue
        raw_t = sample.get("elapsedMs", sample.get("t"))
        try:
            current_t = float(raw_t)
        except (TypeError, ValueError):
            current_t = None
        if current and current_t is not None and last_t is not None and current_t < last_t:
            segments.append(current)
            current = []
        current.append(sample)
        if current_t is not None:
            last_t = current_t
    if current:
        segments.append(current)
    return segments


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
    return sort_samples_by_time(normalized)


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
    return normalized_sorted_samples(grouped.get(int(target_temp_c), []))


def warmup_output_is_full(summary: dict[str, Any], target_temp_c: int) -> bool:
    warmup_outputs = [
        sample.get("output")
        for sample in samples_for_target(summary, target_temp_c)
        if sample.get("phase") == "warmup" and isinstance(sample.get("output"), (int, float))
    ]
    first_full_index = next(
        (index for index, output in enumerate(warmup_outputs) if float(output) >= 99.5),
        None,
    )
    return first_full_index is not None and all(
        float(output) >= 99.5 for output in warmup_outputs[first_full_index:]
    )


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
        "samples": normalized_sorted_samples(samples),
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
