#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import inspect
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("thermal_tuning_sprint.py")
SPEC = importlib.util.spec_from_file_location("thermal_tuning_sprint", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"failed to load module from {MODULE_PATH}")
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def make_point(target: int, **overrides: int) -> dict[str, int]:
    point = {
        "targetTempC": target,
        "brakeDistanceCentiC": 1000,
        "warmupPowerPermille": 1000,
        "approachPowerPermille": 400,
        "approachFloorPowerPermille": 250,
        "approachDampingExponentPermille": 1000,
        "approachTailWindowCentiC": 0,
        "holdPowerPermille": 200,
        "holdReheatPowerPermille": 240,
        "holdEntryCentiC": 120,
        "holdExitCentiC": 80,
        "holdOnCentiC": 20,
        "holdOffCentiC": 80,
        "overshootCutoffCentiC": 120,
        "holdKpPermillePerC": 20,
        "holdKiPermillePerCTick": 1,
        "holdBlendTicks": 1,
        "approachLeadTicks": 0,
        "holdLeadTicks": 0,
    }
    point.update(overrides)
    return point


def make_power_show_payload(
    *,
    usb_c_power_enabled: bool = True,
    sample_uptime_ms: int = 1_000,
    tps_mode: str = "auto_follow",
    power_watts: int = 100,
    pps3_limit_ma: int = 5_000,
    pd_pps_5a: bool = True,
    output_enabled: bool = True,
    usb_status: str = "ok",
) -> dict[str, object]:
    return {
        "config": {
            "capability": {
                "current": {
                    "pd_pps_5a": pd_pps_5a,
                    "pps3_limit_ma": pps3_limit_ma,
                },
                "pd": {
                    "pps": True,
                },
                "power_watts": power_watts,
                "protocols": {
                    "pd": True,
                },
            },
            "runtime": {
                "output_enabled": output_enabled,
            },
            "tps_mode": tps_mode,
        },
        "diagnostics": {
            "usb_c_actual": {
                "sample_uptime_ms": sample_uptime_ms,
                "status": usb_status,
            },
            "usb_c_power_enabled": usb_c_power_enabled,
        },
    }


