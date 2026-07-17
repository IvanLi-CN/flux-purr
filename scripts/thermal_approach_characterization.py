#!/usr/bin/env python3

from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_TARGETS = [60, 80, 100, 120, 140, 160, 180, 220, 240]
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
SETTINGS_FIELDS = [
    "tempFilterAlphaPermille",
    "warmupReenterCentiC",
    "approachMaxTicks",
    "approachMinPowerRatioPermille",
    "autoAdjustableWorkingFloorMv",
    "heaterCurrentReserveMa",
]


class HttpError(RuntimeError):
    pass


def now_iso() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def now_ms() -> int:
    return int(time.time() * 1000)


def log(message: str) -> None:
    print(message, flush=True)


def parse_targets(value: str | None) -> list[int]:
    if not value:
        return list(DEFAULT_TARGETS)
    targets: list[int] = []
    for part in value.split(","):
        part = part.strip()
        if not part:
            continue
        targets.append(int(part))
    if not targets:
        raise ValueError("targets list is empty")
    return targets


def slugify(parts: list[str]) -> str:
    raw = "-".join(parts).lower()
    out = []
    for ch in raw:
        if ch.isalnum():
            out.append(ch)
        elif ch in ("-", "_"):
            out.append("-")
        else:
            out.append("-")
    slug = "".join(out).strip("-")
    while "--" in slug:
        slug = slug.replace("--", "-")
    return slug


def http_json(method: str, url: str, payload: dict[str, Any] | None = None) -> Any:
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode()
        headers["Content-Type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            body = resp.read().decode()
            return json.loads(body) if body else None
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode()
        raise HttpError(f"{method} {url} -> HTTP {exc.code}: {detail}") from exc


def is_timeout_like(exc: BaseException) -> bool:
    return isinstance(exc, TimeoutError | urllib.error.URLError)


def run_isolapurr_json(source_url: str, args: list[str]) -> Any:
    cmd = ["isolapurr", "--json", *args, "--url", source_url]
    last_error: BaseException | None = None
    for attempt in range(2):
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=4)
        except subprocess.TimeoutExpired as exc:
            last_error = exc
            if attempt == 1:
                break
            time.sleep(0.5)
            continue
        if proc.returncode == 0:
            return json.loads(proc.stdout)
        last_error = RuntimeError(
            f"isolapurr command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr.strip()}"
        )
        if attempt == 1:
            break
        time.sleep(0.5)
    raise RuntimeError(f"isolapurr command failed: {' '.join(cmd)}") from last_error


def ensure_isolapurr_available() -> None:
    for cmd in (["isolapurr", "--help"],):
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=8)
        if proc.returncode != 0:
            raise RuntimeError(f"required tool missing or unhealthy: {' '.join(cmd)}")


def format_source_class(source_status: dict[str, Any]) -> str:
    max_mv = int(source_status.get("ppsCapabilityMaxMv") or 0)
    max_ma = int(source_status.get("ppsCapabilityMaxMa") or 0)
    return "pps5a" if max_mv >= 20_000 and max_ma >= 5_000 else "pps3a"


@dataclass
class DeviceInfo:
    device_id: str
    port_path: str
    hardware_id: str | None
    display_name: str


class FluxClient:
    def __init__(self, devd_url: str, authorized_port: str):
        self.devd_url = devd_url.rstrip("/")
        self.authorized_port = authorized_port

    def find_device(self) -> DeviceInfo:
        payload = http_json("GET", f"{self.devd_url}/api/v1/devices")
        devices = payload.get("devices", [])
        matches = [device for device in devices if device.get("portPath") == self.authorized_port]
        if len(matches) != 1:
            raise RuntimeError(
                f"expected exactly one device on authorized port {self.authorized_port}, got {len(matches)}"
            )
        device = matches[0]
        return DeviceInfo(
            device_id=device["id"],
            port_path=device["portPath"],
            hardware_id=device.get("hardwareId"),
            display_name=device.get("displayName", device["id"]),
        )

    def create_lease(self, device_id: str) -> str:
        payload = http_json(
            "POST",
            f"{self.devd_url}/api/v1/devices/{urllib.parse.quote(device_id)}/leases",
            {},
        )
        return payload["leaseId"]

    def heartbeat(self, lease_id: str) -> None:
        http_json(
            "POST",
            f"{self.devd_url}/api/v1/leases/{urllib.parse.quote(lease_id)}/heartbeat",
            {},
        )

    def release_lease(self, lease_id: str) -> None:
        http_json(
            "DELETE",
            f"{self.devd_url}/api/v1/leases/{urllib.parse.quote(lease_id)}",
        )

    def leased_status(self, device_id: str, lease_id: str) -> dict[str, Any]:
        url = (
            f"{self.devd_url}/api/v1/devices/{urllib.parse.quote(device_id)}/status"
            f"?lease_id={urllib.parse.quote(lease_id)}"
        )
        last_error: BaseException | None = None
        for attempt in range(3):
            try:
                return http_json("GET", url)
            except BaseException as exc:
                last_error = exc
                if not is_timeout_like(exc) or attempt == 2:
                    raise
                time.sleep(0.5 * (attempt + 1))
        raise RuntimeError(f"status request failed for {device_id}") from last_error

    def runtime_put(
        self, device_id: str, lease_id: str, payload: dict[str, Any]
    ) -> dict[str, Any]:
        body = {"leaseId": lease_id, **payload}
        url = (
            f"{self.devd_url}/api/v1/devices/{urllib.parse.quote(device_id)}/runtime"
            f"?lease_id={urllib.parse.quote(lease_id)}"
        )
        last_error: BaseException | None = None
        for attempt in range(3):
            try:
                return http_json("PUT", url, body)
            except BaseException as exc:
                last_error = exc
                if not is_timeout_like(exc) or attempt == 2:
                    raise
                time.sleep(0.5 * (attempt + 1))
        raise RuntimeError(f"runtime request failed for {device_id}") from last_error


def verify_isolapurr_source(source_url: str, expected_prefix: str) -> dict[str, Any]:
    ensure_isolapurr_available()
    status = run_isolapurr_json(source_url, ["status"])
    device = status.get("device", {})
    device_id = str(device.get("device_id") or device.get("deviceId") or "")
    if not device_id.startswith(expected_prefix):
        raise RuntimeError(
            f"isolapurr device id mismatch: expected prefix {expected_prefix}, got {device_id or 'missing'}"
        )
    config = run_isolapurr_json(source_url, ["power", "config", "show"])
    capability = (config.get("config") or config).get("capability", {})
    power_watts = int(capability.get("power_watts") or capability.get("powerWatts") or 0)
    pps_enabled = bool((capability.get("pd") or {}).get("pps"))
    pps_5a = bool((capability.get("current") or {}).get("pd_pps_5a"))
    if power_watts < 100 or not pps_enabled or not pps_5a:
        raise RuntimeError(
            f"isolapurr capability is not 100W PPS 5A: power={power_watts} pps={pps_enabled} pps5a={pps_5a}"
        )
    return {
        "deviceId": device_id,
        "status": status,
        "config": config.get("config") or config,
    }


def set_source_auto(source_url: str) -> None:
    run_isolapurr_json(source_url, ["power", "output", "auto"])


