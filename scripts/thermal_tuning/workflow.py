from .core import *
from .report import render_baseline_html
from .runner import *


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
        and warmup_output_is_full(summary, target_temp_c)
        and metrics.get("stopReason") == "completed"
        and evidence.get("failureClass") != "within_gate_low_margin"
    )


def choose_promotable_batch_run(batch_summary: dict[str, Any], target_temp_c: int) -> dict[str, Any] | None:
    ensure_batch_source(batch_summary)
    promotable = []
    for run in batch_summary.get("runs") or []:
        if not isinstance(run, dict) or not stage_reference_gate_satisfied(run, target_temp_c):
            continue
        profile_file = value_at_path(run, "parameters", "candidateProfileFile")
        if not isinstance(profile_file, str):
            raise RuntimeError(f"promotable batch run for {target_temp_c}°C is missing candidateProfileFile")
        promotable.append(
            {
                "summary": dict(run),
                "score": candidate_score(run, target_temp_c),
                "candidateProfileFile": Path(profile_file),
            }
        )
    return min(promotable, key=lambda item: item["score"]) if promotable else None


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
            "warmupOutputFull": warmup_output_is_full(summary, target_temp_c),
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


def default_preliminary_bundle_dir(targets_c: list[int] | None = None) -> Path:
    targets = targets_c or list(DEFAULT_TUNE_TARGETS)
    target_slug = "-".join(str(int(target)) for target in targets)
    return REPO_ROOT / f"thermal-self-test-runs/preliminary-pd100w-pps5a-{target_slug}-{today_slug()}"