class ThermalTuningSprintTests(unittest.TestCase):
    def test_target_workflow_runs_at_most_one_hold_confirm_without_recovery_scout(self) -> None:
        source = inspect.getsource(MODULE.tune_flagship_target)

        self.assertEqual(source.count("evaluation_mode=EVALUATION_MODE_HOLD_CONFIRM"), 1)
        self.assertNotIn("confirm-recovery", source)
        self.assertNotIn("confirm_recovery", source)

    def test_two_target_seed_uses_only_requested_explicit_points(self) -> None:
        class NoMaterializeRunner:
            def self_test(self, **_: object) -> object:
                raise AssertionError("two-target seed must not materialize another temperature")

        preliminary = MODULE.sparse_profile(
            {"tempFilterAlphaPermille": 700},
            [make_point(60), make_point(220)],
        )
        seed = MODULE.build_initial_sparse_seed(
            NoMaterializeRunner(),
            preliminary,
            preliminary,
            [60, 220],
            Path("/tmp/two-target-seed"),
        )

        self.assertEqual(
            [point["targetTempC"] for point in seed["points"] if point is not None],
            [60, 220],
        )

    def test_default_bundle_directory_uses_the_explicit_target_set(self) -> None:
        self.assertEqual(MODULE.DEFAULT_MAX_TUNING_ROUNDS, 2)
        self.assertIn("preliminary-pd100w-pps5a-60-220-", str(MODULE.default_preliminary_bundle_dir([60, 220])))
        self.assertIn("preliminary-pd100w-pps5a-60-140-220-", str(MODULE.default_preliminary_bundle_dir()))

    def test_predicts_target_local_fix_for_stable_window_breaking_high(self) -> None:
        point = make_point(
            60,
            brakeDistanceCentiC=1210,
            approachPowerPermille=450,
            approachFloorPowerPermille=270,
        )
        samples = [
            {"t": 5.8, "temp": 49.2, "phase": "approach"},
            {"t": 11.5, "temp": 58.5, "phase": "hold"},
            {"t": 16.0, "temp": 61.2, "phase": "hold"},
            {"t": 16.6, "temp": 61.93, "phase": "hold"},
        ]

        evidence = MODULE.classify_stability_evidence(
            samples,
            target_temp_c=60,
            warmup_exited_at_ms=5_800,
            limit_ms=10_000,
        )
        predicted = MODULE.predict_next_point(point, evidence)

        self.assertEqual(evidence["failureClass"], "stable_window_broke_high")
        self.assertGreater(predicted["brakeDistanceCentiC"], point["brakeDistanceCentiC"])
        self.assertLess(predicted["approachFloorPowerPermille"], point["approachFloorPowerPermille"])
        changed = {key for key in point if predicted[key] != point[key]}
        self.assertEqual(changed, {"brakeDistanceCentiC", "approachFloorPowerPermille"})

    def test_predicts_later_handoff_when_high_target_is_already_at_full_power(self) -> None:
        point = make_point(
            220,
            brakeDistanceCentiC=756,
            approachPowerPermille=1000,
            approachFloorPowerPermille=1000,
        )
        samples = [
            {"t": 29.196, "temp": 212.03, "phase": "approach"},
            {"t": 33.9, "temp": 218.19, "phase": "approach"},
            {"t": 34.293, "temp": 218.44, "phase": "approach"},
        ]

        evidence = MODULE.classify_stability_evidence(
            samples,
            target_temp_c=220,
            warmup_exited_at_ms=29_196,
            limit_ms=5_000,
        )
        predicted = MODULE.predict_next_point(point, evidence)

        self.assertEqual(evidence["failureClass"], "missed_lower_band_before_limit")
        self.assertAlmostEqual(evidence["temperatureGapC"], 0.06, places=2)
        self.assertLess(predicted["brakeDistanceCentiC"], point["brakeDistanceCentiC"])
        self.assertEqual(predicted["approachPowerPermille"], 1000)
        self.assertEqual(predicted["approachFloorPowerPermille"], 1000)
        changed = {key for key in point if predicted[key] != point[key]}
        self.assertEqual(changed, {"brakeDistanceCentiC"})

    def test_batch_attempt_records_preserve_every_valid_candidate_and_selection(self) -> None:
        def candidate_run(index: int) -> dict[str, object]:
            profile = MODULE.sparse_profile(
                {"tempFilterAlphaPermille": 700},
                [make_point(60, brakeDistanceCentiC=1210 + index * 40)],
            )
            return {
                "runId": f"candidate-{index}",
                "candidateProfile": profile,
                "parameters": {"candidateProfileFile": f"/tmp/candidate-{index}.json"},
                "files": {
                    "summaryPath": f"/tmp/candidate-{index}/run.json",
                    "samplesPath": f"/tmp/candidate-{index}/samples.ndjson",
                },
                "applied": [
                    {
                        "targetTempC": 60,
                        "stopReason": "completed" if index == 1 else "full_speed_to_stable_timeout",
                        "maxOvershootC": 1.0 + index,
                        "holdPeakToPeakC": 1.2 + index,
                        "analysis": {},
                        "fullSpeedToStable": {"limitMs": 10_000},
                    }
                ],
                "validation": {"failures": []},
            }

        records = MODULE.batch_attempt_records(
            {"runs": [candidate_run(0), candidate_run(1), candidate_run(2)]},
            60,
            first_round_number=2,
            tuning_round=1,
            selected_run_id="candidate-1",
        )

        self.assertEqual([record["round"] for record in records], [2, 3, 4])
        self.assertEqual([record["candidateName"] for record in records], ["candidate-0", "candidate-1", "candidate-2"])
        self.assertEqual([record["selected"] for record in records], [False, True, False])
        self.assertTrue(all(record["attemptType"] == "batch_candidate" for record in records))

    def test_completed_high_target_with_only_200ms_margin_requires_another_candidate(self) -> None:
        point = make_point(
            220,
            brakeDistanceCentiC=756,
            approachPowerPermille=1000,
            approachFloorPowerPermille=1000,
        )
        stage = {
            "targetTempC": 220,
            "stopReason": "completed",
            "fullSpeedToStable": {
                "warmupExitedAtMs": 28_849,
                "settleTimeMs": 4_800,
                "limitMs": 5_000,
            },
        }
        evidence = MODULE.stability_evidence_for_stage(stage, [], 220)
        predicted = MODULE.predict_next_point(point, evidence)

        self.assertEqual(evidence["failureClass"], "within_gate_low_margin")
        self.assertEqual(evidence["timeMarginMs"], 200)
        self.assertLess(predicted["brakeDistanceCentiC"], point["brakeDistanceCentiC"])

    def test_completed_stable_window_is_not_reclassified_by_later_hold_excursion(self) -> None:
        stage = {
            "targetTempC": 140,
            "stopReason": "completed",
            "fullSpeedToStable": {
                "warmupExitedAtMs": 10_000,
                "settleTimeMs": 5_000,
                "limitMs": 10_000,
            },
        }
        samples = [
            {"t": 10.0, "temp": 130.0},
            {"t": 15.0, "temp": 140.0},
            {"t": 30.0, "temp": 142.0},
        ]

        evidence = MODULE.stability_evidence_for_stage(stage, samples, 140)

        self.assertEqual(evidence["failureClass"], "within_gate")

    def test_candidate_generation_ignores_unrelated_retune_changes_when_evidence_is_specific(self) -> None:
        current_point = make_point(60, brakeDistanceCentiC=1210, approachFloorPowerPermille=270)
        current = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [current_point])
        retuned = MODULE.sparse_profile(
            {"tempFilterAlphaPermille": 700},
            [
                make_point(
                    60,
                    brakeDistanceCentiC=1800,
                    approachFloorPowerPermille=600,
                    holdPowerPermille=700,
                    holdReheatPowerPermille=800,
                    holdKpPermillePerC=90,
                )
            ],
        )
        stage = {
            "targetTempC": 60,
            "stopReason": "full_speed_to_stable_timeout",
            "holdPeakToPeakC": 2.2,
            "analysis": {},
            "fullSpeedToStable": {"warmupExitedAtMs": 5_800, "limitMs": 10_000},
        }
        samples = [
            {"t": 5.8, "temp": 49.2},
            {"t": 11.5, "temp": 58.5},
            {"t": 16.6, "temp": 61.93},
        ]

        variants = MODULE.build_candidate_variants(current, retuned, 60, stage, samples)

        self.assertEqual([variant.name for variant in variants], ["current", "stable_window_broke_high"])
        predicted = MODULE.explicit_point(variants[1].profile, 60)
        self.assertEqual(predicted["holdPowerPermille"], current_point["holdPowerPermille"])
        self.assertEqual(predicted["holdReheatPowerPermille"], current_point["holdReheatPowerPermille"])
        self.assertEqual(predicted["holdKpPermillePerC"], current_point["holdKpPermillePerC"])

    def test_short_scout_p2p_does_not_add_a_hold_candidate(self) -> None:
        current = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [make_point(60)])
        stage = {
            "targetTempC": 60,
            "stopReason": "full_speed_to_stable_timeout",
            "holdPeakToPeakC": 3.2,
            "analysis": {},
            "fullSpeedToStable": {"warmupExitedAtMs": 5_800, "limitMs": 10_000},
        }
        samples = [
            {"t": 5.8, "temp": 49.2},
            {"t": 11.5, "temp": 58.5},
            {"t": 16.6, "temp": 61.93},
        ]

        variants = MODULE.build_candidate_variants(current, current, 60, stage, samples)

        self.assertEqual([variant.name for variant in variants], ["current", "stable_window_broke_high"])

    def test_promotion_requires_full_warmup_and_margin(self) -> None:
        def samples_file(path: Path, output: int) -> None:
            path.write_text(
                json.dumps(
                    {
                        "targetTempC": 60,
                        "elapsedMs": 100,
                        "phase": "warmup",
                        "status": {
                            "currentTempC": 25.0,
                            "heaterOutputPercent": 100,
                            "heaterPhysicalOutputPercent": output,
                        },
                    }
                )
                + "\n",
                encoding="utf-8",
            )

        def candidate(run_id: str, path: Path, settle_time_ms: int, output: int) -> dict[str, object]:
            return {
                "runId": run_id,
                "source": {
                    "selectedMode": "100w",
                    "resolvedBank": "pps5a",
                    "detectedSourceClass": "pps5a",
                },
                "parameters": {"candidateProfileFile": str(path.with_suffix(".json"))},
                "files": {"samplesPath": str(path), "summaryPath": str(path.with_suffix(".run.json"))},
                "applied": [
                    {
                        "targetTempC": 60,
                        "stopReason": "completed",
                        "analysis": {},
                        "fullSpeedToStable": {
                            "warmupExitedAtMs": 1_000,
                            "settleTimeMs": settle_time_ms,
                            "limitMs": 10_000,
                        },
                    }
                ],
                "validation": {"failures": []},
            }

        with tempfile.TemporaryDirectory() as tmpdir:
            directory = Path(tmpdir)
            low_margin_path = directory / "low-margin.ndjson"
            accepted_path = directory / "accepted.ndjson"
            no_warmup_path = directory / "no-warmup.ndjson"
            samples_file(low_margin_path, 100)
            samples_file(accepted_path, 100)
            samples_file(no_warmup_path, 90)
            batch = {
                "runs": [
                    candidate("low-margin", low_margin_path, 9_500, 100),
                    candidate("accepted", accepted_path, 8_500, 100),
                    candidate("no-warmup", no_warmup_path, 8_000, 90),
                ]
            }

            promoted = MODULE.choose_promotable_batch_run(batch, 60)

        self.assertIsNotNone(promoted)
        self.assertEqual(promoted["summary"]["runId"], "accepted")

    def test_merge_point_preserves_sparse_shape(self) -> None:
        profile = MODULE.sparse_profile(
            {"tempFilterAlphaPermille": 700},
            [make_point(60), make_point(100), make_point(140)],
        )
        merged = MODULE.merge_point(profile, make_point(100, holdPowerPermille=333))
        point = MODULE.explicit_point(merged, 100)
        self.assertIsNotNone(point)
        self.assertEqual(point["holdPowerPermille"], 333)
        self.assertEqual(len(merged["points"]), MODULE.THERMAL_CONTROL_PROFILE_MAX_POINTS)
        self.assertEqual(sum(1 for item in merged["points"] if item is not None), 3)

    def test_candidate_score_prefers_completed_low_error(self) -> None:
        better = {
            "applied": [
                {
                    "targetTempC": 100,
                    "stopReason": "completed",
                    "maxOvershootC": 0.8,
                    "holdPeakToPeakC": 1.2,
                    "analysis": {
                        "approachCurveMeanAbsErrorC": 0.5,
                        "approachPreferredMs": 5000,
                        "approachLimitMs": 10000,
                    },
                    "fullSpeedToStable": {"settleTimeMs": 5200, "limitMs": 10_000},
                    "terminalRuntimeDropReason": None,
                }
            ],
            "validation": {"failures": []},
        }
        worse = {
            "applied": [
                {
                    "targetTempC": 100,
                    "stopReason": "full_speed_to_stable_timeout",
                    "maxOvershootC": 2.5,
                    "holdPeakToPeakC": 2.6,
                    "analysis": {
                        "approachCurveMeanAbsErrorC": 1.8,
                        "approachPreferredMs": 5000,
                        "approachLimitMs": 10000,
                    },
                    "fullSpeedToStable": {"settleTimeMs": None, "limitMs": 10_000},
                    "terminalRuntimeDropReason": None,
                }
            ],
            "validation": {
                "failures": [
                    {
                        "targetTempC": 100,
                        "reason": "full_speed_to_stable_missing",
                    }
                ]
            },
        }
        self.assertLess(MODULE.candidate_score(better, 100), MODULE.candidate_score(worse, 100))

    def test_candidate_score_uses_full_speed_gate_before_hold_metrics(self) -> None:
        within_gate = {
            "applied": [
                {
                    "targetTempC": 60,
                    "stopReason": "completed",
                    "maxOvershootC": 1.4,
                    "holdPeakToPeakC": 1.7,
                    "analysis": {},
                    "fullSpeedToStable": {"settleTimeMs": 8_500, "limitMs": 10_000},
                    "terminalRuntimeDropReason": None,
                }
            ],
            "validation": {"failures": []},
        }
        late_even_with_better_hold = {
            "applied": [
                {
                    "targetTempC": 60,
                    "stopReason": "completed",
                    "maxOvershootC": 1.0,
                    "holdPeakToPeakC": 1.3,
                    "analysis": {},
                    "fullSpeedToStable": {"settleTimeMs": 12_000, "limitMs": 10_000},
                    "terminalRuntimeDropReason": None,
                }
            ],
            "validation": {"failures": []},
        }
        self.assertLess(
            MODULE.candidate_score(within_gate, 60),
            MODULE.candidate_score(late_even_with_better_hold, 60),
        )

    def test_budget_helpers_enforce_per_target_limit(self) -> None:
        self.assertEqual(MODULE.budget_remaining_seconds(100.0, 1200, now_monotonic=1250.0), 50)
        self.assertTrue(MODULE.budget_exhausted(100.0, 1200, now_monotonic=1300.0))
        self.assertIsNone(MODULE.step_timeouts_for_budget(80, 60))
        self.assertEqual(MODULE.cooldown_threshold(60), 35.0)
        self.assertEqual(MODULE.cooldown_threshold(140), 100.0)

    def test_verify_isolapurr_power_show_accepts_expected_recovery_state(self) -> None:
        baseline = make_power_show_payload(sample_uptime_ms=10_000)
        uptime_ms = MODULE.verify_isolapurr_power_show(baseline, expect_usb_c_enabled=True)
        self.assertEqual(uptime_ms, 10_000)
        followup = make_power_show_payload(sample_uptime_ms=10_600)
        self.assertEqual(
            MODULE.verify_isolapurr_power_show(
                followup,
                expect_usb_c_enabled=True,
                previous_sample_uptime_ms=10_000,
            ),
            10_600,
        )

    def test_verify_isolapurr_power_show_rejects_stale_telemetry(self) -> None:
        with self.assertRaises(RuntimeError):
            MODULE.verify_isolapurr_power_show(
                make_power_show_payload(sample_uptime_ms=10_000),
                expect_usb_c_enabled=True,
                previous_sample_uptime_ms=10_000,
            )

    def test_build_plan_payload_lists_only_whitelisted_flow(self) -> None:
        planned = MODULE.build_plan_payload(
            source_id="f293cc",
            source_url="http://192.168.31.224",
            authorized_port="/dev/cu.usbmodem2111401",
            output_root=Path("/tmp/flagship"),
            initial_sparse_profile=Path("/tmp/flagship/seed/initial-sparse-profile.json"),
            bundle_dir=Path("/tmp/flagship/bundle"),
            anchors_c=[60, 140, 220],
            validation_targets_c=[60, 140, 220],
            tune_targets_c=[60, 140, 220],
            per_target_budget_seconds=1_200,
            max_tuning_rounds=2,
            scout_hold_seconds=12,
            confirm_hold_seconds=60,
            dry_run=False,
        )
        self.assertEqual(planned["scope"]["flagshipTargetsC"], [60, 140, 220])
        self.assertEqual(planned["deviceConnection"]["fluxPurr"]["authorizedPort"], "/dev/cu.usbmodem2111401")
        self.assertIn("Do not run the full temperature ladder.", planned["forbiddenOperations"])
        self.assertEqual(
            planned["executionWhitelist"]["allowedResults"],
            ["completed", "not_converged", "budget_exhausted", "environment_blocked"],
        )
        self.assertEqual(len(planned["powerCycleRecovery"]["steps"]), 7)

    def test_more_heat_delays_hold_entry(self) -> None:
        point = make_point(60, holdEntryCentiC=240, approachFloorPowerPermille=160)
        mutated = MODULE.mutate_more_heat(point, 60)
        self.assertLess(mutated["holdEntryCentiC"], point["holdEntryCentiC"])
        self.assertGreater(mutated["approachFloorPowerPermille"], point["approachFloorPowerPermille"])

    def test_remedial_targets_include_failed_anchors(self) -> None:
        self.assertEqual(MODULE.remedial_anchor_targets([60, 80, 160]), [60, 100, 180])

    def test_source_run_summary_filters_to_selected_target(self) -> None:
        summary = {
            "parameters": {"targetsC": [60, 100]},
            "tuningSteps": [
                {"targetTempC": 60, "stageIndex": 0},
                {"targetTempC": 100, "stageIndex": 1},
            ],
            "applied": [
                {"targetTempC": 60, "stopReason": "completed"},
                {"targetTempC": 100, "stopReason": "completed"},
            ],
            "validation": {
                "failures": [{"targetTempC": 100, "reason": "hold_p2p"}],
                "passed": False,
            },
        }
        filtered = MODULE.source_run_summary(summary, 100)
        self.assertEqual(filtered["parameters"]["targetsC"], [100])
        self.assertEqual(len(filtered["applied"]), 1)
        self.assertEqual(filtered["applied"][0]["targetTempC"], 100)
        self.assertEqual(len(filtered["tuningSteps"]), 1)
        self.assertEqual(filtered["tuningSteps"][0]["targetTempC"], 100)
        self.assertEqual(filtered["validation"]["expectedTargetsC"], [100])
        self.assertEqual(filtered["validation"]["failures"][0]["reason"], "hold_p2p")
        self.assertFalse(filtered["validation"]["passed"])

    def test_target_run_entry_uses_sample_parameters_for_interpolated_target(self) -> None:
        summary = {
            "profilePersistence": "not_saved",
            "applied": [
                {
                    "targetTempC": 80,
                    "stopReason": "completed",
                    "analysis": {},
                    "fullSpeedToStable": {},
                }
            ],
            "validation": {"failures": [], "passed": True},
            "tuningSteps": [],
        }
        accepted = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [make_point(60), make_point(100)])
        samples = [
            {
                "heaterParameters": make_point(
                    80,
                    brakeDistanceCentiC=1432,
                    approachPowerPermille=448,
                    approachFloorPowerPermille=293,
                )
            }
        ]
        entry = MODULE.target_run_entry(summary, accepted, 80, samples)
        self.assertEqual(entry["pointSource"], "sample_parameters")
        self.assertEqual(entry["point"]["targetTempC"], 80)
        self.assertEqual(entry["point"]["brakeDistanceCentiC"], 1432)

    def test_tuning_rounds_for_target_extracts_point_and_metrics(self) -> None:
        candidate_profile = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [make_point(100, holdPowerPermille=333)])
        summary = {
            "tuningSteps": [
                {
                    "stageIndex": 2,
                    "targetTempC": 100,
                    "candidateProfile": candidate_profile,
                    "result": {
                        "stopReason": "completed",
                        "maxOvershootC": 1.2,
                        "holdPeakToPeakC": 1.6,
                        "fullSpeedToStable": {"settleTimeMs": 5300},
                        "analysis": {
                            "approachCurveMeanAbsErrorC": 0.7,
                            "approachCurveDeviationClass": "within_gate",
                        },
                    },
                    "samples": [
                        {
                            "elapsedMs": 1000,
                            "phase": "approach",
                            "status": {
                                "currentTempC": 99.1,
                                "heaterFilteredTempC": 98.8,
                                "heaterOutputPercent": 21,
                                "heaterPhysicalOutputPercent": 21,
                                "pdRequestMv": 11800,
                            },
                            "sourceTelemetry": {
                                "voltageMv": 11850,
                                "currentMa": 1500,
                                "powerMw": 17775,
                            },
                        }
                    ],
                }
            ]
        }
        rounds = MODULE.tuning_rounds_for_target(summary, 100)
        self.assertEqual(len(rounds), 1)
        self.assertEqual(rounds[0]["point"]["holdPowerPermille"], 333)
        self.assertEqual(rounds[0]["result"]["settleTimeMs"], 5300)
        self.assertEqual(len(rounds[0]["samples"]), 1)
        self.assertEqual(rounds[0]["samples"][0]["phase"], "approach")

    def test_build_baseline_bundle_json_maps_source_runs(self) -> None:
        summary = {
            "files": {
                "summaryPath": str(MODULE.REPO_ROOT / "thermal-self-test-runs/final/run.json"),
                "samplesPath": str(MODULE.REPO_ROOT / "thermal-self-test-runs/final/samples.ndjson"),
            },
            "parameters": {"sampleIntervalMs": 300},
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            bundle_dir = Path(tmpdir)
            source_paths = {
                60: bundle_dir / "source-run-summaries/60.run.json",
                100: bundle_dir / "source-run-summaries/100.run.json",
            }
            bundle = MODULE.build_baseline_bundle_json(summary, bundle_dir, source_paths)
        self.assertEqual(bundle["kind"], "thermal_self_test_baseline_bundle")
        self.assertEqual(bundle["selectedMode"], MODULE.PROFILE_MODE)
        self.assertEqual(bundle["resolvedBank"], MODULE.EXPECTED_BANK)
        self.assertEqual(bundle["detectedSourceClass"], MODULE.EXPECTED_SOURCE_CLASS)

    def test_preliminary_bundle_samples_preserve_all_valid_attempts(self) -> None:
        profile = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [make_point(60)])
        attempts = [
            {
                "round": 1,
                "attemptType": "scout",
                "candidateName": None,
                "selected": False,
                "evidenceValid": True,
                "samples": [{"t": 0.0, "temp": 30.0}],
            },
            {
                "round": 2,
                "attemptType": "batch_candidate",
                "candidateName": "stable_window_broke_high",
                "selected": True,
                "evidenceValid": True,
                "samples": [{"t": 0.0, "temp": 31.0}],
            },
            {
                "round": 3,
                "attemptType": "batch_candidate",
                "candidateName": "invalid-source",
                "selected": False,
                "evidenceValid": False,
                "samples": [{"t": 0.0, "temp": 32.0}],
            },
        ]
        entry = {
            "runId": "fixture",
            "target": 60,
            "targetTempC": 60,
            "ok": False,
            "saved": False,
            "budgetOutcome": "not_converged",
            "timeSpentSeconds": 10,
            "roundCount": 3,
            "validTestCount": 2,
            "invalidTestCount": 1,
            "approachReference": {"limitMs": 10_000},
            "point": make_point(60),
            "pointSource": "review_candidate_snapshot",
            "rounds": attempts,
            "result": {"stopReason": "full_speed_to_stable_timeout"},
            "failures": [{"reason": "full_speed_to_stable_timeout"}],
            "samples": [],
        }
        with tempfile.TemporaryDirectory() as tmpdir:
            bundle_dir = Path(tmpdir)
            MODULE.write_preliminary_review_bundle(
                bundle_dir=bundle_dir,
                accepted_profile=profile,
                entries=[entry],
                source_id="f293cc9c139e",
                device_id="fixture-device",
                port_path="/dev/cu.usbmodem2111401",
                tuning_budget_seconds=1200,
            )
            samples = [json.loads(line) for line in (bundle_dir / "samples.ndjson").read_text().splitlines()]
            bundle = json.loads((bundle_dir / "run.bundle.json").read_text())

        self.assertEqual(len(samples), 2)
        self.assertEqual([sample["attemptNumber"] for sample in samples], [1, 2])
        self.assertEqual(samples[1]["candidateName"], "stable_window_broke_high")
        self.assertEqual(bundle["runs"][0]["validTestCount"], 2)

    def test_report_html_tooltips_show_all_visible_series_with_raw_units(self) -> None:
        html = MODULE.render_baseline_html(
            {
                "generatedAt": "2026-07-18T00:00:00Z",
                "title": "fixture",
                "subtitle": "fixture",
                "selectedMode": MODULE.PROFILE_MODE,
                "resolvedBank": MODULE.EXPECTED_BANK,
                "detectedSourceClass": MODULE.EXPECTED_SOURCE_CLASS,
                "sourcePreset": "21V / 5.0A",
                "provider": "IsolaPurr",
                "sourceDeviceId": "f293cc9c139e",
                "deviceId": "fixture-device",
                "port": "/dev/cu.usbmodem2111401",
                "runs": [
                    {
                        "runId": "fixture-run",
                        "target": 60,
                        "targetTempC": 60,
                        "ok": True,
                        "saved": False,
                        "evidence": "preliminary_review",
                        "budgetOutcome": "completed",
                        "timeSpentSeconds": 1,
                        "roundCount": 1,
                        "approachReference": {"limitMs": 10_000},
                        "point": make_point(60),
                        "pointSource": "review_candidate_snapshot",
                        "rounds": [],
                        "result": {"maxOvershootC": 1.0, "holdPeakToPeakC": 1.0},
                        "failures": [],
                        "samples": [
                            {
                                "t": 0.0,
                                "temp": 59.0,
                                "filtered": 59.0,
                                "command": 100,
                                "requestV": 21.0,
                                "sourceVoltageV": 21.0,
                                "sourceCurrentA": 4.8,
                                "sourcePowerW": 96.0,
                                "phase": "hold",
                            }
                        ],
                    }
                ],
                "history": [],
            }
        )
        self.assertIn("function seriesTooltip(anchor,items,options)", html)
        self.assertIn("items=painted.map", html)
        self.assertIn("name:'PD 请求电压'", html)
        self.assertIn("rawUnit:'V'", html)
        self.assertIn("name:'电流'", html)
        self.assertIn("rawUnit:'A'", html)
        self.assertIn("name:'功率'", html)
        self.assertIn("rawUnit:'W'", html)


if __name__ == "__main__":
    unittest.main()