def read_source_telemetry(source_url: str) -> dict[str, Any]:
    payload = run_isolapurr_json(source_url, ["power", "show"])
    diag = payload.get("diagnostics", {})
    usb = diag.get("usb_c_actual")
    if not usb:
        ports = (payload.get("ports") or {}).get("ports") or []
        for port in ports:
            if port.get("portId") == "port_c":
                usb = port.get("telemetry")
                break
    if not usb:
        raise RuntimeError("isolapurr USB-C telemetry unavailable")
    return {
        "voltageMv": int(usb.get("voltage_mv") or usb.get("voltageMv") or 0),
        "currentMa": int(usb.get("current_ma") or usb.get("currentMa") or 0),
        "powerMw": int(usb.get("power_mw") or usb.get("powerMw") or 0),
        "status": usb.get("status"),
    }


def extract_point_from_status(status: dict[str, Any], target_temp_c: int) -> dict[str, Any]:
    thermal = status.get("thermalControl")
    if not isinstance(thermal, dict):
        raise RuntimeError("status missing thermalControl")
    point = {field: int(thermal[field]) for field in POINT_FIELDS if field in thermal}
    settings = {field: int(thermal[field]) for field in SETTINGS_FIELDS if field in thermal}
    point["targetTempC"] = target_temp_c
    return {"point": point, "settings": settings}


def minimal_profile_with_point(
    base_profile: dict[str, Any], explicit_point: dict[str, Any]
) -> dict[str, Any]:
    return {
        "settings": json.loads(json.dumps(base_profile.get("settings"))),
        "points": [json.loads(json.dumps(explicit_point)), None, None, None, None, None, None, None, None, None],
    }


def cooldown_threshold(target_temp_c: int) -> float:
    return max(35.0, target_temp_c - 30.0)


def stats_from_samples(samples: list[dict[str, Any]]) -> dict[str, Any]:
    if not samples:
        return {}
    voltages = [sample["sourceTelemetry"]["voltageMv"] for sample in samples]
    currents = [sample["sourceTelemetry"]["currentMa"] for sample in samples]
    powers = [sample["sourceTelemetry"]["powerMw"] for sample in samples]
    return {
        "sampleCount": len(samples),
        "voltageMv": {
            "min": min(voltages),
            "max": max(voltages),
            "avg": round(sum(voltages) / len(voltages), 1),
        },
        "currentMa": {
            "min": min(currents),
            "max": max(currents),
            "avg": round(sum(currents) / len(currents), 1),
        },
        "powerMw": {
            "min": min(powers),
            "max": max(powers),
            "avg": round(sum(powers) / len(powers), 1),
        },
    }


def variant_label(variant_id: str) -> str:
    return "0加热" if variant_id == "zero_coast" else "50%最低功率加热"


def make_variant_point(
    point: dict[str, Any],
    variant_id: str,
    brake_distance_centi_c: int,
) -> tuple[dict[str, Any], int]:
    tuned = dict(point)
    sustain_min = max(int(point["holdPowerPermille"]), int(point["holdReheatPowerPermille"]))
    half_floor = max(1, int(round(sustain_min / 2.0)))
    if variant_id == "zero_coast":
        tuned.update(
            {
                "brakeDistanceCentiC": brake_distance_centi_c,
                "approachPowerPermille": 0,
                "approachFloorPowerPermille": 0,
                "approachTailWindowCentiC": 0,
                "holdPowerPermille": 0,
                "holdReheatPowerPermille": 0,
                "holdEntryCentiC": 1,
                "holdOnCentiC": 1,
                "approachLeadTicks": 0,
                "holdLeadTicks": 0,
            }
        )
        return tuned, 0
    tuned.update(
        {
            "brakeDistanceCentiC": brake_distance_centi_c,
            "approachPowerPermille": half_floor,
            "approachFloorPowerPermille": half_floor,
            "approachTailWindowCentiC": 0,
            "holdPowerPermille": 0,
            "holdReheatPowerPermille": 0,
            "holdEntryCentiC": 1,
            "holdOnCentiC": 1,
            "approachLeadTicks": 0,
            "holdLeadTicks": 0,
        }
    )
    return tuned, half_floor


def unique_brakes(values: list[int]) -> list[int]:
    ordered: list[int] = []
    for value in values:
        clamped = max(100, value)
        if clamped not in ordered:
            ordered.append(clamped)
    return ordered


def candidate_brake_plan(base_brake: int, variant_id: str) -> tuple[int, list[int], list[int]]:
    if variant_id == "zero_coast":
        seed_factor = 0.85
        smaller_factors = [0.80, 0.75, 0.70, 0.65, 0.60, 0.55]
        larger_factors = [0.90, 0.95, 1.00, 1.05]
    else:
        seed_factor = 0.93
        smaller_factors = [0.88, 0.83, 0.78, 0.73, 0.68, 0.63]
        larger_factors = [0.98, 1.03, 1.08, 1.13, 1.18]
    seed = max(100, int(round(base_brake * seed_factor)))
    smaller = unique_brakes([int(round(base_brake * factor)) for factor in smaller_factors])
    larger = unique_brakes([int(round(base_brake * factor)) for factor in larger_factors])
    return seed, smaller, larger


def invalid_reason_summary(invalid: dict[str, Any]) -> str:
    reason = invalid.get("reason", "unknown")
    sample = invalid.get("sample")
    if isinstance(sample, dict):
        return (
            f"{reason}"
            f" temp={sample.get('currentTempC')}"
            f" phase={sample.get('heaterControlPhase')}"
            f" output={sample.get('heaterOutputPercent')}"
            f" t={sample.get('approachElapsedMs')}"
        )
    peak = invalid.get("peak")
    if isinstance(peak, dict):
        return f"{reason} peak={peak.get('tempC')} t={peak.get('approachElapsedMs')}"
    first_band = invalid.get("firstBand")
    if isinstance(first_band, dict):
        return f"{reason} firstBand={first_band.get('tempC')} t={first_band.get('approachElapsedMs')}"
    return reason


def invalid_direction(
    invalid: dict[str, Any],
    target_temp_c: int,
    band_low: float,
    band_high: float,
) -> str | None:
    reason = str(invalid.get("reason") or "")
    sample = invalid.get("sample")
    sample_temp = float(sample.get("currentTempC")) if isinstance(sample, dict) and sample.get("currentTempC") is not None else None
    peak = invalid.get("peak")
    peak_temp = float(peak.get("tempC")) if isinstance(peak, dict) and peak.get("tempC") is not None else None
    first_band = invalid.get("firstBand")
    first_band_temp = (
        float(first_band.get("tempC"))
        if isinstance(first_band, dict) and first_band.get("tempC") is not None
        else None
    )
    if reason == "entered_hold":
        return "less_heat"
    if reason == "left_band_before_rollback":
        if sample_temp is None:
            return None
        if sample_temp < band_low:
            return "more_heat"
        if sample_temp > band_high:
            return "less_heat"
        return None
    if reason == "never_entered_approach":
        return "more_heat"
    if reason == "timeout_without_valid_rollback":
        if peak_temp is not None:
            if peak_temp < band_low:
                return "more_heat"
            if peak_temp > target_temp_c + 0.2:
                return "less_heat"
        if first_band_temp is not None:
            return "more_heat" if first_band_temp < target_temp_c else "less_heat"
        return "more_heat"
    return None


