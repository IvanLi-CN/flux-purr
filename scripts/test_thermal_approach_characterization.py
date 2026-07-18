#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
MODULE_PATH = Path(__file__).with_name("thermal_approach_characterization.py")
MODULE_SPEC = importlib.util.spec_from_file_location("thermal_approach_characterization", MODULE_PATH)
if MODULE_SPEC is None or MODULE_SPEC.loader is None:
    raise RuntimeError(f"failed to load module from {MODULE_PATH}")
MODULE = importlib.util.module_from_spec(MODULE_SPEC)
sys.modules[MODULE_SPEC.name] = MODULE
MODULE_SPEC.loader.exec_module(MODULE)


def make_point(target: int, **overrides: int) -> dict[str, int]:
    point = {
        "targetTempC": target,
        "brakeDistanceCentiC": 1200,
        "warmupPowerPermille": 1000,
        "approachPowerPermille": 420,
        "approachFloorPowerPermille": 210,
        "approachDampingExponentPermille": 1000,
        "approachTailWindowCentiC": 0,
        "holdPowerPermille": 160,
        "holdReheatPowerPermille": 180,
        "holdEntryCentiC": 180,
        "holdExitCentiC": 120,
        "holdOnCentiC": 20,
        "holdOffCentiC": 80,
        "overshootCutoffCentiC": 120,
        "holdKpPermillePerC": 20,
        "holdKiPermillePerCTick": 1,
        "holdBlendTicks": 1,
        "approachLeadTicks": 1,
        "holdLeadTicks": 1,
    }
    point.update(overrides)
    return point


def make_bundle(target_temp_c: int) -> tuple[dict, dict]:
    point = make_point(target_temp_c)
    warmup = MODULE.dry_run_target_result(target_temp_c, point, "warmup_scout_25")
    zero = MODULE.dry_run_target_result(target_temp_c, point, "zero_coast")
    half = MODULE.dry_run_target_result(target_temp_c, point, "half_floor_50")
    target = MODULE.build_target_result(target_temp_c, point, [warmup, zero, half])
    bundle = {
        "kind": "thermal_approach_characterization",
        "runId": f"dry-{target_temp_c}",
        "generatedAt": "2026-07-18T00:00:00Z",
        "selectedMode": "100w",
        "resolvedBank": "pps5a",
        "detectedSourceClass": "pps5a",
        "acceptedProfileRole": "seed_profile_snapshot",
        "target": {
            "deviceId": "dry-run-device",
            "portPath": "/dev/cu.usbmodem2111401",
            "hardwareId": None,
        },
        "source": {
            "selectedMode": "100w",
            "resolvedBank": "pps5a",
            "detectedSourceClass": "pps5a",
            "sourceDeviceId": "f293cc-mock",
        },
        "seedProfileFile": str(REPO_ROOT / "thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json"),
        "targets": [target],
        "files": {},
    }
    return bundle, target


