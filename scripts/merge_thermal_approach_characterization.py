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


def main() -> int:
    parser = argparse.ArgumentParser(description="Merge single-target Flux Purr approach characterization bundles")
    parser.add_argument("--bundle-dir", action="append", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--accepted-profile-file", type=Path)
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