def narrowest_brake_bracket(
    more_heat_brakes: set[int],
    less_heat_brakes: set[int],
) -> tuple[int, int] | None:
    best: tuple[int, int] | None = None
    best_width: int | None = None
    for lower in less_heat_brakes:
        for upper in more_heat_brakes:
            if lower >= upper:
                continue
            width = upper - lower
            if best_width is None or width < best_width:
                best = (lower, upper)
                best_width = width
    return best


def serialize_sample(
    run_id: str,
    target_temp_c: int,
    variant_id: str,
    brake_distance_centi_c: int,
    half_floor_permille: int,
    elapsed_ms: int,
    approach_elapsed_ms: int | None,
    status: dict[str, Any],
    source_telemetry: dict[str, Any],
) -> dict[str, Any]:
    return {
        "runId": run_id,
        "targetTempC": target_temp_c,
        "variantId": variant_id,
        "brakeDistanceCentiC": brake_distance_centi_c,
        "halfFloorPermille": half_floor_permille,
        "elapsedMs": elapsed_ms,
        "approachElapsedMs": approach_elapsed_ms,
        "currentTempC": float(status["currentTempC"]),
        "heaterFilteredTempC": float(status.get("heaterFilteredTempC") or 0.0),
        "heaterControlErrorC": float(status.get("heaterControlErrorC") or 0.0),
        "heaterControlPhase": status.get("heaterControlPhase"),
        "heaterOutputPercent": int(status.get("heaterOutputPercent") or 0),
        "heaterPhysicalOutputPercent": int(status.get("heaterPhysicalOutputPercent") or 0),
        "sourceTelemetry": source_telemetry,
    }


def dry_run_target_result(target_temp_c: int, point: dict[str, Any], variant_id: str) -> dict[str, Any]:
    half_floor = max(1, round(max(point["holdPowerPermille"], point["holdReheatPowerPermille"]) / 2))
    brake = int(round(point["brakeDistanceCentiC"] * (0.85 if variant_id == "zero_coast" else 0.93)))
    label = variant_label(variant_id)
    start_temp = round(target_temp_c - brake / 100.0 + (1.2 if variant_id == "zero_coast" else 2.3), 2)
    duration_ms = 8200 if variant_id == "zero_coast" else 5200
    peak_temp = round(target_temp_c - 0.2 + (0.4 if variant_id == "half_floor_50" else -0.1), 2)
    rollback_temp = round(target_temp_c - 0.9, 2)
    samples = []
    for index in range(32):
        elapsed_ms = int(index * duration_ms / 31)
        progress = index / 31
        curve = 1.0 - (1.0 - progress) ** 2
        temp = start_temp + (peak_temp - start_temp) * curve
        if index > 24:
            rollback_progress = (index - 24) / 7
            temp = peak_temp - (peak_temp - rollback_temp) * rollback_progress
        output = 0 if variant_id == "zero_coast" else max(1, int(round(half_floor / 10)))
        if index >= 24:
            output = 0
        samples.append(
            {
                "approachElapsedMs": elapsed_ms,
                "currentTempC": round(temp, 2),
                "heaterFilteredTempC": round(temp - 0.25, 2),
                "heaterControlPhase": "approach",
                "heaterOutputPercent": output,
                "heaterPhysicalOutputPercent": output,
                "sourceTelemetry": {
                    "voltageMv": 21000,
                    "currentMa": 35 if output == 0 else 2000,
                    "powerMw": 735 if output == 0 else 42000,
                    "status": "ok",
                },
            }
        )
    warmup_exit = {
        "elapsedMs": 0,
        "approachElapsedMs": 0,
        "tempC": start_temp,
    }
    first_band = next(sample for sample in samples if sample["currentTempC"] >= target_temp_c - 1.5)
    peak = max(samples, key=lambda sample: sample["currentTempC"])
    rollback = next(
        sample
        for sample in samples
        if sample["approachElapsedMs"] > peak["approachElapsedMs"] and sample["currentTempC"] <= peak["currentTempC"] - 0.4
    )
    return {
        "targetTempC": target_temp_c,
        "variantId": variant_id,
        "variantLabel": label,
        "valid": True,
        "tunedPoint": make_variant_point(point, variant_id, brake)[0],
        "halfFloorPermille": half_floor if variant_id != "zero_coast" else 0,
        "metrics": {
            "warmupExit": warmup_exit,
            "firstBand": first_band,
            "peak": peak,
            "rollback": rollback,
            "approachDurationMs": first_band["approachElapsedMs"],
            "peakDelayMs": peak["approachElapsedMs"] - first_band["approachElapsedMs"],
            "rollbackDelayMs": rollback["approachElapsedMs"] - peak["approachElapsedMs"],
            "sourceStats": stats_from_samples(samples),
        },
        "samples": samples,
    }