class ThermalApproachCharacterizationTests(unittest.TestCase):
    def test_parse_variant_ids_defaults_and_custom(self) -> None:
        self.assertEqual(MODULE.parse_variant_ids(None), ["warmup_scout_25"])
        self.assertEqual(
            MODULE.parse_variant_ids("warmup_scout_25,zero_coast,half_floor_50"),
            ["warmup_scout_25", "zero_coast", "half_floor_50"],
        )

    def test_cooldown_threshold_uses_flagship_rules(self) -> None:
        self.assertEqual(MODULE.cooldown_threshold(60), 35.0)
        self.assertEqual(MODULE.cooldown_threshold(79), 35.0)
        self.assertEqual(MODULE.cooldown_threshold(80), 40.0)
        self.assertEqual(MODULE.cooldown_threshold(140), 100.0)
        self.assertEqual(MODULE.cooldown_threshold(220), 180.0)

    def test_build_target_result_attaches_warmup_scout_summary(self) -> None:
        point = make_point(140)
        warmup = MODULE.dry_run_target_result(140, point, "warmup_scout_25")
        target = MODULE.build_target_result(140, point, [warmup])
        summary = target["warmupScout25"]
        self.assertTrue(summary["passed"])
        self.assertEqual(summary["variantId"], "warmup_scout_25")
        self.assertIsNotNone(summary["warmupExitTempC"])
        self.assertIsNotNone(summary["approachDurationMs"])
        self.assertEqual(summary["variantFloorPermille"], warmup["variantFloorPermille"])

    def test_generate_html_renders_warmup_scout_and_candidate_snapshot(self) -> None:
        bundle, _ = make_bundle(60)
        html = MODULE.generate_html(bundle)
        self.assertIn("逼近阶段 25% 参考报告", html)
        self.assertIn("25% scout", html)
        self.assertIn("candidate brake", html)
        self.assertIn("Hold confirm", html)
        self.assertIn("warmup → approach", html)

    def test_merge_preserves_warmup_scout_and_attaches_hold_check(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            source_dirs = []
            for target_temp_c in (60, 140):
                bundle, target = make_bundle(target_temp_c)
                bundle_dir = root / f"{target_temp_c}c"
                bundle_dir.mkdir(parents=True, exist_ok=True)
                (bundle_dir / "run.bundle.json").write_text(
                    json.dumps(bundle, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8",
                )
                sample_variant = target["variants"][0]
                samples = sample_variant["samples"][:2]
                (bundle_dir / "samples.ndjson").write_text(
                    "".join(json.dumps(sample, ensure_ascii=False) + "\n" for sample in samples),
                    encoding="utf-8",
                )
                accepted_profile = {
                    "settings": {"tempFilterAlphaPermille": 700},
                    "points": [make_point(target_temp_c)],
                }
                (bundle_dir / "thermal-profile.accepted.json").write_text(
                    json.dumps(accepted_profile, ensure_ascii=False, indent=2) + "\n",
                    encoding="utf-8",
                )
                source_dirs.append(bundle_dir)

            hold_run_dir = root / "hold-runs"
            hold_run_dir.mkdir(parents=True, exist_ok=True)
            hold_summary_path = hold_run_dir / "60.run.json"
            hold_summary = {
                "runId": "hold-60",
                "parameters": {"holdSeconds": 60},
                "source": {
                    "selectedMode": "100w",
                    "resolvedBank": "pps5a",
                    "detectedSourceClass": "pps5a",
                },
                "applied": [
                    {
                        "targetTempC": 60,
                        "stopReason": "completed",
                        "maxOvershootC": 1.2,
                        "holdPeakToPeakC": 1.1,
                        "analysis": {
                            "holdMedianOutputPermille": 92,
                            "holdP90OutputPermille": 110,
                            "approachSource": {"powerMw": {"avg": 42000}},
                            "holdSource": {"powerMw": {"avg": 5200}},
                        },
                        "guard": {"firstHoldAtMs": 6500},
                    }
                ],
            }
            hold_summary_path.write_text(
                json.dumps(hold_summary, ensure_ascii=False, indent=2) + "\n",
                encoding="utf-8",
            )

            output_dir = root / "merged"
            cmd = [
                "python3",
                str(REPO_ROOT / "scripts/merge_thermal_approach_characterization.py"),
                "--bundle-dir",
                str(source_dirs[0]),
                "--bundle-dir",
                str(source_dirs[1]),
                "--output-dir",
                str(output_dir),
                "--bundle-disposition",
                "preliminary_review",
                "--accepted-profile-role",
                "review_candidate_snapshot",
                "--hold-run",
                f"60={hold_summary_path}",
            ]
            subprocess.run(cmd, check=True, cwd=REPO_ROOT)
            merged = json.loads((output_dir / "run.bundle.json").read_text(encoding="utf-8"))
            target_60 = next(item for item in merged["targets"] if int(item["targetTempC"]) == 60)
            self.assertIn("warmupScout25", target_60)
            self.assertIn("holdCheck", target_60)
            self.assertEqual(target_60["holdCheck"]["confirmRunId"], "hold-60")
            self.assertTrue(target_60["holdCheck"]["passed"])
            self.assertEqual(merged["bundleDisposition"], "preliminary_review")
            self.assertEqual(merged["acceptedProfileRole"], "review_candidate_snapshot")


if __name__ == "__main__":
    unittest.main()
