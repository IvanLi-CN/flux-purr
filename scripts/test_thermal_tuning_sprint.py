#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import inspect
import json
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock
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
    usb_current_ma: int = 41,
    usb_power_mw: int = 497,
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
                "current_ma": usb_current_ma,
                "power_mw": usb_power_mw,
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

    def test_tune_flagship_target_writes_alarm_pause_file(self) -> None:
        class PauseRunner:
            def self_test(self, **_: object) -> object:
                raise MODULE.AlarmInterventionRequired(
                    [
                        {
                            "runId": "alarm-1",
                            "faultReasons": ["sensor-glitch"],
                            "faultAttentionPending": True,
                            "summaryPath": "/tmp/alarm-1.json",
                        }
                    ]
                )

        with tempfile.TemporaryDirectory() as tmpdir:
            workspace = Path(tmpdir)
            profile = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [make_point(60)])
            with self.assertRaises(MODULE.AlarmInterventionRequired):
                MODULE.tune_flagship_target(
                    PauseRunner(),
                    profile,
                    60,
                    [60],
                    workspace,
                    per_target_budget_seconds=1200,
                    max_tuning_rounds=None,
                    scout_hold_seconds=12,
                    confirm_hold_seconds=60,
                )

            pause = json.loads((workspace / "alarm-pause.json").read_text(encoding="utf-8"))

        self.assertEqual(pause["kind"], "thermal_alarm_pause")
        self.assertEqual(pause["targetTempC"], 60)
        self.assertEqual(pause["attempts"][0]["runId"], "alarm-1")

    def test_disarm_and_clear_preview_reenables_active_cooling(self) -> None:
        recorded: list[list[str]] = []

        class RecordingRunner(MODULE.FluxPurrRunner):
            def resolve_device_id(self, dry_run_override: bool = False) -> str:
                return "fp-device-1"

            def run_json_command(
                self,
                cmd: list[str],
                *,
                retry_with_source_recovery: bool = False,
            ) -> dict[str, object]:
                recorded.append(cmd)
                return {"ok": True}

        runner = RecordingRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=False,
        )

        runner.disarm_and_clear_preview()

        self.assertEqual(len(recorded), 2)
        self.assertEqual(recorded[0][-4:], ["--heater-enabled", "false", "--active-cooling", "true"])

    def test_disarm_and_clear_preview_refreshes_stale_device_id(self) -> None:
        recorded: list[list[str]] = []

        class RefreshingRunner(MODULE.FluxPurrRunner):
            def __init__(self) -> None:
                super().__init__(
                    flux_purr_bin=Path("/tmp/flux-purr"),
                    devd_url="http://127.0.0.1:62610",
                    authorized_port="/dev/cu.usbmodem2111401",
                    source_id="f293cc",
                    source_url="http://192.168.31.224",
                    dry_run=False,
                    auto_recover_source=False,
                )
                self._device_ids = iter(["fp-device-1", "fp-device-2", "fp-device-2"])
                self.recovered = False

            def resolve_device_id(self, dry_run_override: bool = False) -> str:
                return next(self._device_ids)

            def recover_source_output(self) -> None:
                self.recovered = True

        def fake_run(cmd: list[str], cwd: Path, capture_output: bool, text: bool) -> SimpleNamespace:
            recorded.append(list(cmd))
            if len(recorded) == 1:
                return SimpleNamespace(
                    returncode=1,
                    stdout="",
                    stderr='Error: "create lease failed for fp-device-1 after waiting for native device refresh: {\\"error\\":{\\"code\\":\\"device_not_found\\"}}"',
                )
            return SimpleNamespace(returncode=0, stdout='{"ok":true}\n', stderr="")

        runner = RefreshingRunner()
        with mock.patch.object(MODULE.subprocess, "run", side_effect=fake_run):
            with mock.patch.object(MODULE.time, "sleep", return_value=None):
                runner.disarm_and_clear_preview()

        self.assertFalse(runner.recovered)
        self.assertEqual(recorded[0][recorded[0].index("--device") + 1], "fp-device-1")
        self.assertEqual(recorded[1][recorded[1].index("--device") + 1], "fp-device-2")
        self.assertEqual(recorded[2][recorded[2].index("--device") + 1], "fp-device-2")

    def test_device_refresh_classifier_covers_serial_reconnect_timeout(self) -> None:
        runner = MODULE.FluxPurrRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=False,
        )

        self.assertTrue(runner._command_needs_device_refresh("device_not_found"))
        self.assertTrue(runner._command_needs_device_refresh("lease_conflict"))
        self.assertTrue(runner._command_needs_device_refresh("serial_reconnect_timeout"))

    def test_resolve_source_id_expands_short_prefix_to_full_device_id(self) -> None:
        runner = MODULE.FluxPurrRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=False,
        )

        with mock.patch.object(
            runner,
            "run_subprocess_json",
            return_value={"device": {"device_id": "f293cc9c139e"}},
        ) as run_subprocess_json:
            resolved = runner.resolve_source_id(False)

        self.assertEqual(resolved, "f293cc9c139e")
        self.assertEqual(runner._resolved_source_id, "f293cc9c139e")
        run_subprocess_json.assert_called_once()

    def test_resolve_source_id_rejects_mismatched_device_id(self) -> None:
        runner = MODULE.FluxPurrRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=False,
        )

        with mock.patch.object(
            runner,
            "run_subprocess_json",
            return_value={"device": {"device_id": "deadbeef0000"}},
        ):
            with self.assertRaisesRegex(RuntimeError, "isolapurr identity mismatch"):
                runner.resolve_source_id(False)

    def test_resolve_device_id_rejects_missing_port_placeholder_only_authorized_port(self) -> None:
        runner = MODULE.FluxPurrRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=False,
        )
        placeholder = {
            "id": "serial-_dev_cu.usbmodem2111401",
            "portPath": "/dev/cu.usbmodem2111401",
            "identity": {"buildId": "native-serial-placeholder"},
        }

        with mock.patch.object(runner, "_query_devd_devices", return_value=[placeholder]):
            with mock.patch.object(MODULE.time, "sleep", return_value=None):
                with self.assertRaisesRegex(RuntimeError, "only exposed missing-port placeholders"):
                    runner.resolve_device_id(False)

    def test_refresh_device_command_keeps_last_live_device_when_only_missing_port_placeholder_remains(self) -> None:
        runner = MODULE.FluxPurrRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=False,
        )
        runner._resolved_device_id = "serial-303a-1001-D0:CF:13:08:A1:48"
        placeholder = {
            "id": "serial-_dev_cu.usbmodem2111401",
            "portPath": "/dev/cu.usbmodem2111401",
            "identity": {"buildId": "native-serial-placeholder"},
        }

        with mock.patch.object(runner, "_query_devd_devices", return_value=[placeholder]):
            with mock.patch.object(MODULE.time, "sleep", return_value=None):
                refreshed = runner._refresh_device_command(
                    [
                        "/tmp/flux-purr",
                        "--devd",
                        "http://127.0.0.1:62610",
                        "--json",
                        "runtime",
                        "set",
                        "--device",
                        "serial-stale",
                        "--heater-enabled",
                        "false",
                    ]
                )

        assert refreshed is not None
        self.assertEqual(refreshed[refreshed.index("--device") + 1], "serial-303a-1001-D0:CF:13:08:A1:48")
        self.assertEqual(runner._resolved_device_id, "serial-303a-1001-D0:CF:13:08:A1:48")

    def test_clear_transient_temperature_warning_waits_for_fault_to_clear(self) -> None:
        recorded: list[list[str]] = []

        class ClearingRunner(MODULE.FluxPurrRunner):
            def __init__(self) -> None:
                super().__init__(
                    flux_purr_bin=Path("/tmp/flux-purr"),
                    devd_url="http://127.0.0.1:62610",
                    authorized_port="/dev/cu.usbmodem2111401",
                    source_id="f293cc",
                    source_url="http://192.168.31.224",
                    dry_run=False,
                    auto_recover_source=False,
                )
                self._status_payloads = iter(
                    [
                        {"mode": "fault", "heaterFaultReason": "sensor-glitch"},
                        {
                            "mode": "idle",
                            "heaterFaultReason": None,
                            "faultAttentionPending": True,
                        },
                    ]
                )

            def resolve_device_id(self, dry_run_override: bool = False) -> str:
                return "fp-device-1"

            def run_json_command(
                self,
                cmd: list[str],
                *,
                retry_with_source_recovery: bool = False,
            ) -> dict[str, object]:
                recorded.append(cmd)
                if "--fault-attention-acknowledged" in cmd:
                    return {"ok": True}
                if cmd[4:6] == ["runtime", "set"]:
                    return {"ok": True}
                if cmd[4] == "status":
                    return next(self._status_payloads)
                if cmd[4:7] == ["thermal", "profile", "clear-preview"]:
                    return {"ok": True}
                raise AssertionError(cmd)

        runner = ClearingRunner()
        with mock.patch.object(MODULE.time, "sleep", return_value=None):
            self.assertTrue(runner.clear_transient_temperature_warning())

        self.assertEqual(sum(1 for cmd in recorded if cmd[4] == "status"), 2)
        self.assertTrue(any("--fault-attention-acknowledged" in cmd for cmd in recorded))
        self.assertTrue(any(cmd[4:7] == ["thermal", "profile", "clear-preview"] for cmd in recorded))

    def test_self_test_retries_after_transient_temperature_fault(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)

            class RetryRunner(MODULE.FluxPurrRunner):
                def __init__(self) -> None:
                    super().__init__(
                        flux_purr_bin=Path("/tmp/flux-purr"),
                        devd_url="http://127.0.0.1:62610",
                        authorized_port="/dev/cu.usbmodem2111401",
                        source_id="f293cc",
                        source_url="http://192.168.31.224",
                        dry_run=False,
                        auto_recover_source=False,
                    )
                    self.calls = 0
                    self.clears = 0

                def resolve_device_id(self, dry_run_override: bool = False) -> str:
                    return "fp-device-1"

                def clear_transient_temperature_warning(self) -> bool:
                    self.clears += 1
                    return True

                def run_json_command(
                    self,
                    cmd: list[str],
                    *,
                    retry_with_source_recovery: bool = False,
                ) -> dict[str, object]:
                    self.calls += 1
                    output_dir = Path(cmd[cmd.index("--output-dir") + 1])
                    run_dir = output_dir / f"thermal-run-{self.calls}"
                    run_dir.mkdir(parents=True, exist_ok=True)
                    summary_path = run_dir / "run.json"
                    samples_path = run_dir / "samples.ndjson"
                    if self.calls == 1:
                        summary = {
                            "runId": "fault-run",
                            "applied": [
                                {
                                    "targetTempC": 60,
                                    "stopReason": "latched_fault",
                                    "terminalRuntimeDropReason": "latched_fault",
                                    "analysis": {},
                                    "fullSpeedToStable": {"limitMs": 10_000},
                                }
                            ],
                            "validation": {"failures": [{"targetTempC": 60, "reason": "latched_fault"}]},
                            "files": {
                                "summaryPath": str(summary_path),
                                "samplesPath": str(samples_path),
                            },
                        }
                        samples_path.write_text(
                            json.dumps({"status": {"heaterFaultReason": "sensor-glitch"}}) + "\n",
                            encoding="utf-8",
                        )
                    else:
                        summary = {
                            "runId": "success-run",
                            "applied": [
                                {
                                    "targetTempC": 60,
                                    "stopReason": "completed",
                                    "terminalRuntimeDropReason": None,
                                    "analysis": {},
                                    "fullSpeedToStable": {"limitMs": 10_000},
                                }
                            ],
                            "validation": {"failures": []},
                            "files": {
                                "summaryPath": str(summary_path),
                                "samplesPath": str(samples_path),
                            },
                        }
                        samples_path.write_text(
                            json.dumps({"status": {"heaterFaultReason": None}}) + "\n",
                            encoding="utf-8",
                        )
                    summary_path.write_text(json.dumps(summary), encoding="utf-8")
                    return {
                        "files": {
                            "summaryPath": str(summary_path),
                            "runDir": str(run_dir),
                            "samplesPath": str(samples_path),
                        }
                    }

            runner = RetryRunner()
            result = runner.self_test(
                targets_c=[60],
                hold_seconds=12,
                output_dir=root / "scout",
                evaluation_mode=MODULE.EVALUATION_MODE_TUNING_SCOUT,
                cooldown_temp_c=35.0,
                stage_timeout_seconds=60,
                warmup_timeout_seconds=45,
                cooldown_timeout_seconds=60,
            )

        self.assertEqual(runner.clears, 1)
        self.assertEqual(runner.calls, 2)
        self.assertEqual(result.summary["runId"], "success-run")

    def test_self_test_pauses_after_three_consecutive_alarm_affected_runs(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)

            class AlarmRunner(MODULE.FluxPurrRunner):
                def __init__(self) -> None:
                    super().__init__(
                        flux_purr_bin=Path("/tmp/flux-purr"),
                        devd_url="http://127.0.0.1:62610",
                        authorized_port="/dev/cu.usbmodem2111401",
                        source_id="f293cc",
                        source_url="http://192.168.31.224",
                        dry_run=False,
                        auto_recover_source=False,
                    )
                    self.calls = 0
                    self.clears = 0

                def resolve_device_id(self, dry_run_override: bool = False) -> str:
                    return "fp-device-1"

                def clear_transient_temperature_warning(self) -> bool:
                    self.clears += 1
                    return True

                def run_json_command(
                    self,
                    cmd: list[str],
                    *,
                    retry_with_source_recovery: bool = False,
                ) -> dict[str, object]:
                    self.calls += 1
                    output_dir = Path(cmd[cmd.index("--output-dir") + 1])
                    run_dir = output_dir / f"thermal-run-{self.calls}"
                    run_dir.mkdir(parents=True, exist_ok=True)
                    summary_path = run_dir / "run.json"
                    samples_path = run_dir / "samples.ndjson"
                    summary = {
                        "runId": f"alarm-run-{self.calls}",
                        "applied": [
                            {
                                "targetTempC": 60,
                                "stopReason": "latched_fault",
                                "terminalRuntimeDropReason": "latched_fault",
                                "analysis": {},
                                "fullSpeedToStable": {"limitMs": 10_000},
                            }
                        ],
                        "validation": {
                            "failures": [{"targetTempC": 60, "reason": "sensor-glitch"}],
                        },
                        "files": {
                            "summaryPath": str(summary_path),
                            "samplesPath": str(samples_path),
                        },
                    }
                    summary_path.write_text(json.dumps(summary), encoding="utf-8")
                    samples_path.write_text(
                        json.dumps(
                            {
                                "status": {
                                    "heaterFaultReason": "sensor-glitch",
                                    "faultAttentionPending": True,
                                }
                            }
                        )
                        + "\n",
                        encoding="utf-8",
                    )
                    return {
                        "files": {
                            "summaryPath": str(summary_path),
                            "runDir": str(run_dir),
                            "samplesPath": str(samples_path),
                        }
                    }

            runner = AlarmRunner()

            runner.self_test(
                targets_c=[60],
                hold_seconds=12,
                output_dir=root / "alarm-scout-1",
                evaluation_mode=MODULE.EVALUATION_MODE_TUNING_SCOUT,
                cooldown_temp_c=35.0,
                stage_timeout_seconds=60,
                warmup_timeout_seconds=45,
                cooldown_timeout_seconds=60,
            )

            with self.assertRaises(MODULE.AlarmInterventionRequired) as raised:
                runner.self_test(
                    targets_c=[60],
                    hold_seconds=12,
                    output_dir=root / "alarm-scout-2",
                    evaluation_mode=MODULE.EVALUATION_MODE_TUNING_SCOUT,
                    cooldown_temp_c=35.0,
                    stage_timeout_seconds=60,
                    warmup_timeout_seconds=45,
                    cooldown_timeout_seconds=60,
                )

        self.assertEqual(len(raised.exception.attempts), 3)
        self.assertTrue(all("sensor-glitch" in attempt["faultReasons"] for attempt in raised.exception.attempts))
        self.assertGreaterEqual(runner.clears, 1)

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
        self.assertIsNone(MODULE.DEFAULT_MAX_TUNING_ROUNDS)
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

    def test_predicts_later_handoff_when_high_target_floor_is_nearly_saturated(self) -> None:
        point = make_point(
            220,
            brakeDistanceCentiC=701,
            approachPowerPermille=1000,
            approachFloorPowerPermille=958,
        )

        predicted = MODULE.predict_next_point(
            point,
            {
                "failureClass": "missed_lower_band_before_limit",
                "temperatureGapC": 0.35,
            },
        )

        self.assertLess(predicted["brakeDistanceCentiC"], point["brakeDistanceCentiC"])
        self.assertEqual(predicted["approachFloorPowerPermille"], point["approachFloorPowerPermille"])

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

    def test_high_target_low_side_hold_starved_scout_adds_more_heat_to_prediction(self) -> None:
        current_point = make_point(
            220,
            brakeDistanceCentiC=409,
            approachPowerPermille=1000,
            approachFloorPowerPermille=1000,
            holdEntryCentiC=159,
            holdPowerPermille=970,
            holdReheatPowerPermille=930,
        )
        current = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [current_point])
        stage = {
            "targetTempC": 220,
            "stopReason": "full_speed_to_stable_timeout",
            "analysis": {
                "firstHoldTempC": 218.52,
                "firstHoldErrorC": 1.48,
                "holdMedianOutputPermille": 0,
                "holdP90OutputPermille": 1000,
            },
            "fullSpeedToStable": {"warmupExitedAtMs": 78_951, "limitMs": 5_000},
        }
        samples = [
            {"t": 78.951, "temp": 214.0},
            {"t": 83.000, "temp": 218.9},
            {"t": 84.000, "temp": 218.4},
        ]

        variants = MODULE.build_candidate_variants(current, current, 220, stage, samples)

        self.assertEqual([variant.name for variant in variants], ["current", "stable_window_broke_low"])
        predicted = MODULE.explicit_point(variants[1].profile, 220)
        assert predicted is not None
        self.assertLess(predicted["brakeDistanceCentiC"], current_point["brakeDistanceCentiC"])
        self.assertLess(predicted["holdEntryCentiC"], current_point["holdEntryCentiC"])
        self.assertGreater(predicted["holdPowerPermille"], current_point["holdPowerPermille"])
        self.assertGreater(predicted["holdReheatPowerPermille"], current_point["holdReheatPowerPermille"])

    def test_high_target_stable_window_broke_high_advances_hold_and_cools_reheat(self) -> None:
        point = make_point(
            220,
            brakeDistanceCentiC=488,
            approachFloorPowerPermille=979,
            approachDampingExponentPermille=230,
            approachLeadTicks=1,
            holdEntryCentiC=142,
            holdReheatPowerPermille=970,
        )

        predicted = MODULE.predict_next_point(
            point,
            {
                "failureClass": "stable_window_broke_high",
                "temperatureGapC": 0.19,
            },
        )

        self.assertGreater(predicted["brakeDistanceCentiC"], point["brakeDistanceCentiC"])
        self.assertLess(predicted["approachFloorPowerPermille"], point["approachFloorPowerPermille"])
        self.assertGreater(predicted["approachDampingExponentPermille"], point["approachDampingExponentPermille"])
        self.assertGreater(predicted["approachLeadTicks"], point["approachLeadTicks"])
        self.assertGreater(predicted["holdEntryCentiC"], point["holdEntryCentiC"])
        self.assertLess(predicted["holdReheatPowerPermille"], point["holdReheatPowerPermille"])

    def test_completed_high_target_adds_conservative_more_brake_candidate(self) -> None:
        current = MODULE.sparse_profile(
            {"tempFilterAlphaPermille": 700},
            [
                make_point(
                    220,
                    brakeDistanceCentiC=393,
                    approachDampingExponentPermille=110,
                    approachFloorPowerPermille=1000,
                    approachPowerPermille=1000,
                    holdEntryCentiC=150,
                    holdReheatPowerPermille=1000,
                )
            ],
        )
        stage = {
            "targetTempC": 220,
            "stopReason": "completed",
            "fullSpeedToStable": {
                "warmupExitedAtMs": 34_015,
                "settleTimeMs": 2_662,
                "limitMs": 5_000,
            },
        }

        variants = MODULE.build_candidate_variants(current, current, 220, stage, [])

        self.assertEqual([variant.name for variant in variants], ["current", "within_gate_more_brake"])
        predicted = MODULE.explicit_point(variants[1].profile, 220)
        assert predicted is not None
        current_point = MODULE.explicit_point(current, 220)
        assert current_point is not None
        self.assertGreater(predicted["brakeDistanceCentiC"], current_point["brakeDistanceCentiC"])
        self.assertGreater(
            predicted["approachDampingExponentPermille"],
            current_point["approachDampingExponentPermille"],
        )
        self.assertLess(
            predicted["approachFloorPowerPermille"],
            current_point["approachFloorPowerPermille"],
        )
        self.assertGreater(predicted["holdEntryCentiC"], current_point["holdEntryCentiC"])

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

    def test_candidate_score_prefers_band_progress_over_repeated_late_current(self) -> None:
        def write_samples(path: Path, samples: list[tuple[int, float]]) -> None:
            path.write_text(
                "".join(
                    json.dumps(
                        {
                            "targetTempC": 220,
                            "elapsedMs": elapsed_ms,
                            "status": {"currentTempC": temp_c},
                        }
                    )
                    + "\n"
                    for elapsed_ms, temp_c in samples
                ),
                encoding="utf-8",
            )

        def summary(samples_path: Path, settle_time_ms: int | None) -> dict[str, object]:
            return {
                "files": {"samplesPath": str(samples_path)},
                "applied": [
                    {
                        "targetTempC": 220,
                        "stopReason": "full_speed_to_stable_timeout",
                        "maxOvershootC": 0.0,
                        "analysis": {"approachCurveMeanAbsErrorC": 1.0},
                        "fullSpeedToStable": {
                            "warmupExitedAtMs": 32_000,
                            "settleTimeMs": settle_time_ms,
                            "limitMs": 5_000,
                        },
                        "terminalRuntimeDropReason": None,
                    }
                ],
                "validation": {"failures": [{"targetTempC": 220, "reason": "incomplete_stage"}]},
            }

        with tempfile.TemporaryDirectory() as tmpdir:
            root = Path(tmpdir)
            late_current_samples = root / "late-current.ndjson"
            progressed_candidate_samples = root / "progressed-candidate.ndjson"
            write_samples(late_current_samples, [(32_000, 212.0), (37_000, 218.4), (37_100, 218.7)])
            write_samples(progressed_candidate_samples, [(32_000, 212.0), (36_800, 218.6), (37_050, 218.4)])

            late_current = summary(late_current_samples, 5_100)
            progressed_candidate = summary(progressed_candidate_samples, None)

            self.assertLess(
                MODULE.candidate_score(progressed_candidate, 220),
                MODULE.candidate_score(late_current, 220),
            )

    def test_budget_helpers_enforce_per_target_limit(self) -> None:
        self.assertEqual(MODULE.budget_remaining_seconds(100.0, 1200, now_monotonic=1250.0), 50)
        self.assertTrue(MODULE.budget_exhausted(100.0, 1200, now_monotonic=1300.0))
        self.assertIsNone(MODULE.step_timeouts_for_budget(80, 60))
        self.assertEqual(MODULE.cooldown_threshold(60), 35.0)
        self.assertEqual(MODULE.cooldown_threshold(140), 100.0)
        self.assertEqual(MODULE.step_timeouts_for_budget(894, 60), (714, 180, 180))

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

    def test_verify_isolapurr_output_disabled_accepts_runtime_gate_off(self) -> None:
        disabled = make_power_show_payload(
            usb_c_power_enabled=True,
            output_enabled=False,
            usb_status="ok",
            usb_current_ma=0,
            usb_power_mw=0,
        )
        self.assertEqual(MODULE.verify_isolapurr_output_disabled(disabled), 1_000)

    def test_verify_isolapurr_output_disabled_rejects_live_usb_c_output(self) -> None:
        with self.assertRaisesRegex(RuntimeError, "still sourcing power"):
            MODULE.verify_isolapurr_output_disabled(
                make_power_show_payload(
                    usb_c_power_enabled=True,
                    output_enabled=False,
                    usb_status="ok",
                    usb_current_ma=41,
                    usb_power_mw=497,
                )
            )

    def test_recover_source_output_toggles_runtime_output_gate(self) -> None:
        runner = MODULE.FluxPurrRunner(
            flux_purr_bin=Path("/tmp/flux-purr"),
            devd_url="http://127.0.0.1:62610",
            authorized_port="/dev/cu.usbmodem2111401",
            source_id="f293cc",
            source_url="http://192.168.31.224",
            dry_run=False,
            auto_recover_source=True,
        )
        recorded: list[list[str]] = []
        responses = iter(
            [
                {"ok": True},
                make_power_show_payload(
                    usb_c_power_enabled=True,
                    output_enabled=False,
                    usb_status="ok",
                    usb_current_ma=0,
                    usb_power_mw=0,
                ),
                {"ok": True},
                make_power_show_payload(sample_uptime_ms=10_000),
                make_power_show_payload(sample_uptime_ms=10_600),
            ]
        )

        def fake_run_subprocess_json(cmd: list[str]) -> dict[str, object]:
            recorded.append(list(cmd))
            return next(responses)

        with mock.patch.object(runner, "run_subprocess_json", side_effect=fake_run_subprocess_json):
            with mock.patch.object(MODULE.time, "sleep", return_value=None):
                with mock.patch.object(Path, "exists", return_value=True):
                    runner.recover_source_output()

        self.assertEqual(
            recorded,
            [
                [
                    "isolapurr",
                    "power",
                    "runtime",
                    "output",
                    "--url",
                    "http://192.168.31.224",
                    "--enabled",
                    "false",
                    "--json",
                ],
                ["isolapurr", "power", "show", "--url", "http://192.168.31.224", "--json"],
                [
                    "isolapurr",
                    "power",
                    "runtime",
                    "output",
                    "--url",
                    "http://192.168.31.224",
                    "--enabled",
                    "true",
                    "--json",
                ],
                ["isolapurr", "power", "show", "--url", "http://192.168.31.224", "--json"],
                ["isolapurr", "power", "show", "--url", "http://192.168.31.224", "--json"],
            ],
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
            max_tuning_rounds=None,
            scout_hold_seconds=12,
            confirm_hold_seconds=60,
            dry_run=False,
        )
        self.assertEqual(planned["scope"]["flagshipTargetsC"], [60, 140, 220])
        self.assertEqual(planned["scope"]["roundLimitMode"], "budget_only")
        self.assertIsNone(planned["scope"]["maxTuningRounds"])
        self.assertEqual(planned["deviceConnection"]["fluxPurr"]["authorizedPort"], "/dev/cu.usbmodem2111401")
        self.assertIn("Do not run the full temperature ladder.", planned["forbiddenOperations"])
        self.assertIn(
            "Continue targeted tuning rounds until the per-target budget is exhausted or the target completes.",
            planned["executionWhitelist"]["perTargetWorkflow"],
        )
        self.assertEqual(
            planned["executionWhitelist"]["allowedResults"],
            ["completed", "not_converged", "budget_exhausted", "environment_blocked"],
        )
        self.assertEqual(len(planned["powerCycleRecovery"]["steps"]), 7)

    def test_target_keeps_tuning_after_failed_hold_confirm_while_budget_remains(self) -> None:
        class StubRunner:
            def __init__(self, module: object) -> None:
                self.module = module
                self.self_test_calls: list[dict[str, object]] = []
                self.retune_calls = 0

            def self_test(
                self,
                *,
                seed_profile_file: Path | None = None,
                candidate_profile_files: list[Path] | None = None,
                targets_c: list[int],
                hold_seconds: int,
                output_dir: Path,
                evaluation_mode: str,
                cooldown_temp_c: float,
                stage_timeout_seconds: int,
                warmup_timeout_seconds: int,
                cooldown_timeout_seconds: int,
            ) -> object:
                call = {
                    "seed_profile_file": seed_profile_file,
                    "candidate_profile_files": candidate_profile_files,
                    "targets_c": list(targets_c),
                    "hold_seconds": hold_seconds,
                    "output_dir": output_dir,
                    "evaluation_mode": evaluation_mode,
                    "stage_timeout_seconds": stage_timeout_seconds,
                    "warmup_timeout_seconds": warmup_timeout_seconds,
                    "cooldown_timeout_seconds": cooldown_timeout_seconds,
                }
                self.self_test_calls.append(call)
                stage_target = int(targets_c[0])
                files = {
                    "summaryPath": str(output_dir / "run.json"),
                    "samplesPath": str(output_dir / "samples.ndjson"),
                }
                source = {
                    "selectedMode": "100w",
                    "resolvedBank": "pps5a",
                    "detectedSourceClass": "pps5a",
                }
                sample = {
                    "targetTempC": stage_target,
                    "elapsedMs": 100,
                    "phase": "warmup",
                    "status": {
                        "currentTempC": 25.0,
                        "heaterOutputPercent": 100,
                        "heaterPhysicalOutputPercent": 100,
                    },
                }
                if candidate_profile_files is not None:
                    batch_run = {
                        "runId": f"candidate-{len(self.self_test_calls)}",
                        "source": source,
                        "parameters": {"candidateProfileFile": str(candidate_profile_files[0])},
                        "files": files,
                        "applied": [
                            {
                                "targetTempC": stage_target,
                                "stopReason": "completed",
                                "maxOvershootC": 1.1,
                                "holdPeakToPeakC": 1.2,
                                "analysis": {},
                                "fullSpeedToStable": {
                                    "warmupExitedAtMs": 1_000,
                                    "settleTimeMs": 8_000,
                                    "limitMs": 10_000,
                                },
                            }
                        ],
                        "validation": {"failures": []},
                    }
                    summary = {"runs": [batch_run]}
                elif evaluation_mode == self.module.EVALUATION_MODE_HOLD_CONFIRM:
                    confirm_index = sum(
                        1
                        for item in self.self_test_calls
                        if item["evaluation_mode"] == self.module.EVALUATION_MODE_HOLD_CONFIRM
                    )
                    passed = confirm_index >= 2
                    stop_reason = "completed" if passed else "timeout"
                    validation = {"failures": [], "passed": passed}
                    if not passed:
                        validation = {
                            "failures": [{"targetTempC": stage_target, "reason": "hold_peak_to_peak"}],
                            "passed": False,
                        }
                    summary = {
                        "runId": f"confirm-{confirm_index}",
                        "source": source,
                        "files": files,
                        "applied": [
                            {
                                "targetTempC": stage_target,
                                "stopReason": stop_reason,
                                "maxOvershootC": 1.4,
                                "holdPeakToPeakC": 3.4 if not passed else 1.4,
                                "analysis": {},
                                "fullSpeedToStable": {
                                    "warmupExitedAtMs": 1_000,
                                    "settleTimeMs": 8_000,
                                    "limitMs": 10_000,
                                },
                            }
                        ],
                        "validation": validation,
                    }
                else:
                    summary = {
                        "runId": f"scout-{len(self.self_test_calls)}",
                        "source": source,
                        "files": files,
                        "applied": [
                            {
                                "targetTempC": stage_target,
                                "stopReason": "full_speed_to_stable_timeout",
                                "maxOvershootC": 1.8,
                                "holdPeakToPeakC": 2.2,
                                "analysis": {},
                                "fullSpeedToStable": {
                                    "warmupExitedAtMs": 1_000,
                                    "settleTimeMs": None,
                                    "limitMs": 10_000,
                                    "failureReason": "full_speed_to_stable_timeout",
                                },
                            }
                        ],
                        "validation": {
                            "failures": [{"targetTempC": stage_target, "reason": "full_speed_to_stable"}],
                            "passed": False,
                        },
                    }
                output_dir.mkdir(parents=True, exist_ok=True)
                Path(files["summaryPath"]).write_text(json.dumps(summary), encoding="utf-8")
                Path(files["samplesPath"]).write_text(json.dumps(sample) + "\n", encoding="utf-8")

                class Result:
                    pass

                result = Result()
                result.summary = summary
                result.summary_path = Path(files["summaryPath"])
                result.run_dir = output_dir
                return result

            def retune(self, run_dir: Path, target_temp_c: int) -> tuple[dict[str, object], Path]:
                self.retune_calls += 1
                candidate = MODULE.sparse_profile(
                    {"tempFilterAlphaPermille": 700},
                    [make_point(int(target_temp_c), brakeDistanceCentiC=1_100 + self.retune_calls * 10)],
                )
                path = run_dir / f"retuned-{self.retune_calls}.json"
                path.write_text(json.dumps(candidate), encoding="utf-8")
                return candidate, path

            def disarm_and_clear_preview(self) -> None:
                return None

        runner = StubRunner(MODULE)
        profile = MODULE.sparse_profile({"tempFilterAlphaPermille": 700}, [make_point(60)])
        with tempfile.TemporaryDirectory() as tmpdir:
            updated, entry = MODULE.tune_flagship_target(
                runner,
                profile,
                60,
                [60],
                Path(tmpdir),
                per_target_budget_seconds=1_200,
                max_tuning_rounds=None,
                scout_hold_seconds=12,
                confirm_hold_seconds=60,
            )

        self.assertEqual(entry["budgetOutcome"], "completed")
        self.assertEqual(
            [call["evaluation_mode"] for call in runner.self_test_calls],
            [
                MODULE.EVALUATION_MODE_TUNING_SCOUT,
                MODULE.EVALUATION_MODE_TUNING_SCOUT,
                MODULE.EVALUATION_MODE_HOLD_CONFIRM,
                MODULE.EVALUATION_MODE_TUNING_SCOUT,
                MODULE.EVALUATION_MODE_TUNING_SCOUT,
                MODULE.EVALUATION_MODE_HOLD_CONFIRM,
            ],
        )
        self.assertEqual(sum(1 for round_item in entry["rounds"] if round_item["attemptType"] == "hold_confirm"), 2)
        self.assertTrue(all(call["stage_timeout_seconds"] == 180 for call in runner.self_test_calls))
        self.assertTrue(all(call["warmup_timeout_seconds"] == 180 for call in runner.self_test_calls))
        self.assertEqual(updated["points"][0]["targetTempC"], 60)

    def test_failed_hold_confirm_reseeds_next_round_from_confirm_evidence(self) -> None:
        initial_profile = MODULE.sparse_profile(
            {"tempFilterAlphaPermille": 700},
            [
                make_point(
                    220,
                    brakeDistanceCentiC=488,
                    approachFloorPowerPermille=979,
                    approachDampingExponentPermille=230,
                    approachLeadTicks=1,
                    holdEntryCentiC=142,
                    holdReheatPowerPermille=970,
                )
            ],
        )

        class StubRunner:
            def __init__(self, module: object) -> None:
                self.module = module
                self.scout_seed_points: list[dict[str, object]] = []
                self.call_index = 0

            def self_test(
                self,
                *,
                seed_profile_file: Path | None = None,
                candidate_profile_files: list[Path] | None = None,
                targets_c: list[int],
                hold_seconds: int,
                output_dir: Path,
                evaluation_mode: str,
                cooldown_temp_c: float,
                stage_timeout_seconds: int,
                warmup_timeout_seconds: int,
                cooldown_timeout_seconds: int,
            ) -> object:
                self.call_index += 1
                if evaluation_mode == self.module.EVALUATION_MODE_TUNING_SCOUT and seed_profile_file is not None:
                    seed = json.loads(seed_profile_file.read_text(encoding="utf-8"))
                    self.scout_seed_points.append(MODULE.explicit_point(seed, 220))

                output_dir.mkdir(parents=True, exist_ok=True)
                summary_path = output_dir / "run.json"
                samples_path = output_dir / "samples.ndjson"
                files = {
                    "summaryPath": str(summary_path),
                    "samplesPath": str(samples_path),
                }
                source = {
                    "selectedMode": "100w",
                    "resolvedBank": "pps5a",
                    "detectedSourceClass": "pps5a",
                }

                def write_samples(samples: list[dict[str, object]]) -> None:
                    samples_path.write_text(
                        "".join(json.dumps(sample, ensure_ascii=False) + "\n" for sample in samples),
                        encoding="utf-8",
                    )

                if candidate_profile_files is not None:
                    candidate_profile = json.loads(candidate_profile_files[0].read_text(encoding="utf-8"))
                    write_samples(
                        [
                            {
                                "targetTempC": 220,
                                "elapsedMs": 32_000,
                                "phase": "warmup",
                                "status": {"currentTempC": 217.0, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 100},
                            },
                            {
                                "targetTempC": 220,
                                "elapsedMs": 35_000,
                                "phase": "hold",
                                "status": {"currentTempC": 220.2, "heaterOutputPercent": 0, "heaterPhysicalOutputPercent": 0},
                            },
                            {
                                "targetTempC": 220,
                                "elapsedMs": 36_500,
                                "phase": "hold",
                                "status": {"currentTempC": 220.4, "heaterOutputPercent": 0, "heaterPhysicalOutputPercent": 0},
                            },
                        ]
                    )
                    summary = {
                        "runs": [
                            {
                                "runId": f"candidate-{self.call_index}",
                                "source": source,
                                "parameters": {"candidateProfileFile": str(candidate_profile_files[0])},
                                "candidateProfile": candidate_profile,
                                "files": files,
                                "applied": [
                                    {
                                        "targetTempC": 220,
                                        "stopReason": "completed",
                                        "maxOvershootC": 1.4,
                                        "holdPeakToPeakC": 1.8,
                                        "analysis": {},
                                        "fullSpeedToStable": {
                                            "warmupExitedAtMs": 31_500,
                                            "settleTimeMs": 3_500,
                                            "limitMs": 5_000,
                                        },
                                    }
                                ],
                                "validation": {"failures": []},
                            }
                        ]
                    }
                elif evaluation_mode == self.module.EVALUATION_MODE_HOLD_CONFIRM:
                    write_samples(
                        [
                            {
                                "targetTempC": 220,
                                "elapsedMs": 31_500,
                                "phase": "warmup",
                                "status": {"currentTempC": 217.0, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 100},
                            },
                            {
                                "targetTempC": 220,
                                "elapsedMs": 35_100,
                                "phase": "hold",
                                "status": {"currentTempC": 220.3, "heaterOutputPercent": 0, "heaterPhysicalOutputPercent": 0},
                            },
                            {
                                "targetTempC": 220,
                                "elapsedMs": 36_200,
                                "phase": "hold",
                                "status": {"currentTempC": 221.69, "heaterOutputPercent": 0, "heaterPhysicalOutputPercent": 0},
                            },
                        ]
                    )
                    summary = {
                        "runId": f"confirm-{self.call_index}",
                        "source": source,
                        "files": files,
                        "applied": [
                            {
                                "targetTempC": 220,
                                "stopReason": "full_speed_to_stable_timeout",
                                "maxOvershootC": 1.69,
                                "holdPeakToPeakC": 2.15,
                                "analysis": {},
                                "fullSpeedToStable": {
                                    "warmupExitedAtMs": 31_500,
                                    "settleTimeMs": None,
                                    "limitMs": 5_000,
                                    "failureReason": "full_speed_to_stable_timeout",
                                },
                            }
                        ],
                        "validation": {
                            "failures": [
                                {
                                    "targetTempC": 220,
                                    "reason": "full_speed_to_stable_missing",
                                }
                            ],
                            "passed": False,
                        },
                    }
                else:
                    write_samples(
                        [
                            {
                                "targetTempC": 220,
                                "elapsedMs": 31_500,
                                "phase": "warmup",
                                "status": {"currentTempC": 217.0, "heaterOutputPercent": 100, "heaterPhysicalOutputPercent": 100},
                            },
                            {
                                "targetTempC": 220,
                                "elapsedMs": 35_000,
                                "phase": "hold",
                                "status": {"currentTempC": 220.2, "heaterOutputPercent": 0, "heaterPhysicalOutputPercent": 0},
                            },
                            {
                                "targetTempC": 220,
                                "elapsedMs": 36_000,
                                "phase": "hold",
                                "status": {"currentTempC": 220.5, "heaterOutputPercent": 0, "heaterPhysicalOutputPercent": 0},
                            },
                        ]
                    )
                    summary = {
                        "runId": f"scout-{self.call_index}",
                        "source": source,
                        "files": files,
                        "applied": [
                            {
                                "targetTempC": 220,
                                "stopReason": "completed",
                                "maxOvershootC": 1.5,
                                "holdPeakToPeakC": 1.9,
                                "analysis": {},
                                "fullSpeedToStable": {
                                    "warmupExitedAtMs": 31_500,
                                    "settleTimeMs": 3_500,
                                    "limitMs": 5_000,
                                },
                            }
                        ],
                        "validation": {"failures": [], "passed": True},
                    }

                summary_path.write_text(json.dumps(summary), encoding="utf-8")

                class Result:
                    pass

                result = Result()
                result.summary = summary
                result.summary_path = summary_path
                result.run_dir = output_dir
                result.samples_path = samples_path
                return result

            def retune(self, run_dir: Path, target_temp_c: int) -> tuple[dict[str, object], Path]:
                path = run_dir / "retuned.json"
                path.write_text(json.dumps(initial_profile), encoding="utf-8")
                return initial_profile, path

            def resolve_device_id(self, dry_run_override: bool = False) -> str:
                return "fp-device-1"

        runner = StubRunner(MODULE)
        with tempfile.TemporaryDirectory() as tmpdir:
            workspace = Path(tmpdir)
            updated, entry = MODULE.tune_flagship_target(
                runner,
                initial_profile,
                220,
                [220],
                workspace,
                per_target_budget_seconds=1_200,
                max_tuning_rounds=2,
                scout_hold_seconds=12,
                confirm_hold_seconds=60,
            )

            reseed = json.loads((workspace / "hold-confirm-1-reseed.json").read_text(encoding="utf-8"))

        first_seed = runner.scout_seed_points[0]
        second_seed = runner.scout_seed_points[1]
        assert first_seed is not None
        assert second_seed is not None
        reseeded_point = MODULE.explicit_point(reseed, 220)
        assert reseeded_point is not None
        self.assertEqual(entry["budgetOutcome"], "not_converged")
        self.assertGreater(second_seed["brakeDistanceCentiC"], first_seed["brakeDistanceCentiC"])
        self.assertGreater(second_seed["holdEntryCentiC"], first_seed["holdEntryCentiC"])
        self.assertLess(second_seed["holdReheatPowerPermille"], first_seed["holdReheatPowerPermille"])
        self.assertEqual(reseeded_point["brakeDistanceCentiC"], second_seed["brakeDistanceCentiC"])
        self.assertGreaterEqual(updated["points"][0]["brakeDistanceCentiC"], second_seed["brakeDistanceCentiC"])

    def test_more_heat_delays_hold_entry(self) -> None:
        point = make_point(60, holdEntryCentiC=240, approachFloorPowerPermille=160)
        mutated = MODULE.mutate_more_heat(point, 60)
        self.assertLess(mutated["holdEntryCentiC"], point["holdEntryCentiC"])
        self.assertGreater(mutated["approachFloorPowerPermille"], point["approachFloorPowerPermille"])

    def test_failed_high_target_hold_confirm_low_side_reseeds_hold_heat(self) -> None:
        profile = MODULE.sparse_profile(
            {"tempFilterAlphaPermille": 700},
            [
                make_point(
                    220,
                    brakeDistanceCentiC=524,
                    approachPowerPermille=1000,
                    approachFloorPowerPermille=1000,
                    holdEntryCentiC=159,
                    holdPowerPermille=970,
                    holdReheatPowerPermille=930,
                )
            ],
        )

        with tempfile.TemporaryDirectory() as tmpdir:
            samples_path = Path(tmpdir) / "samples.ndjson"
            samples_path.write_text(
                "".join(
                    json.dumps(
                        {
                            "targetTempC": 220,
                            "elapsedMs": elapsed_ms,
                            "phase": phase,
                            "status": {"currentTempC": temp_c},
                        }
                    )
                    + "\n"
                    for elapsed_ms, phase, temp_c in [
                        (31_848, "warmup", 217.2),
                        (35_099, "hold", 218.76),
                        (36_000, "hold", 218.26),
                    ]
                ),
                encoding="utf-8",
            )
            confirm_summary = {
                "files": {"samplesPath": str(samples_path)},
                "applied": [
                    {
                        "targetTempC": 220,
                        "stopReason": "full_speed_to_stable_timeout",
                        "analysis": {
                            "holdMedianOutputPermille": 0,
                            "holdP90OutputPermille": 0,
                        },
                        "fullSpeedToStable": {
                            "warmupExitedAtMs": 31_848,
                            "settleTimeMs": None,
                            "limitMs": 5_000,
                            "failureReason": "full_speed_to_stable_timeout",
                        },
                        "terminalRuntimeDropReason": None,
                    }
                ],
                "validation": {"failures": [{"targetTempC": 220, "reason": "incomplete_stage"}]},
            }

            reseeded = MODULE.reseed_after_failed_hold_confirm(profile, 220, confirm_summary)

        current = MODULE.explicit_point(profile, 220)
        reseeded_point = MODULE.explicit_point(reseeded, 220)
        assert current is not None
        assert reseeded_point is not None
        self.assertLess(reseeded_point["brakeDistanceCentiC"], current["brakeDistanceCentiC"])
        self.assertLess(reseeded_point["holdEntryCentiC"], current["holdEntryCentiC"])
        self.assertGreater(reseeded_point["holdPowerPermille"], current["holdPowerPermille"])
        self.assertGreater(reseeded_point["holdReheatPowerPermille"], current["holdReheatPowerPermille"])

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