def characterize_variant(
    flux: FluxClient,
    device: DeviceInfo,
    source_url: str,
    lease_id: str,
    run_id: str,
    base_profile: dict[str, Any],
    target_temp_c: int,
    effective_point: dict[str, Any],
    variant_id: str,
    samples_writer,
) -> dict[str, Any]:
    base_brake = int(effective_point["brakeDistanceCentiC"])
    band_low = target_temp_c - 1.5
    band_high = target_temp_c + 1.5
    seed_brake, smaller_brakes, larger_brakes = candidate_brake_plan(base_brake, variant_id)
    pending_brakes = [seed_brake]
    attempted_brakes: set[int] = set()
    more_heat_brakes: set[int] = set()
    less_heat_brakes: set[int] = set()
    smaller_index = 0
    larger_index = 0
    extrapolation_step = max(40, int(round(base_brake * 0.05)))
    while pending_brakes:
        brake_distance = pending_brakes.pop(0)
        if brake_distance in attempted_brakes:
            continue
        attempted_brakes.add(brake_distance)
        log(
            f"[target {target_temp_c}C] {variant_label(variant_id)} try brake={brake_distance}"
        )
        tuned_point, half_floor = make_variant_point(effective_point, variant_id, brake_distance)
        variant_profile = minimal_profile_with_point(base_profile, tuned_point)
        flux.runtime_put(
            device.device_id,
            lease_id,
            {
                "thermalProfileMode": "100w",
                "thermalControlProfile": {"op": "preview", "profile": variant_profile},
            },
        )
        flux.runtime_put(
            device.device_id,
            lease_id,
            {
                "heaterEnabled": False,
                "activeCoolingEnabled": True,
                "targetTempC": target_temp_c,
            },
        )
        cool_limit = cooldown_threshold(target_temp_c)
        while True:
            status = flux.leased_status(device.device_id, lease_id)
            current_temp = float(status["currentTempC"])
            if current_temp <= cool_limit:
                break
            time.sleep(1.0)
            flux.heartbeat(lease_id)
        armed = flux.runtime_put(
            device.device_id,
            lease_id,
            {
                "heaterEnabled": True,
                "activeCoolingEnabled": False,
                "targetTempC": target_temp_c,
            },
        )
        start = time.time()
        last_heartbeat = start
        last_source_telemetry_at = 0.0
        last_source_telemetry: dict[str, Any] | None = None
        source_telemetry_warning_emitted = False
        warmup_exit = None
        first_band = None
        peak = None
        rollback = None
        invalid = None
        variant_samples: list[dict[str, Any]] = []
        try:
            while time.time() - start < 45.0:
                now = time.time()
                if now - last_heartbeat > 5.0:
                    flux.heartbeat(lease_id)
                    last_heartbeat = now
                status = armed if warmup_exit is None and not variant_samples else flux.leased_status(
                    device.device_id, lease_id
                )
                if last_source_telemetry is None or now - last_source_telemetry_at >= 1.0:
                    try:
                        last_source_telemetry = read_source_telemetry(source_url)
                        last_source_telemetry_at = now
                    except Exception as exc:
                        if not source_telemetry_warning_emitted:
                            log(
                                f"[target {target_temp_c}C] {variant_label(variant_id)} "
                                f"source telemetry refresh failed: {exc}"
                            )
                            source_telemetry_warning_emitted = True
                        if last_source_telemetry is None:
                            last_source_telemetry = {
                                "voltageMv": 0,
                                "currentMa": 0,
                                "powerMw": 0,
                                "status": "unavailable",
                            }
                        else:
                            stale = dict(last_source_telemetry)
                            stale["status"] = "stale"
                            last_source_telemetry = stale
                source_telemetry = last_source_telemetry
                elapsed_ms = int((now - start) * 1000)
                phase = status.get("heaterControlPhase")
                output = max(
                    int(status.get("heaterOutputPercent") or 0),
                    int(status.get("heaterPhysicalOutputPercent") or 0),
                )
                approach_elapsed_ms = None if warmup_exit is None else elapsed_ms - warmup_exit["elapsedMs"]
                sample = serialize_sample(
                    run_id,
                    target_temp_c,
                    variant_id,
                    brake_distance,
                    half_floor,
                    elapsed_ms,
                    approach_elapsed_ms,
                    status,
                    source_telemetry,
                )
                variant_samples.append(sample)
                samples_writer.write(json.dumps(sample, ensure_ascii=False) + "\n")
                samples_writer.flush()
                temp_c = float(status["currentTempC"])
                if warmup_exit is None:
                    if phase == "approach":
                        warmup_exit = {
                            "elapsedMs": elapsed_ms,
                            "approachElapsedMs": 0,
                            "tempC": round(temp_c, 2),
                        }
                        if variant_id == "zero_coast" and output != 0:
                            invalid = {"reason": "approach_nonzero_on_entry", "sample": sample}
                            break
                        if variant_id == "half_floor_50" and output <= 0:
                            invalid = {"reason": "approach_zero_on_entry", "sample": sample}
                            break
                else:
                    in_band = band_low <= temp_c <= band_high
                    if phase == "hold":
                        invalid = {"reason": "entered_hold", "sample": sample}
                        break
                    if variant_id == "zero_coast" and phase == "approach" and output != 0:
                        invalid = {"reason": "approach_nonzero_after_warmup", "sample": sample}
                        break
                    if first_band is None:
                        if in_band:
                            first_band = {
                                "elapsedMs": elapsed_ms,
                                "approachElapsedMs": approach_elapsed_ms,
                                "tempC": round(temp_c, 2),
                            }
                            peak = dict(first_band)
                    else:
                        if not in_band:
                            invalid = {
                                "reason": "left_band_before_rollback",
                                "sample": sample,
                                "peak": peak,
                            }
                            break
                        if temp_c > peak["tempC"]:
                            peak = {
                                "elapsedMs": elapsed_ms,
                                "approachElapsedMs": approach_elapsed_ms,
                                "tempC": round(temp_c, 2),
                            }
                        if temp_c <= peak["tempC"] - 0.4:
                            rollback = {
                                "elapsedMs": elapsed_ms,
                                "approachElapsedMs": approach_elapsed_ms,
                                "tempC": round(temp_c, 2),
                            }
                            break
                time.sleep(0.25)
            if warmup_exit is None and invalid is None:
                invalid = {"reason": "never_entered_approach"}
            elif rollback is None and invalid is None:
                invalid = {
                    "reason": "timeout_without_valid_rollback",
                    "firstBand": first_band,
                    "peak": peak,
                }
        finally:
            flux.runtime_put(
                device.device_id,
                lease_id,
                {
                    "heaterEnabled": False,
                    "activeCoolingEnabled": True,
                    "targetTempC": target_temp_c,
                },
            )
            flux.runtime_put(
                device.device_id,
                lease_id,
                {
                    "thermalControlProfile": {"op": "clear_preview"},
                },
            )
        if invalid is None:
            log(
                f"[target {target_temp_c}C] {variant_label(variant_id)} pass brake={brake_distance} "
                f"warmupExit={warmup_exit['tempC']}C approach={first_band['approachElapsedMs']}ms "
                f"peak={peak['tempC']}C rollback={rollback['tempC']}C"
            )
            return {
                "targetTempC": target_temp_c,
                "variantId": variant_id,
                "variantLabel": variant_label(variant_id),
                "valid": True,
                "tunedPoint": tuned_point,
                "halfFloorPermille": half_floor,
                "metrics": {
                    "warmupExit": warmup_exit,
                    "firstBand": first_band,
                    "peak": peak,
                    "rollback": rollback,
                    "approachDurationMs": first_band["approachElapsedMs"],
                    "peakDelayMs": peak["approachElapsedMs"] - first_band["approachElapsedMs"],
                    "rollbackDelayMs": rollback["approachElapsedMs"] - peak["approachElapsedMs"],
                    "sourceStats": stats_from_samples(
                        [
                            sample
                            for sample in variant_samples
                            if sample["approachElapsedMs"] is not None and sample["approachElapsedMs"] >= 0
                        ]
                    ),
                },
                "samples": variant_samples,
            }
        log(
            f"[target {target_temp_c}C] {variant_label(variant_id)} reject brake={brake_distance} "
            f"{invalid_reason_summary(invalid)}"
        )
        direction = invalid_direction(invalid, target_temp_c, band_low, band_high)
        if direction == "more_heat":
            more_heat_brakes.add(brake_distance)
        elif direction == "less_heat":
            less_heat_brakes.add(brake_distance)
        bracket = narrowest_brake_bracket(more_heat_brakes, less_heat_brakes)
        if bracket is not None and bracket[1] - bracket[0] > 10:
            lower, upper = bracket
            midpoint = int(round((lower + upper) / 2.0))
            if midpoint not in attempted_brakes and midpoint not in pending_brakes:
                pending_brakes.insert(0, midpoint)
                continue
        inserted_directional_candidate = False
        if direction == "more_heat":
            while smaller_index < len(smaller_brakes):
                candidate = smaller_brakes[smaller_index]
                smaller_index += 1
                if candidate not in attempted_brakes and candidate not in pending_brakes:
                    pending_brakes.insert(0, candidate)
                    inserted_directional_candidate = True
                    break
            if not inserted_directional_candidate:
                candidate = max(100, brake_distance - extrapolation_step)
                if candidate not in attempted_brakes and candidate not in pending_brakes:
                    pending_brakes.insert(0, candidate)
                    inserted_directional_candidate = True
        elif direction == "less_heat":
            while larger_index < len(larger_brakes):
                candidate = larger_brakes[larger_index]
                larger_index += 1
                if candidate not in attempted_brakes and candidate not in pending_brakes:
                    pending_brakes.insert(0, candidate)
                    inserted_directional_candidate = True
                    break
            if not inserted_directional_candidate:
                candidate = brake_distance + extrapolation_step
                if candidate not in attempted_brakes and candidate not in pending_brakes:
                    pending_brakes.insert(0, candidate)
                    inserted_directional_candidate = True
        if not pending_brakes:
            while smaller_index < len(smaller_brakes):
                candidate = smaller_brakes[smaller_index]
                smaller_index += 1
                if candidate not in attempted_brakes:
                    pending_brakes.append(candidate)
                    break
            if not pending_brakes:
                while larger_index < len(larger_brakes):
                    candidate = larger_brakes[larger_index]
                    larger_index += 1
                    if candidate not in attempted_brakes:
                        pending_brakes.append(candidate)
                        break
    raise RuntimeError(f"failed to characterize {target_temp_c}C {variant_label(variant_id)}")


