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


REPO_ROOT = Path(__file__).resolve().parents[2]
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
DEFAULT_MAX_TUNING_ROUNDS: int | None = None
DEFAULT_SCOUT_HOLD_SECONDS = 12
DEFAULT_CONFIRM_HOLD_SECONDS = 60
DEFAULT_STAGE_TIMEOUT_SECONDS = 180
DEFAULT_WARMUP_TIMEOUT_SECONDS = 180
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


def step_timeouts_for_budget(remaining_seconds: int, hold_seconds: int) -> tuple[int, int, int] | None:
    del hold_seconds
    remaining = int(remaining_seconds)
    stage_timeout = DEFAULT_STAGE_TIMEOUT_SECONDS
    warmup_timeout = DEFAULT_WARMUP_TIMEOUT_SECONDS
    if remaining <= stage_timeout:
        return None
    # Remaining target budget may cap only pre-run cooldown waiting. The active
    # round itself keeps a fixed 180s timeout, and warmup remains an explicit
    # timeout contract instead of being synthesized from budget slack.
    cooldown_timeout = max(1, remaining - stage_timeout)
    return cooldown_timeout, stage_timeout, warmup_timeout


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


def verify_isolapurr_output_disabled(payload: dict[str, Any]) -> int | None:
    output_enabled = value_at_path(payload, "config", "runtime", "output_enabled")
    if output_enabled is not False:
        raise RuntimeError(
            f"isolapurr runtime output is still enabled while recovery expects power-off: {output_enabled}"
        )
    usb_status = value_at_path(payload, "diagnostics", "usb_c_actual", "status")
    usb_current_ma = value_at_path(payload, "diagnostics", "usb_c_actual", "current_ma")
    usb_power_mw = value_at_path(payload, "diagnostics", "usb_c_actual", "power_mw")
    if usb_status == "ok" and not (
        isinstance(usb_current_ma, (int, float))
        and isinstance(usb_power_mw, (int, float))
        and int(usb_current_ma) == 0
        and int(usb_power_mw) == 0
    ):
        raise RuntimeError(
            "isolapurr usb_c_actual is still sourcing power while recovery expects power-off: "
            f"status={usb_status} current_ma={usb_current_ma} power_mw={usb_power_mw}"
        )
    return source_usb_c_sample_uptime_ms(payload)


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