def write_preliminary_review_bundle(
    *,
    bundle_dir: Path,
    accepted_profile: dict[str, Any],
    entries: list[dict[str, Any]],
    source_id: str,
    device_id: str,
    port_path: str,
    tuning_budget_seconds: int,
    generated_at: str | None = None,
    source_preset: str = "21V / 5.0A",
    provider: str = "IsolaPurr",
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
        "generatedAt": generated_at or now_iso(),
        "selectedMode": PROFILE_MODE,
        "resolvedBank": EXPECTED_BANK,
        "detectedSourceClass": EXPECTED_SOURCE_CLASS,
        "tuningBudgetSeconds": int(tuning_budget_seconds),
        "flagshipTargetsC": [entry["target"] for entry in entries],
        "sourcePreset": source_preset,
        "provider": provider,
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
    target_label = " / ".join(f"{entry['target']}°C" for entry in entries)
    html_data = {
        "generatedAt": bundle["generatedAt"],
        "title": f"Flux Purr 100W / pps5a {target_label} preliminary review",
        "subtitle": f"当前只收口 {target_label}。full-speed-to-stable 按目标温度使用动态门槛：≤150°C 为 10s，>150°C 为 5s；轮次详情展示全部有效调参轮次、预算结果与 hold confirm。",
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


def legacy_preliminary_review_entries(
    legacy_bundle: dict[str, Any],
    grouped_target_samples: dict[int, list[dict[str, Any]]],
) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for target_payload in legacy_bundle.get("targets") or []:
        if not isinstance(target_payload, dict):
            continue
        target_temp_c = int(target_payload["targetTempC"])
        hold_check = target_payload.get("holdCheck") if isinstance(target_payload.get("holdCheck"), dict) else {}
        variants = target_payload.get("variants") if isinstance(target_payload.get("variants"), list) else []
        effective_point = sanitize_point(target_payload.get("effectivePoint"), target_temp_c)
        top_level_samples = [
            normalized_sample(sample)
            for sample in grouped_target_samples.get(target_temp_c, [])
            if isinstance(sample, dict)
        ]
        rounds: list[dict[str, Any]] = []
        selected_round = max(len(variants), 1)
        for index, variant in enumerate(variants, start=1):
            if not isinstance(variant, dict):
                continue
            variant_point = sanitize_point(variant.get("tunedPoint"), target_temp_c)
            variant_samples = []
            for sample in variant.get("samples") or []:
                if not isinstance(sample, dict):
                    continue
                source = sample.get("sourceTelemetry") if isinstance(sample.get("sourceTelemetry"), dict) else {}
                variant_samples.append(
                    {
                        "t": round(float(sample.get("elapsedMs", 0)) / 1000.0, 3),
                        "temp": sample.get("currentTempC"),
                        "filtered": sample.get("heaterFilteredTempC"),
                        "command": sample.get("heaterOutputPercent"),
                        "output": sample.get("heaterPhysicalOutputPercent"),
                        "requestV": None,
                        "phase": sample.get("heaterControlPhase"),
                        "sourceVoltageV": round(float(source["voltageMv"]) / 1000.0, 3)
                        if isinstance(source.get("voltageMv"), (int, float))
                        else None,
                        "sourceCurrentA": round(float(source["currentMa"]) / 1000.0, 3)
                        if isinstance(source.get("currentMa"), (int, float))
                        else None,
                        "sourcePowerW": round(float(source["powerMw"]) / 1000.0, 3)
                        if isinstance(source.get("powerMw"), (int, float))
                        else None,
                    }
                )
            metrics = variant.get("metrics") if isinstance(variant.get("metrics"), dict) else {}
            rounds.append(
                {
                    "round": index,
                    "label": variant.get("variantLabel") or f"variant {index}",
                    "attemptType": "characterization",
                    "candidateName": variant.get("variantId") or f"variant_{index}",
                    "selected": index == selected_round,
                    "evidenceValid": bool(variant.get("valid", True)),
                    "point": variant_point,
                    "samples": variant_samples,
                    "failures": [],
                    "result": {
                        "stopReason": "completed",
                        "maxOvershootC": metrics.get("peak"),
                        "holdPeakToPeakC": metrics.get("rollback"),
                        "settleTimeMs": metrics.get("approachDurationMs"),
                    },
                }
            )
        full_speed_limit_ms = 10_000 if target_temp_c <= 150 else 5_000
        result = {
            "stopReason": hold_check.get("stopReason") or ("completed" if hold_check.get("passed") else hold_check.get("failureReason")),
            "maxOvershootC": hold_check.get("maxOvershootC"),
            "holdPeakToPeakC": hold_check.get("holdPeakToPeakC"),
            "fullSpeedToStable": {
                "limitMs": full_speed_limit_ms,
                "settleTimeMs": None,
                "failureReason": hold_check.get("failureReason"),
            },
            "analysis": {
                "holdMedianOutputPermille": hold_check.get("holdMedianOutputPermille"),
                "holdP90OutputPermille": hold_check.get("holdP90OutputPermille"),
                "approachSource": hold_check.get("approachSource"),
                "holdSource": hold_check.get("holdSource"),
            },
        }
        failures = []
        if hold_check and not bool(hold_check.get("passed")):
            failures.append(
                {
                    "targetTempC": target_temp_c,
                    "reason": hold_check.get("failureReason") or "hold_check_failed",
                }
            )
        entries.append(
            {
                "runId": hold_check.get("confirmRunId") or legacy_bundle.get("runId") or f"legacy-{target_temp_c}",
                "target": target_temp_c,
                "targetTempC": target_temp_c,
                "ok": bool(hold_check.get("passed")),
                "saved": False,
                "evidence": "preliminary_review",
                "budgetOutcome": "completed" if hold_check.get("passed") else "not_converged",
                "timeSpentSeconds": int(round(top_level_samples[-1]["t"])) if top_level_samples else 0,
                "roundCount": len(rounds),
                "validTestCount": sum(1 for round_item in rounds if round_item.get("evidenceValid") is not False),
                "invalidTestCount": sum(1 for round_item in rounds if round_item.get("evidenceValid") is False),
                "approachReference": {"limitMs": full_speed_limit_ms},
                "point": effective_point,
                "truthPoint": effective_point,
                "pointSource": "review_candidate_snapshot",
                "rounds": rounds,
                "result": result,
                "failures": failures,
                "samples": top_level_samples,
                "holdCheck": hold_check,
            }
        )
    return entries


def rerender_legacy_preliminary_review_bundle(
    *,
    legacy_bundle_dir: Path,
    output_dir: Path,
) -> dict[str, Any]:
    legacy_bundle = read_json(legacy_bundle_dir / "run.bundle.json")
    accepted_profile = read_json(legacy_bundle_dir / "thermal-profile.accepted.json")
    target_samples = grouped_samples(legacy_bundle_dir / "samples.ndjson")
    entries = legacy_preliminary_review_entries(legacy_bundle, target_samples)
    source = legacy_bundle.get("source") if isinstance(legacy_bundle.get("source"), dict) else {}
    return write_preliminary_review_bundle(
        bundle_dir=output_dir,
        accepted_profile=accepted_profile,
        entries=entries,
        source_id=str(source.get("sourceDeviceId") or "unknown-source"),
        device_id=str(source.get("deviceId") or "unknown-device"),
        port_path=str(source.get("port") or source.get("portPath") or "/dev/cu.usbmodem2111401"),
        tuning_budget_seconds=0,
        generated_at=legacy_bundle.get("generatedAt"),
        source_preset="21V / 5.0A",
        provider="IsolaPurr",
    )


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
    cooldown_timeout_seconds, stage_timeout_seconds, warmup_timeout_seconds = timeouts
    return runner.self_test(
        seed_profile_file=seed_profile_file,
        candidate_profile_files=candidate_profile_files,
        targets_c=targets_c,
        hold_seconds=hold_seconds,
        output_dir=output_dir,
        evaluation_mode=evaluation_mode,
        cooldown_temp_c=cooldown_temp_c,
        stage_timeout_seconds=stage_timeout_seconds,
        warmup_timeout_seconds=warmup_timeout_seconds,
        cooldown_timeout_seconds=cooldown_timeout_seconds,
    )


def reseed_after_failed_hold_confirm(
    profile: dict[str, Any],
    target_temp_c: int,
    confirm_summary: dict[str, Any],
) -> dict[str, Any]:
    current_point = explicit_point(profile, target_temp_c)
    if current_point is None:
        return profile
    evidence = stability_evidence_for_stage(
        stage_for_target(confirm_summary, target_temp_c),
        samples_for_target(confirm_summary, target_temp_c),
        target_temp_c,
    )
    metrics = stage_metrics(stage_for_target(confirm_summary, target_temp_c))
    predicted_point = predict_next_point(current_point, evidence)
    failure_class = str(evidence.get("failureClass") or "")
    hold_median = metrics.get("holdMedianOutputPermille")
    if (
        int(target_temp_c) >= 180
        and failure_class in {"missed_lower_band_before_limit", "stable_window_broke_low"}
        and isinstance(hold_median, (int, float))
        and float(hold_median) <= 100.0
    ):
        predicted_point = mutate_more_heat(predicted_point, target_temp_c)
    if predicted_point == current_point:
        return profile
    return merge_point(profile, predicted_point)


def tune_flagship_target(
    runner: FluxPurrRunner,
    current_profile: dict[str, Any],
    target_temp_c: int,
    anchors_c: list[int],
    workspace_dir: Path,
    *,
    per_target_budget_seconds: int,
    max_tuning_rounds: int | None,
    scout_hold_seconds: int,
    confirm_hold_seconds: int,
) -> tuple[dict[str, Any], dict[str, Any]]:
    workspace_dir.mkdir(parents=True, exist_ok=True)
    budget_started_at = time.monotonic()
    cooldown_temp = cooldown_threshold(target_temp_c)
    updated_profile = copy.deepcopy(current_profile)
    explicit_round_limit = (
        int(max_tuning_rounds)
        if isinstance(max_tuning_rounds, int) and int(max_tuning_rounds) > 0
        else None
    )
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
    round_index = 0

    try:
        while True:
            if budget_exhausted(budget_started_at, per_target_budget_seconds):
                budget_outcome = "budget_exhausted"
                break
            if explicit_round_limit is not None and round_index >= explicit_round_limit:
                break
            round_index += 1
            round_dir = workspace_dir / f"round-{round_index}"
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
                        f"tuning {round_index} / scout",
                        explicit_point(updated_profile, target_temp_c),
                        attempt_type="scout",
                        tuning_round=round_index,
                        selected=False,
                        budget_elapsed_seconds_value=budget_elapsed_seconds(budget_started_at),
                    )
                )
                if run_is_disqualified(scout.summary, target_temp_c):
                    budget_outcome = "environment_blocked"
                    break
                if not warmup_output_is_full(scout.summary, target_temp_c):
                    budget_outcome = "not_converged"
                    break

                retuned_profile_raw, retuned_candidate_path = runner.retune(scout.run_dir, target_temp_c)
                retuned_profile = normalize_sparse_profile(
                    runner,
                    retuned_profile_raw,
                    anchors_c,
                    round_dir / "materialized",
                    f"normalize-retune-{target_temp_c}-{round_index}",
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
                diagnostic_best = choose_best_batch_run(batch.summary, target_temp_c)
                promoted_best = choose_promotable_batch_run(batch.summary, target_temp_c)
                selected_best = promoted_best or diagnostic_best
                selected_run_id = str(selected_best["summary"].get("runId") or "")
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
                        tuning_round=round_index,
                        selected_run_id=selected_run_id,
                        score_by_run_id=score_by_run_id,
                        budget_elapsed_seconds_value=budget_elapsed_seconds(budget_started_at),
                    )
                )
                chosen_profile = read_json(selected_best["candidateProfileFile"])
                updated_profile = normalize_sparse_profile(
                    runner,
                    chosen_profile,
                    anchors_c,
                    round_dir / "materialized",
                    f"normalize-best-{target_temp_c}-{round_index}",
                )
                write_json(round_dir / "accepted-sparse-profile.json", updated_profile)
                chosen_summary = selected_best["summary"]
                last_summary = chosen_summary
            except AlarmInterventionRequired:
                raise
            except Exception as exc:
                budget_outcome = (
                    "budget_exhausted" if "target_budget_exhausted" in str(exc) else "environment_blocked"
                )
                last_summary = synthetic_failure_summary(
                    target_temp_c,
                    "target_budget_exhausted" if budget_outcome == "budget_exhausted" else "round_execution_failed",
                )
                break

            if promoted_best is None:
                continue

            if budget_exhausted(budget_started_at, per_target_budget_seconds):
                budget_outcome = "budget_exhausted"
                break
            hold_seed = workspace_dir / f"hold-confirm-{round_index}-seed.json"
            write_json(hold_seed, updated_profile)
            try:
                confirm = run_budgeted_self_test(
                    runner,
                    seed_profile_file=hold_seed,
                    targets_c=[target_temp_c],
                    hold_seconds=confirm_hold_seconds,
                    output_dir=workspace_dir / f"hold-confirm-{round_index}",
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
                        "hold confirm",
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
                reseeded_profile = reseed_after_failed_hold_confirm(updated_profile, target_temp_c, confirm.summary)
                if reseeded_profile != updated_profile:
                    updated_profile = normalize_sparse_profile(
                        runner,
                        reseeded_profile,
                        anchors_c,
                        workspace_dir / "materialized",
                        f"normalize-confirm-{target_temp_c}-{round_index}",
                    )
                    write_json(workspace_dir / f"hold-confirm-{round_index}-reseed.json", updated_profile)
                budget_outcome = "not_converged"
            except AlarmInterventionRequired:
                raise
            except Exception as exc:
                budget_outcome = (
                    "budget_exhausted" if "target_budget_exhausted" in str(exc) else "environment_blocked"
                )
                last_summary = synthetic_failure_summary(
                    target_temp_c,
                    "target_budget_exhausted" if budget_outcome == "budget_exhausted" else "hold_confirm_failed",
                )
                break
    except AlarmInterventionRequired as exc:
        write_json(
            workspace_dir / "alarm-pause.json",
            {
                "kind": "thermal_alarm_pause",
                "targetTempC": int(target_temp_c),
                "budgetOutcome": "alarm_pause_required",
                "timeSpentSeconds": budget_elapsed_seconds(budget_started_at),
                "message": str(exc),
                "resumeAction": "inspect hardware, clear the alarm, then rerun the affected tests",
                "attempts": exc.attempts,
            },
        )
        raise

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
    max_tuning_rounds: int | None,
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
            "roundLimitMode": "explicit_cap"
            if isinstance(max_tuning_rounds, int) and int(max_tuning_rounds) > 0
            else "budget_only",
            "maxTuningRounds": int(max_tuning_rounds)
            if isinstance(max_tuning_rounds, int) and int(max_tuning_rounds) > 0
            else None,
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
                "Classify the failure, then compare the current point with one evidence-specific predicted point.",
                (
                    f"Allow at most {max_tuning_rounds} targeted tuning rounds while the per-target budget remains."
                    if isinstance(max_tuning_rounds, int) and int(max_tuning_rounds) > 0
                    else "Continue targeted tuning rounds until the per-target budget is exhausted or the target completes."
                ),
                f"Run a {confirm_hold_seconds}s hold confirm only after a candidate clears the promotion gate; thermal confirm failures feed the next tuning round while budget remains.",
                "Modify only the current target profile point; keep warmupPowerPermille fixed at 1000 and require 100% warmup output.",
            ],
            "dynamicApproachGate": [
                f"{target}°C: warmup exit to stable-window start must be <= {'5' if int(target) > 150 else '10'}s."
                for target in tune_targets_c
            ]
            + ["Stable window means 10s continuous sampling at >=3Hz with abs(temp-target) <= 1.5°C."],
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
                f"Run `isolapurr power runtime output --url {source_url} --enabled false --json`.",
                f"Run `isolapurr power show --url {source_url} --json` and confirm runtime.output_enabled=false plus a non-sourcing USB-C state, then wait 2s.",
                f"Run `isolapurr power runtime output --url {source_url} --enabled true --json`.",
                f"Run `isolapurr power show --url {source_url} --json` until telemetry advances and confirm runtime.output_enabled=true, auto_follow, 100W, PPS, PPS 5A, and 5000mA capability readback.",
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
