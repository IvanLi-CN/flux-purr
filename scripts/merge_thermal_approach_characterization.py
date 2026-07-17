#!/usr/bin/env python3

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path
from typing import Any


def load_characterization_module():
    module_path = Path(__file__).with_name("thermal_approach_characterization.py")
    spec = importlib.util.spec_from_file_location("thermal_approach_characterization", module_path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"failed to load characterization module from {module_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def bundle_sort_key(bundle_dir: Path) -> tuple[int, str]:
    name = bundle_dir.name
    digits = "".join(ch for ch in name if ch.isdigit())
    return (int(digits) if digits else 0, name)


def read_bundle(bundle_dir: Path) -> dict[str, Any]:
    bundle_path = bundle_dir / "run.bundle.json"
    if not bundle_path.exists():
        raise RuntimeError(f"missing run.bundle.json in {bundle_dir}")
    bundle = json.loads(bundle_path.read_text(encoding="utf-8"))
    if bundle.get("kind") != "thermal_approach_characterization":
        raise RuntimeError(f"unexpected bundle kind in {bundle_path}: {bundle.get('kind')}")
    if len(bundle.get("targets") or []) != 1:
        raise RuntimeError(f"expected single-target bundle in {bundle_path}")
    return bundle


def parse_target_path_arg(raw: str) -> tuple[int, Path]:
    target_raw, sep, path_raw = raw.partition("=")
    if not sep or not target_raw.strip() or not path_raw.strip():
        raise RuntimeError(f"expected TARGET=PATH, got: {raw}")
    return int(target_raw.strip()), Path(path_raw.strip())


def resolve_run_summary_path(path: Path) -> Path:
    if path.is_dir():
        candidate = path / "run.json"
        if candidate.exists():
            return candidate
    return path


def hold_failure_reason(summary: dict[str, Any], stage: dict[str, Any]) -> str | None:
    stop_reason = stage.get("stopReason")
    if stop_reason and stop_reason != "completed":
        return str(stop_reason)
    max_overshoot = stage.get("maxOvershootC")
    if isinstance(max_overshoot, (int, float)) and max_overshoot > 3.0:
        return "overshoot"
    hold_peak_to_peak = stage.get("holdPeakToPeakC")
    if isinstance(hold_peak_to_peak, (int, float)) and hold_peak_to_peak > 3.0:
        return "hold_peak_to_peak"
    validation = summary.get("validation")
    failures = validation.get("failures") if isinstance(validation, dict) else None
    if isinstance(failures, list):
        target_temp_c = stage.get("targetTempC")
        for failure in failures:
            if not isinstance(failure, dict):
                continue
            if failure.get("targetTempC") == target_temp_c:
                return str(
                    failure.get("failureReason")
                    or failure.get("reason")
                    or failure.get("stopReason")
                    or "validation_failed"
                )
    return None


def load_hold_check(summary_ref: Path, target_temp_c: int) -> dict[str, Any]:
    summary_path = resolve_run_summary_path(summary_ref)
    if not summary_path.exists():
        raise RuntimeError(f"missing confirm run summary: {summary_path}")
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    applied = summary.get("applied") or []
    stage = next(
        (
            item
            for item in applied
            if isinstance(item, dict) and int(item.get("targetTempC")) == target_temp_c
        ),
        None,
    )
    if stage is None:
        raise RuntimeError(f"missing applied stage for {target_temp_c}C in {summary_path}")
    analysis = stage.get("analysis") if isinstance(stage.get("analysis"), dict) else {}
    guard = stage.get("guard") if isinstance(stage.get("guard"), dict) else {}
    source = summary.get("source") if isinstance(summary.get("source"), dict) else {}
    passed = (
        stage.get("stopReason") == "completed"
        and isinstance(stage.get("maxOvershootC"), (int, float))
        and float(stage.get("maxOvershootC")) <= 3.0
        and (
            stage.get("holdPeakToPeakC") is None
            or (
                isinstance(stage.get("holdPeakToPeakC"), (int, float))
                and float(stage.get("holdPeakToPeakC")) <= 3.0
            )
        )
    )
    return {
        "confirmRunId": summary.get("runId"),
        "passed": passed,
        "failureReason": None if passed else hold_failure_reason(summary, stage),
        "holdSeconds": (summary.get("parameters") or {}).get("holdSeconds"),
        "maxOvershootC": stage.get("maxOvershootC"),
        "holdPeakToPeakC": stage.get("holdPeakToPeakC"),
        "firstHoldAtMs": guard.get("firstHoldAtMs"),
        "holdMedianOutputPermille": analysis.get("holdMedianOutputPermille"),
        "holdP90OutputPermille": analysis.get("holdP90OutputPermille"),
        "approachSource": analysis.get("approachSource"),
        "holdSource": analysis.get("holdSource"),
        "sourceRunPath": str(summary_path),
        "stopReason": stage.get("stopReason"),
        "selectedMode": source.get("selectedMode"),
        "resolvedBank": source.get("resolvedBank"),
        "detectedSourceClass": source.get("detectedSourceClass"),
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Merge single-target Flux Purr approach characterization bundles")
    parser.add_argument("--bundle-dir", action="append", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--accepted-profile-file", type=Path)
    parser.add_argument("--hold-run", action="append", default=[], help="TARGET=PATH to thermal self-test confirm run.json or its directory")
    parser.add_argument(
        "--bundle-disposition",
        choices=["accepted_reference", "preliminary_review"],
        default="accepted_reference",
    )
    parser.add_argument(
        "--accepted-profile-role",
        choices=["accepted_baseline", "review_candidate_snapshot"],
        default="accepted_baseline",
    )
    args = parser.parse_args()

    module = load_characterization_module()
    bundle_dirs = sorted(args.bundle_dir, key=bundle_sort_key)
    bundles = [read_bundle(bundle_dir) for bundle_dir in bundle_dirs]
    if not bundles:
        raise RuntimeError("no bundles to merge")

    first = bundles[0]
    selected_mode = first.get("selectedMode") or (first.get("source") or {}).get("selectedMode")
    resolved_bank = first.get("resolvedBank") or (first.get("source") or {}).get("resolvedBank")
    detected_source_class = first.get("detectedSourceClass") or (first.get("source") or {}).get("detectedSourceClass")
    source_device_id = (first.get("source") or {}).get("sourceDeviceId")
    target_payload = dict(first.get("target") or {})
    source_payload = dict(first.get("source") or {})
    seed_profile_file = first.get("seedProfileFile")
    combined_targets: list[dict[str, Any]] = []
    sample_lines: list[str] = []
    hold_checks = dict(parse_target_path_arg(raw) for raw in args.hold_run)

    for bundle_dir, bundle in zip(bundle_dirs, bundles):
        bundle_selected_mode = bundle.get("selectedMode") or (bundle.get("source") or {}).get("selectedMode")
        bundle_resolved_bank = bundle.get("resolvedBank") or (bundle.get("source") or {}).get("resolvedBank")
        bundle_detected_source_class = bundle.get("detectedSourceClass") or (bundle.get("source") or {}).get("detectedSourceClass")
        bundle_source_device_id = (bundle.get("source") or {}).get("sourceDeviceId")
        if bundle_selected_mode != selected_mode:
            raise RuntimeError(f"selectedMode mismatch in {bundle_dir}: {bundle_selected_mode} != {selected_mode}")
        if bundle_resolved_bank != resolved_bank:
            raise RuntimeError(f"resolvedBank mismatch in {bundle_dir}: {bundle_resolved_bank} != {resolved_bank}")
        if bundle_detected_source_class != detected_source_class:
            raise RuntimeError(
                f"detectedSourceClass mismatch in {bundle_dir}: {bundle_detected_source_class} != {detected_source_class}"
            )
        if bundle_source_device_id != source_device_id:
            raise RuntimeError(f"sourceDeviceId mismatch in {bundle_dir}: {bundle_source_device_id} != {source_device_id}")
        if bundle.get("target") != first.get("target"):
            raise RuntimeError(f"target device payload mismatch in {bundle_dir}")
        target_entry = (bundle.get("targets") or [None])[0]
        if not isinstance(target_entry, dict):
            raise RuntimeError(f"invalid target entry in {bundle_dir}")
        target_temp_c = int(target_entry["targetTempC"])
        if target_temp_c in hold_checks:
            hold_check = load_hold_check(hold_checks[target_temp_c], target_temp_c)
            if hold_check["selectedMode"] not in (None, selected_mode):
                raise RuntimeError(
                    f"hold selectedMode mismatch for {target_temp_c}C: {hold_check['selectedMode']} != {selected_mode}"
                )
            if hold_check["resolvedBank"] not in (None, resolved_bank):
                raise RuntimeError(
                    f"hold resolvedBank mismatch for {target_temp_c}C: {hold_check['resolvedBank']} != {resolved_bank}"
                )
            if hold_check["detectedSourceClass"] not in (None, detected_source_class):
                raise RuntimeError(
                    "hold detectedSourceClass mismatch for "
                    f"{target_temp_c}C: {hold_check['detectedSourceClass']} != {detected_source_class}"
                )
            target_entry = dict(target_entry)
            target_entry["holdCheck"] = {
                key: value
                for key, value in hold_check.items()
                if key not in {"selectedMode", "resolvedBank", "detectedSourceClass"}
            }
        combined_targets.append(target_entry)
        samples_path = bundle_dir / "samples.ndjson"
        if not samples_path.exists():
            raise RuntimeError(f"missing samples.ndjson in {bundle_dir}")
        with samples_path.open("r", encoding="utf-8") as handle:
            sample_lines.extend(line for line in handle if line.strip())

    combined_targets.sort(key=lambda item: int(item["targetTempC"]))

    if args.accepted_profile_file is not None:
        accepted_profile_path = args.accepted_profile_file
    else:
        accepted_profile_path = bundle_dirs[-1] / "thermal-profile.accepted.json"
    if not accepted_profile_path.exists():
        raise RuntimeError(f"missing accepted profile file: {accepted_profile_path}")
    accepted_profile = json.loads(accepted_profile_path.read_text(encoding="utf-8"))

    output_dir = args.output_dir
    output_dir.mkdir(parents=True, exist_ok=True)
    bundle_path = output_dir / "run.bundle.json"
    samples_path = output_dir / "samples.ndjson"
    accepted_profile_out = output_dir / "thermal-profile.accepted.json"
    report_path = output_dir / "index.html"
    run_id = module.slugify(
        [
            "approach-characterization-merged",
            module.dt.datetime.now().strftime("%Y%m%d-%H%M%S"),
            "pd100w-pps5a",
        ]
    )
    bundle = {
        "kind": "thermal_approach_characterization",
        "runId": run_id,
        "generatedAt": module.now_iso(),
        "selectedMode": selected_mode,
        "resolvedBank": resolved_bank,
        "detectedSourceClass": detected_source_class,
        "bundleDisposition": args.bundle_disposition,
        "acceptedProfileRole": args.accepted_profile_role,
        "target": target_payload,
        "source": source_payload,
        "seedProfileFile": seed_profile_file,
        "targets": combined_targets,
        "files": {
            "runBundlePath": str(bundle_path),
            "samplesPath": str(samples_path),
            "acceptedProfilePath": str(accepted_profile_out),
            "reportHtmlPath": str(report_path),
            "sourceBundles": [str(bundle_dir / "run.bundle.json") for bundle_dir in bundle_dirs],
        },
    }

    bundle_path.write_text(json.dumps(bundle, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    samples_path.write_text("".join(sample_lines), encoding="utf-8")
    accepted_profile_out.write_text(json.dumps(accepted_profile, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    report_path.write_text(module.generate_html(bundle), encoding="utf-8")
    print(f"merged report ready: {report_path}")
    print(f"merged bundle data: {bundle_path}")
    print(f"merged bundle samples: {samples_path}")
    print(f"merged bundle profile: {accepted_profile_out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