def generate_html(bundle: dict[str, Any]) -> str:
    data_json = json.dumps(bundle, ensure_ascii=False, separators=(",", ":"))
    return f"""<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Flux Purr Approach 曲线报告</title>
<style>
:root{{--bg:#f4f5f6;--paper:#fff;--ink:#182026;--muted:#66717a;--line:#d9dee2;--grid:#e8ebed;--green:#18794e;--blue:#1261a0;--amber:#9a6700;--amber-bg:#fff4ce;--cyan:#157a82;--radius:8px}}
*{{box-sizing:border-box}}body{{margin:0;background:var(--bg);color:var(--ink);font:14px/1.45 ui-sans-serif,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}}
.shell{{max-width:1320px;margin:auto;padding:24px}}
.topbar{{display:flex;justify-content:space-between;gap:24px;align-items:flex-start;margin-bottom:18px}}
.eyebrow{{font:600 12px/1.2 ui-monospace,SFMono-Regular,Menlo,monospace;color:var(--muted);text-transform:uppercase}}
h1{{font-size:26px;line-height:1.2;margin:6px 0 8px}}
.subtitle{{max-width:76ch;color:var(--muted)}}
.stamp{{text-align:right;color:var(--muted);font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;white-space:nowrap}}
.meta{{display:flex;flex-wrap:wrap;gap:6px 16px;margin-top:10px;color:var(--ink);font:600 12px/1.5 ui-monospace,SFMono-Regular,Menlo,monospace}}
.meta b{{color:var(--blue)}}
.summary{{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-bottom:18px}}
.card{{background:var(--paper);border:1px solid var(--line);border-radius:var(--radius);padding:14px 16px}}
.card h3{{margin:0 0 8px;font-size:17px}}
.metric{{font-size:13px;color:var(--muted);margin-top:6px}}
.metric strong{{color:var(--ink)}}
.section-head{{display:flex;align-items:end;justify-content:space-between;gap:16px;margin:24px 0 10px}}
.section-head h2{{font-size:17px;margin:0}}
.section-head p{{margin:0;color:var(--muted);font-size:12px}}
.segmented{{display:flex;border:1px solid var(--line);border-radius:6px;overflow:hidden;background:#fff}}
.segmented button{{border:0;border-right:1px solid var(--line);background:#fff;color:var(--muted);min-height:36px;padding:0 14px;font:600 13px inherit;cursor:pointer}}
.segmented button:last-child{{border-right:0}}
.segmented button.active{{background:var(--ink);color:#fff}}
.panel{{background:var(--paper);border:1px solid var(--line);border-radius:var(--radius);padding:14px}}
.panel-title{{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:10px}}
.panel-title h3{{font-size:14px;margin:0}}
.panel-title span{{font-size:12px;color:var(--muted)}}
.chart-wrap{{height:360px;position:relative}}
.chart-wrap.compact{{height:250px}}
canvas{{width:100%;height:100%;display:block}}
.facts{{display:grid;grid-template-columns:repeat(4,1fr);gap:1px;background:var(--line);border:1px solid var(--line);border-radius:var(--radius);overflow:hidden;margin-top:10px}}
.fact{{background:#fff;padding:12px}}
.fact label{{display:block;color:var(--muted);font-size:11px;margin-bottom:3px}}
.fact strong{{font:600 14px ui-monospace,SFMono-Regular,Menlo,monospace}}
.legend{{display:flex;gap:12px;align-items:center;color:var(--muted);font-size:12px;margin-top:8px}}
.banner{{margin:0 0 18px;background:var(--amber-bg);border:1px solid #ead899;border-radius:var(--radius);padding:12px 14px;color:#6a5600}}
.badge{{display:inline-block;border-radius:999px;padding:2px 8px;font:700 11px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace;letter-spacing:.02em}}
.badge.pass{{background:rgba(24,121,78,.12);color:var(--green)}}
.badge.fail{{background:rgba(194,59,50,.12);color:#c23b32}}
.badge.info{{background:rgba(18,97,160,.1);color:var(--blue)}}
.dot{{display:inline-block;width:10px;height:10px;border-radius:999px}}
.line-swatch{{display:inline-block;width:14px;height:0;border-top:2px solid currentColor;vertical-align:middle;margin-right:4px}}
.line-swatch.dashed{{border-top-style:dashed}}
.provenance{{margin-top:18px;color:var(--muted);font-size:12px}}
@media(max-width:900px){{.shell{{padding:14px}}.topbar{{display:block}}.stamp{{text-align:left;margin-top:8px}}.summary{{grid-template-columns:1fr}}.facts{{grid-template-columns:1fr 1fr}}}}
</style>
</head>
<body>
<main class="shell">
<header class="topbar">
  <div>
    <div class="eyebrow">Flux Purr / Approach Characterization</div>
    <h1>逼近阶段双曲线采样报告</h1>
    <div class="subtitle">每个目标温度都给出两条实测 Approach 曲线：0加热滑行曲线，以及 50% 最低功率加热曲线。横轴保留 warmup 退出前 3 秒参考窗口；0 秒垂直虚线标记 warmup → approach；曲线在进带并出现可见回落后截断，不混入 Hold 曲线。</div>
    <div class="meta">
      <span>选择模式 <b>{bundle["source"]["selectedMode"]}</b></span>
      <span>EEPROM bank <b>{bundle["source"]["resolvedBank"]}</b></span>
      <span>检测能力 <b>{bundle["source"]["detectedSourceClass"]}</b></span>
      <span>Provider <b>IsolaPurr</b></span>
    </div>
  </div>
  <div class="stamp">DEVICE {bundle["target"]["deviceId"]}<br>PORT {bundle["target"]["portPath"]}<br>REPORT {bundle["generatedAt"]}</div>
</header>
<div class="banner" id="bundleBanner" hidden></div>
<section class="summary" id="summary"></section>
<div class="section-head">
  <div>
    <h2>目标温度</h2>
    <p>Tabs 切换目标温度；每个图表同时显示 0加热 与 50%最低功率加热 两条曲线</p>
  </div>
  <div class="segmented" id="targetTabs"></div>
</div>
<section class="panel">
  <div class="panel-title">
    <h3>Approach 时间-温度曲线</h3>
    <span>保留 warmup 尾段 3 秒参考窗；0 秒虚线为 warmup → approach 分界；绿色带为 ±1.5°C 目标区间</span>
  </div>
  <div class="chart-wrap"><canvas id="approachChart"></canvas></div>
  <div class="legend">
    <span><i class="dot" style="background:#1261a0"></i> 0加热</span>
    <span><i class="dot" style="background:#c23b32"></i> 50%最低功率加热</span>
    <span><i class="dot" style="background:#18794e"></i> 目标区间</span>
    <span><i class="line-swatch dashed" style="color:#66717a"></i> warmup → approach 分界</span>
  </div>
  <div class="facts" id="facts"></div>
</section>
<section class="panel" style="margin-top:12px">
  <div class="panel-title">
    <h3>Approach 用时对比</h3>
    <span>0加热曲线作为 hard-limit upper bound；50%最低功率曲线作为 preferred target</span>
  </div>
  <div class="chart-wrap compact"><canvas id="durationChart"></canvas></div>
</section>
<footer class="provenance">Bundle 文件：<code>index.html</code> / <code>run.bundle.json</code> / <code>samples.ndjson</code> / <code>thermal-profile.accepted.json</code></footer>
</main>
<script>
const DATA={data_json};
const COLORS={{zero:'#1261a0',half:'#c23b32',band:'rgba(24,121,78,.12)',grid:'#e8ebed',text:'#66717a',ink:'#182026'}};
const PRE_CONTEXT_SECONDS=3.0;
function fmt(n,d=2){{return n==null?'—':Number(n).toFixed(d)}}
function fmtInt(n){{return n==null?'—':String(Math.round(Number(n)))}}
function escapeHtml(value){{return String(value??'').replace(/[&<>"]/g,ch=>({{'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}}[ch]||ch));}}
function holdBadge(hold){{if(!hold)return '<span class="badge info">未提供</span>';return hold.passed?'<span class="badge pass">PASS</span>':'<span class="badge fail">FAIL</span>';}}
function variantFor(target,id){{return target.variants.find(v=>v.variantId===id);}}
function approachSeconds(variant){{return variant?.metrics?.approachDurationMs==null?null:variant.metrics.approachDurationMs/1000;}}
function avgSourceWatts(source){{const avg=source?.powerMw?.avg;return avg==null?null:avg/1000;}}
function rangeText(min,max,unit,digits=2){{if(min==null&&max==null)return '—';return `${{fmt(min,digits)}}${{unit}} → ${{fmt(max,digits)}}${{unit}}`;}}
function sourceMetricText(source){{if(!source)return '—';const power=source?.powerMw;const voltage=source?.voltageMv;const current=source?.currentMa;const parts=[];if(power?.avg!=null)parts.push(`${{fmt(power.avg/1000,2)}} W avg`);if(power?.min!=null&&power?.max!=null)parts.push(`${{fmt(power.min/1000,2)}}-${{fmt(power.max/1000,2)}} W`);if(voltage?.avg!=null)parts.push(`${{fmt(voltage.avg/1000,2)}} V avg`);if(current?.avg!=null)parts.push(`${{fmt(current.avg/1000,0)}} mA avg`);return parts.length?parts.join(' · '):'—';}}
function bundleBannerText(data){{if(data.bundleDisposition==='preliminary_review')return '当前产物是 preliminary 审查 bundle；其中 thermal-profile.accepted.json 仅表示当前三点候选快照，不代表 committed accepted baseline，也不代表 EEPROM saved bank。';if(data.acceptedProfileRole==='review_candidate_snapshot')return '当前 thermal-profile.accepted.json 仅用于回放/审查当前候选快照。';return '';}}
const tabs=document.querySelector('#targetTabs');
const summary=document.querySelector('#summary');
const bannerText=bundleBannerText(DATA);
if(bannerText){{const banner=document.querySelector('#bundleBanner');banner.hidden=false;banner.textContent=bannerText;}}
summary.innerHTML=DATA.targets.map(target=>{{
  const zero=variantFor(target,'zero_coast');
  const half=variantFor(target,'half_floor_50');
  const hold=target.holdCheck||null;
  const zeroSeconds=approachSeconds(zero);
  const halfSeconds=approachSeconds(half);
  const gateDelta=(zeroSeconds==null||halfSeconds==null)?'—':fmt(zeroSeconds-halfSeconds,3)+' s';
  const overshoot=hold?.maxOvershootC==null?'—':fmt(hold.maxOvershootC,2)+' °C';
  const p2p=hold?.holdPeakToPeakC==null?'—':fmt(hold.holdPeakToPeakC,2)+' °C';
  const failure=hold?.passed||!hold?.failureReason?'':`<div class="metric">失败原因 <strong>${{escapeHtml(hold.failureReason)}}</strong></div>`;
  return `<article class="card"><h3>${{target.targetTempC}}°C</h3><div class="metric">0加热 <strong>${{zeroSeconds==null?'—':fmt(zeroSeconds,3)+' s'}}</strong></div><div class="metric">50%最低功率 <strong>${{halfSeconds==null?'—':fmt(halfSeconds,3)+' s'}}</strong></div><div class="metric">门槛差值 <strong>${{gateDelta}}</strong></div><div class="metric">Hold confirm <strong>${{holdBadge(hold)}}</strong></div><div class="metric">overshoot / p2p <strong>${{overshoot}} / ${{p2p}}</strong></div>${{failure}}</article>`;
}}).join('');
let active=DATA.targets[0].targetTempC;
tabs.innerHTML=DATA.targets.map((target,index)=>`<button class="${{index===0?'active':''}}" data-target="${{target.targetTempC}}">${{target.targetTempC}}°C</button>`).join('');
tabs.onclick=e=>{{if(!e.target.dataset.target)return;active=Number(e.target.dataset.target);tabs.querySelectorAll('button').forEach(button=>button.classList.toggle('active', Number(button.dataset.target)===active));renderAll();}};
function currentTarget(){{return DATA.targets.find(target=>target.targetTempC===active);}}
function frame(canvas){{const rect=canvas.getBoundingClientRect();const dpr=window.devicePixelRatio||1;canvas.width=rect.width*dpr;canvas.height=rect.height*dpr;const c=canvas.getContext('2d');c.scale(dpr,dpr);return [c,rect.width,rect.height];}}
function plotSeconds(sample,warmupExitElapsedMs){{if(sample.approachElapsedMs!=null)return sample.approachElapsedMs/1000;if(warmupExitElapsedMs==null||sample.elapsedMs==null)return null;return (sample.elapsedMs-warmupExitElapsedMs)/1000;}}
function chartSeries(variant){{if(!variant||!Array.isArray(variant.samples))return[];const warmupExitElapsedMs=variant.metrics?.warmupExit?.elapsedMs??null;return variant.samples.map(sample=>({{...sample,plotSeconds:plotSeconds(sample,warmupExitElapsedMs)}})).filter(sample=>sample.plotSeconds!=null&&sample.plotSeconds>=-PRE_CONTEXT_SECONDS);}}
function drawApproach(){{const [c,w,h]=frame(document.querySelector('#approachChart'));c.clearRect(0,0,w,h);const m={{l:56,r:20,t:18,b:34}};const pw=w-m.l-m.r,ph=h-m.t-m.b;const target=currentTarget();const zero=target.variants.find(v=>v.variantId==='zero_coast');const half=target.variants.find(v=>v.variantId==='half_floor_50');const zeroSeries=chartSeries(zero);const halfSeries=chartSeries(half);const all=[...zeroSeries,...halfSeries];let minX=Math.min(...all.map(sample=>sample.plotSeconds),-PRE_CONTEXT_SECONDS);let maxX=Math.max(...all.map(sample=>sample.plotSeconds),0.5);if(maxX-minX<1)maxX=minX+1;const temps=all.map(sample=>sample.currentTempC);const targetMin=target.targetTempC-1.5,targetMax=target.targetTempC+1.5;const yMin=Math.min(...temps,targetMin)-1;const yMax=Math.max(...temps,targetMax)+1;const x=seconds=>m.l+((seconds-minX)/(maxX-minX))*pw;const y=value=>m.t+((yMax-value)/(yMax-yMin))*ph;
c.fillStyle='rgba(102,113,122,.05)';c.fillRect(m.l,m.t,Math.max(0,x(0)-m.l),ph);
c.fillStyle=COLORS.band;c.fillRect(m.l,y(targetMax),pw,y(targetMin)-y(targetMax));
for(let i=0;i<=4;i++){{const yy=m.t+ph*i/4;c.strokeStyle=COLORS.grid;c.beginPath();c.moveTo(m.l,yy);c.lineTo(w-m.r,yy);c.stroke();const value=yMax-((yMax-yMin)*i/4);c.fillStyle=COLORS.text;c.font='11px ui-monospace,monospace';c.fillText(fmt(value,1),8,yy+4);}}
for(let i=0;i<=6;i++){{const seconds=minX+((maxX-minX)*i/6);const xx=x(seconds);c.strokeStyle=COLORS.grid;c.beginPath();c.moveTo(xx,m.t);c.lineTo(xx,m.t+ph);c.stroke();c.fillStyle=COLORS.text;c.fillText(fmt(seconds,1)+'s',xx-12,h-10);}}
function line(samples,color){{c.beginPath();let started=false;for(const sample of samples){{const xx=x(sample.plotSeconds),yy=y(sample.currentTempC);if(!started){{c.moveTo(xx,yy);started=true;}}else{{c.lineTo(xx,yy);}}}}c.strokeStyle=color;c.lineWidth=2;c.stroke();}}
line(zeroSeries,COLORS.zero);line(halfSeries,COLORS.half);
c.strokeStyle='#18794e';c.lineWidth=1.5;c.beginPath();c.moveTo(m.l,y(target.targetTempC));c.lineTo(w-m.r,y(target.targetTempC));c.stroke();
c.strokeStyle=COLORS.text;c.lineWidth=1.5;c.setLineDash([6,4]);c.beginPath();c.moveTo(x(0),m.t);c.lineTo(x(0),m.t+ph);c.stroke();c.setLineDash([]);
c.fillStyle=COLORS.text;c.font='11px ui-sans-serif,system-ui';c.fillText('warmup→approach',Math.min(w-m.r-92,x(0)+6),m.t+12);
}}
function drawDurations(){{const [c,w,h]=frame(document.querySelector('#durationChart'));c.clearRect(0,0,w,h);const m={{l:56,r:20,t:18,b:34}};const pw=w-m.l-m.r,ph=h-m.t-m.b;const durations=DATA.targets.flatMap(target=>target.variants.map(variant=>variant.metrics.approachDurationMs/1000));const maxY=Math.max(...durations,10);const xStep=pw/DATA.targets.length;const barWidth=xStep*0.28;const y=value=>m.t+((maxY-value)/maxY)*ph;
for(let i=0;i<=4;i++){{const yy=m.t+ph*i/4;c.strokeStyle=COLORS.grid;c.beginPath();c.moveTo(m.l,yy);c.lineTo(w-m.r,yy);c.stroke();const value=maxY-(maxY*i/4);c.fillStyle=COLORS.text;c.font='11px ui-monospace,monospace';c.fillText(fmt(value,1)+'s',8,yy+4);}}
DATA.targets.forEach((target,index)=>{{const zero=target.variants.find(v=>v.variantId==='zero_coast').metrics.approachDurationMs/1000;const half=target.variants.find(v=>v.variantId==='half_floor_50').metrics.approachDurationMs/1000;const baseX=m.l+index*xStep+xStep/2;c.fillStyle=COLORS.zero;c.fillRect(baseX-barWidth-2,y(zero),barWidth,m.t+ph-y(zero));c.fillStyle=COLORS.half;c.fillRect(baseX+2,y(half),barWidth,m.t+ph-y(half));c.fillStyle=COLORS.text;c.fillText(target.targetTempC+'°C',baseX-16,h-10);}});
}}
function renderFacts(){{const target=currentTarget();const zero=variantFor(target,'zero_coast');const half=variantFor(target,'half_floor_50');const hold=target.holdCheck||null;const facts=[
['0加热用时',zero?.metrics?.approachDurationMs==null?'—':fmt(zero.metrics.approachDurationMs/1000,3)+' s'],
['50%最低功率用时',half?.metrics?.approachDurationMs==null?'—':fmt(half.metrics.approachDurationMs/1000,3)+' s'],
['0加热 brake',zero?.tunedPoint?.brakeDistanceCentiC==null?'—':String(zero.tunedPoint.brakeDistanceCentiC)],
['50%最低功率 brake',half?.tunedPoint?.brakeDistanceCentiC==null?'—':String(half.tunedPoint.brakeDistanceCentiC)],
['50%最低功率 half-floor',half?.halfFloorPermille==null?'—':String(half.halfFloorPermille)+' ‰'],
['0加热 warmup 退出温度',zero?.metrics?.warmupExit?.tempC==null?'—':fmt(zero.metrics.warmupExit.tempC,2)+' °C'],
['50%最低功率 warmup 退出温度',half?.metrics?.warmupExit?.tempC==null?'—':fmt(half.metrics.warmupExit.tempC,2)+' °C'],
['0加热 source 摘要',sourceMetricText(zero?.metrics?.sourceStats)],
['50%最低功率 source 摘要',sourceMetricText(half?.metrics?.sourceStats)],
['warmup 参考窗口',fmt(PRE_CONTEXT_SECONDS,1)+' s'],
['0 秒虚线', 'warmup→approach'],
];if(hold){{facts.push(
['Hold confirm',hold.passed?'PASS':'FAIL'],
['Hold failure reason',hold.failureReason||'—'],
['Hold seconds',hold.holdSeconds==null?'—':fmtInt(hold.holdSeconds)+' s'],
['firstHoldAtMs',hold.firstHoldAtMs==null?'—':fmtInt(hold.firstHoldAtMs)+' ms'],
['maxOvershootC',hold.maxOvershootC==null?'—':fmt(hold.maxOvershootC,2)+' °C'],
['holdPeakToPeakC',hold.holdPeakToPeakC==null?'—':fmt(hold.holdPeakToPeakC,2)+' °C'],
['holdMedianOutputPermille',hold.holdMedianOutputPermille==null?'—':fmtInt(hold.holdMedianOutputPermille)+' ‰'],
['holdP90OutputPermille',hold.holdP90OutputPermille==null?'—':fmtInt(hold.holdP90OutputPermille)+' ‰'],
['confirm run id',hold.confirmRunId||'—'],
['Approach source 摘要',sourceMetricText(hold.approachSource)],
['Hold source 摘要',sourceMetricText(hold.holdSource)],
);}}document.querySelector('#facts').innerHTML=facts.map(item=>`<div class="fact"><label>${{item[0]}}</label><strong>${{item[1]}}</strong></div>`).join('');}}
function renderAll(){{drawApproach();drawDurations();renderFacts();}}
new ResizeObserver(renderAll).observe(document.body);
renderAll();
</script>
</body>
</html>
"""


