#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from thermal_tuning.core import *
from thermal_tuning.report import *
from thermal_tuning.runner import *
from thermal_tuning.workflow import *


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run the 100W / pps5a flagship thermal tuning sprint with target-dependent full-speed gates and per-target budgets"
    )
    parser.add_argument("--flux-purr-bin", type=Path, default=REPO_ROOT / "target/debug/flux-purr")
    parser.add_argument("--devd-url", default="http://127.0.0.1:62610")
    parser.add_argument("--authorized-port", default=DEFAULT_AUTHORIZED_PORT)
    parser.add_argument("--source-id", required=True)
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--output-root", type=Path, default=default_output_root())
    parser.add_argument("--preliminary-profile-file", type=Path, default=PRELIMINARY_PROFILE)
    parser.add_argument("--fallback-profile-file", type=Path, default=FALLBACK_PROFILE)
    parser.add_argument("--bundle-dir", type=Path)
    parser.add_argument("--anchor-targets-c")
    parser.add_argument("--validation-targets-c")
    parser.add_argument("--tune-targets-c")
    parser.add_argument("--per-target-budget-seconds", type=int, default=DEFAULT_PER_TARGET_BUDGET_SECONDS)
    parser.add_argument(
        "--max-tuning-rounds",
        type=int,
        default=DEFAULT_MAX_TUNING_ROUNDS,
        help="Optional debug-only round cap. Omit to tune until the per-target budget is exhausted.",
    )
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
    requested_bundle_dir = args.bundle_dir or default_preliminary_bundle_dir(tune_targets_c)
    bundle_dir = requested_bundle_dir if requested_bundle_dir.is_absolute() else REPO_ROOT / requested_bundle_dir

    if args.plan_only:
        print(
            json.dumps(
                build_plan_payload(
                    source_id=args.source_id,
                    source_url=args.source_url,
                    authorized_port=args.authorized_port,
                    output_root=output_root,
                    initial_sparse_profile=output_root / "seed" / "initial-sparse-profile.json",
                    bundle_dir=bundle_dir,
                    anchors_c=anchors_c,
                    validation_targets_c=validation_targets_c,
                    tune_targets_c=tune_targets_c,
                    per_target_budget_seconds=args.per_target_budget_seconds,
                    max_tuning_rounds=args.max_tuning_rounds,
                    scout_hold_seconds=args.scout_hold_seconds,
                    confirm_hold_seconds=args.confirm_hold_seconds,
                    dry_run=args.dry_run,
                ),
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0

    review_entries = []
    try:
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
            bundle = write_preliminary_review_bundle(
                bundle_dir=bundle_dir,
                accepted_profile=current_profile,
            entries=review_entries,
            source_id=args.source_id,
            device_id=runner.resolve_device_id(args.dry_run),
                port_path=args.authorized_port,
                tuning_budget_seconds=args.per_target_budget_seconds,
            )
    except AlarmInterventionRequired as exc:
        print(
            json.dumps(
                {
                    "ok": False,
                    "kind": "thermal_alarm_pause",
                    "message": str(exc),
                    "affectedAttempts": exc.attempts,
                    "resumeAction": "inspect hardware, clear the alarm, then rerun the affected tests",
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 2
    finally:
        runner.disarm_and_clear_preview()
    print(
        json.dumps(
            {
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
                "reviewOutcomes": {str(entry["target"]): entry["budgetOutcome"] for entry in review_entries},
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