def main() -> int:
    parser = argparse.ArgumentParser(description="Collect Flux Purr dual-approach characterization bundle")
    parser.add_argument("--devd-url", default="http://127.0.0.1:62610")
    parser.add_argument("--authorized-port", default="/dev/cu.usbmodem2111401")
    parser.add_argument("--source-url", required=True)
    parser.add_argument("--source-id-prefix", required=True)
    parser.add_argument("--profile-file", required=True, type=Path)
    parser.add_argument("--targets-c")
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    targets = parse_targets(args.targets_c)
    base_profile = json.loads(args.profile_file.read_text())
    bundle_dir = args.output_dir
    bundle_dir.mkdir(parents=True, exist_ok=True)
    samples_path = bundle_dir / "samples.ndjson"
    bundle_path = bundle_dir / "run.bundle.json"
    accepted_profile_path = bundle_dir / "thermal-profile.accepted.json"
    report_path = bundle_dir / "index.html"
    run_id = slugify(
        [
            "approach-characterization",
            dt.datetime.now().strftime("%Y%m%d-%H%M%S"),
            "pd100w-pps5a",
        ]
    )

    source_info = verify_isolapurr_source(args.source_url, args.source_id_prefix)
    set_source_auto(args.source_url)

    target_payload: dict[str, Any]
    characterization_targets: list[dict[str, Any]] = []
    with samples_path.open("w", encoding="utf-8") as samples_writer:
        if args.dry_run:
            target_payload = {
                "deviceId": "dry-run-device",
                "portPath": args.authorized_port,
                "hardwareId": None,
            }
            source_payload = {
                "selectedMode": "100w",
                "resolvedBank": "pps5a",
                "detectedSourceClass": "pps5a",
                "sourceDeviceId": source_info["deviceId"],
            }
            for target_temp_c in targets:
                base_point = next((point for point in base_profile["points"] if point and int(point["targetTempC"]) == target_temp_c), None)
                if base_point is None:
                    lower = max(
                        (
                            point
                            for point in base_profile["points"]
                            if point and int(point["targetTempC"]) < target_temp_c
                        ),
                        key=lambda point: int(point["targetTempC"]),
                    )
                    upper = min(
                        (
                            point
                            for point in base_profile["points"]
                            if point and int(point["targetTempC"]) > target_temp_c
                        ),
                        key=lambda point: int(point["targetTempC"]),
                    )
                    base_point = dict(lower)
                    base_point["targetTempC"] = target_temp_c
                    base_point["brakeDistanceCentiC"] = int(round((int(lower["brakeDistanceCentiC"]) + int(upper["brakeDistanceCentiC"])) / 2))
                    base_point["holdPowerPermille"] = int(round((int(lower["holdPowerPermille"]) + int(upper["holdPowerPermille"])) / 2))
                    base_point["holdReheatPowerPermille"] = int(round((int(lower["holdReheatPowerPermille"]) + int(upper["holdReheatPowerPermille"])) / 2))
                target_result = {
                    "targetTempC": target_temp_c,
                    "effectivePoint": base_point,
                    "variants": [
                        dry_run_target_result(target_temp_c, base_point, "zero_coast"),
                        dry_run_target_result(target_temp_c, base_point, "half_floor_50"),
                    ],
                }
                characterization_targets.append(target_result)
        else:
            flux = FluxClient(args.devd_url, args.authorized_port)
            device = flux.find_device()
            target_payload = {
                "deviceId": device.device_id,
                "portPath": device.port_path,
                "hardwareId": device.hardware_id,
            }
            lease_id = flux.create_lease(device.device_id)
            try:
                base_status = flux.leased_status(device.device_id, lease_id)
                source_payload = {
                    "selectedMode": "100w",
                    "resolvedBank": base_status.get("thermalProfileResolvedBank"),
                    "detectedSourceClass": format_source_class(base_status),
                    "sourceDeviceId": source_info["deviceId"],
                }
                if source_payload["resolvedBank"] != "pps5a":
                    raise RuntimeError(
                        f"runtime bank mismatch: expected pps5a, got {source_payload['resolvedBank']}"
                    )
                if source_payload["detectedSourceClass"] != "pps5a":
                    raise RuntimeError(
                        f"source class mismatch: expected pps5a, got {source_payload['detectedSourceClass']}"
                    )
                for target_temp_c in targets:
                    log(f"=== characterize target {target_temp_c}C ===")
                    flux.runtime_put(
                        device.device_id,
                        lease_id,
                        {
                            "thermalProfileMode": "100w",
                            "targetTempC": target_temp_c,
                            "heaterEnabled": False,
                            "activeCoolingEnabled": True,
                            "thermalControlProfile": {"op": "preview", "profile": base_profile},
                        },
                    )
                    effective_status = flux.leased_status(device.device_id, lease_id)
                    effective = extract_point_from_status(effective_status, target_temp_c)
                    effective_point = effective["point"]
                    flux.runtime_put(
                        device.device_id,
                        lease_id,
                        {"thermalControlProfile": {"op": "clear_preview"}},
                    )
                    target_result = {
                        "targetTempC": target_temp_c,
                        "effectivePoint": effective_point,
                        "variants": [
                            characterize_variant(
                                flux,
                                device,
                                args.source_url,
                                lease_id,
                                run_id,
                                base_profile,
                                target_temp_c,
                                effective_point,
                                "zero_coast",
                                samples_writer,
                            ),
                            characterize_variant(
                                flux,
                                device,
                                args.source_url,
                                lease_id,
                                run_id,
                                base_profile,
                                target_temp_c,
                                effective_point,
                                "half_floor_50",
                                samples_writer,
                            ),
                        ],
                    }
                    characterization_targets.append(target_result)
            finally:
                try:
                    flux.release_lease(lease_id)
                finally:
                    set_source_auto(args.source_url)

    bundle = {
        "kind": "thermal_approach_characterization",
        "runId": run_id,
        "generatedAt": now_iso(),
        "selectedMode": source_payload["selectedMode"],
        "resolvedBank": source_payload["resolvedBank"],
        "detectedSourceClass": source_payload["detectedSourceClass"],
        "acceptedProfileRole": "seed_profile_snapshot",
        "target": target_payload,
        "source": source_payload,
        "seedProfileFile": str(args.profile_file),
        "targets": characterization_targets,
        "files": {
            "runBundlePath": str(bundle_path),
            "samplesPath": str(samples_path),
            "acceptedProfilePath": str(accepted_profile_path),
            "reportHtmlPath": str(report_path),
        },
    }

    accepted_profile_path.write_text(
        json.dumps(base_profile, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    bundle_path.write_text(
        json.dumps(bundle, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    report_path.write_text(generate_html(bundle), encoding="utf-8")
    log(f"bundle ready: {report_path}")
    log(f"bundle data: {bundle_path}")
    log(f"bundle samples: {samples_path}")
    log(f"bundle profile: {accepted_profile_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
