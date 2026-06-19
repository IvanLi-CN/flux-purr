#!/usr/bin/env python3
"""Collect Flux Purr calibration data from Flux Purr and IsolaPurr hardware."""

from __future__ import annotations

import argparse
import json
import statistics
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import uuid4

import serial


FLUX_DEVD = "http://127.0.0.1:30083"
FLUX_DEVICE = "serial-303a-1001-D0:CF:13:08:A1:48"
FLUX_ALLOWED_PORT = "/dev/cu.usbmodem21231401"
ISOLAPURR_DEVICE = "856a141cdbd4"
ISOLAPURR_URL = "http://192.168.31.224"
STOP_TEMP_C = 250.0
TARGET_TEMP_C = 260
SAMPLE_INTERVAL_S = 0.5
ISOLAPURR_SAMPLE_INTERVAL_S = 1.0
MAX_RUNTIME_S = 300.0
FLUX_OPEN_ATTEMPTS = 5
FLUX_OPEN_SETTLE_S = 1.0


@dataclass
class CommandResult:
    payload: Any
    duration_ms: float


def run_json(command: list[str], timeout_s: float = 10.0) -> CommandResult:
    started = time.monotonic()
    completed = subprocess.run(
        command,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        timeout=timeout_s,
    )
    duration_ms = (time.monotonic() - started) * 1000.0
    if completed.returncode != 0:
        raise RuntimeError(
            f"command failed ({completed.returncode}): {' '.join(command)}\n"
            f"stderr={completed.stderr.strip()}"
        )
    try:
        return CommandResult(json.loads(completed.stdout), duration_ms)
    except json.JSONDecodeError as exc:
        raise RuntimeError(
            f"command did not return JSON: {' '.join(command)}\n"
            f"stdout={completed.stdout[:500]!r}\nstderr={completed.stderr.strip()}"
        ) from exc


def flux_cli() -> list[str]:
    local = Path("target/debug/flux-purr")
    if local.exists():
        return [str(local)]
    return ["cargo", "run", "-p", "flux-purr-devd", "--bin", "flux-purr", "--"]


def flux_command(*args: str) -> list[str]:
    return [
        *flux_cli(),
        "--devd",
        FLUX_DEVD,
        "--json",
        *args,
        "--device",
        FLUX_DEVICE,
    ]


def flux_status() -> CommandResult:
    return run_json(flux_command("status"), timeout_s=5.0)


def set_flux_runtime(*args: str) -> Any:
    return run_json(flux_command("runtime", "set", *args), timeout_s=8.0).payload


def isolapurr_show() -> CommandResult:
    return run_json(
        [
            "isolapurr",
            "power",
            "show",
            "--url",
            ISOLAPURR_URL,
            "--json",
        ],
        timeout_s=12.0,
    )


def isolapurr_manual(current_ma: int) -> Any:
    return run_json(
        [
            "isolapurr",
            "power",
            "output",
            "manual",
            "--url",
            ISOLAPURR_URL,
            "--voltage-mv",
            "20000",
            "--current-limit-ma",
            str(current_ma),
            "--usb-c-path",
            "forced-on",
            "--json",
        ],
        timeout_s=15.0,
    ).payload


def stop_heater() -> None:
    try:
        with FluxPurrSerial() as flux:
            flux.runtime(
                heater_enabled=False,
                active_cooling_enabled=True,
                manual_pps_enabled=False,
            )
    except Exception as exc:  # noqa: BLE001
        print(f"warning: failed to stop heater: {exc}", file=sys.stderr)


def validate_flux_status(status: dict[str, Any]) -> None:
    for field in [
        "currentTempC",
        "heaterEnabled",
        "voltageMv",
        "currentMa",
        "rtdRawAdcMv",
        "vinRawAdcMv",
    ]:
        if field not in status:
            raise RuntimeError(f"flux status missing field: {field}")


class FluxPurrSerial:
    def __init__(self) -> None:
        self.serial: serial.Serial | None = None

    def __enter__(self) -> "FluxPurrSerial":
        last_error: Exception | None = None
        for attempt in range(1, FLUX_OPEN_ATTEMPTS + 1):
            try:
                self.serial = serial.Serial(
                    FLUX_ALLOWED_PORT,
                    115200,
                    timeout=2.0,
                    write_timeout=2.0,
                    exclusive=True,
                )
                time.sleep(FLUX_OPEN_SETTLE_S)
                self.serial.reset_input_buffer()
                self.status()
                return self
            except Exception as exc:  # noqa: BLE001
                last_error = exc
                if self.serial is not None:
                    self.serial.close()
                    self.serial = None
                if attempt < FLUX_OPEN_ATTEMPTS:
                    time.sleep(attempt)
        raise RuntimeError(
            f"Flux Purr serial session did not become ready on {FLUX_ALLOWED_PORT}: "
            f"{last_error}"
        )

    def __exit__(self, *_: object) -> None:
        if self.serial is not None:
            self.serial.close()
            self.serial = None

    def request(self, frame: dict[str, Any], timeout_s: float = 3.0) -> CommandResult:
        if self.serial is None:
            raise RuntimeError("Flux Purr serial session is not open")
        last_timeout_id = ""
        for _ in range(3):
            request_id = frame["requestId"] = f"cal-{uuid4().hex[:12]}"
            last_timeout_id = request_id
            started = time.monotonic()
            encoded = json.dumps(frame, separators=(",", ":")).encode("utf-8") + b"\n"
            self.serial.write(encoded)
            self.serial.flush()
            deadline = time.monotonic() + timeout_s
            while time.monotonic() < deadline:
                line = self.serial.readline()
                if not line:
                    continue
                try:
                    decoded = json.loads(line.decode("utf-8", errors="replace"))
                except json.JSONDecodeError:
                    continue
                if decoded.get("requestId") != request_id:
                    continue
                duration_ms = (time.monotonic() - started) * 1000.0
                if not decoded.get("ok", False):
                    raise RuntimeError(f"Flux Purr serial error: {decoded.get('error')}")
                return CommandResult(decoded["result"]["status"], duration_ms)
            self.serial.reset_input_buffer()
        raise RuntimeError(f"Flux Purr serial response timed out for {last_timeout_id}")

    def status(self) -> CommandResult:
        result = self.request({"type": "request", "op": "get_status"})
        validate_flux_status(result.payload)
        return result

    def runtime(
        self,
        *,
        heater_enabled: bool | None = None,
        target_temp_c: int | None = None,
        active_cooling_enabled: bool | None = None,
        manual_pps_enabled: bool | None = None,
        manual_pps_mv: int | None = None,
        manual_pps_ma: int | None = None,
    ) -> CommandResult:
        config: dict[str, Any] = {}
        if heater_enabled is not None:
            config["heaterEnabled"] = heater_enabled
        if target_temp_c is not None:
            config["targetTempC"] = target_temp_c
        if active_cooling_enabled is not None:
            config["activeCoolingEnabled"] = active_cooling_enabled
        if manual_pps_enabled is not None:
            config["manualPpsEnabled"] = manual_pps_enabled
        if manual_pps_mv is not None:
            config["manualPpsMv"] = manual_pps_mv
        if manual_pps_ma is not None:
            config["manualPpsMa"] = manual_pps_ma
        result = self.request({"type": "runtime_config", **config})
        validate_flux_status(result.payload)
        return result


def validate_isolapurr_status(show: dict[str, Any], current_ma: int) -> None:
    config = show["config"]
    diagnostics = show["diagnostics"]
    manual = config["manual"]
    tps = diagnostics["tps_setpoint"]
    actual = diagnostics["usb_c_actual"]
    if config["tps_mode"] != "manual":
        raise RuntimeError(f"IsolaPurr is not in manual mode: {config['tps_mode']}")
    if manual["current_limit_ma"] != current_ma or tps["ilim_ma"] != current_ma:
        raise RuntimeError(
            "IsolaPurr current limit readback mismatch: "
            f"manual={manual['current_limit_ma']} tps={tps['ilim_ma']} expected={current_ma}"
        )
    if manual["usb_c_path_mode"] != "force" or manual["path_policy"] != "force_open":
        raise RuntimeError(f"IsolaPurr USB-C path is not forced on: {manual}")
    if tps["mv"] != 20000 or not tps["output_enabled"]:
        raise RuntimeError(f"IsolaPurr TPS setpoint is not 20V enabled: {tps}")
    if actual["status"] != "ok":
        raise RuntimeError(f"IsolaPurr USB-C telemetry is not ok: {actual}")


def ensure_target_binding() -> None:
    if not Path(FLUX_ALLOWED_PORT).exists():
        raise RuntimeError(f"authorized Flux Purr port is missing: {FLUX_ALLOWED_PORT}")


def summarize(samples: list[dict[str, Any]]) -> dict[str, Any]:
    def values(path: tuple[str, ...]) -> list[float]:
        result: list[float] = []
        for sample in samples:
            value: Any = sample
            for key in path:
                value = value[key]
            if isinstance(value, (int, float)):
                result.append(float(value))
        return result

    def stats(path: tuple[str, ...]) -> dict[str, float | int | None]:
        series = values(path)
        if not series:
            return {"count": 0, "min": None, "max": None, "mean": None, "first": None, "last": None}
        return {
            "count": len(series),
            "min": min(series),
            "max": max(series),
            "mean": statistics.fmean(series),
            "first": series[0],
            "last": series[-1],
        }

    intervals = [
        samples[index]["elapsedMs"] - samples[index - 1]["elapsedMs"]
        for index in range(1, len(samples))
    ]
    return {
        "intervalMs": {
            "count": len(intervals),
            "min": min(intervals) if intervals else None,
            "max": max(intervals) if intervals else None,
            "mean": statistics.fmean(intervals) if intervals else None,
        },
        "fluxCurrentTempC": stats(("flux", "currentTempC")),
        "fluxVoltageMv": stats(("flux", "voltageMv")),
        "fluxCurrentMa": stats(("flux", "currentMa")),
        "fluxRtdRawAdcMv": stats(("flux", "rtdRawAdcMv")),
        "fluxVinRawAdcMv": stats(("flux", "vinRawAdcMv")),
        "isolapurrVoltageMv": stats(("isolapurr", "usbCActual", "voltageMv")),
        "isolapurrCurrentMa": stats(("isolapurr", "usbCActual", "currentMa")),
        "isolapurrPowerMw": stats(("isolapurr", "usbCActual", "powerMw")),
    }


def compact_isolapurr(show: dict[str, Any]) -> dict[str, Any]:
    diagnostics = show["diagnostics"]
    config = show["config"]
    return {
        "manual": config["manual"],
        "tpsSetpoint": diagnostics["tps_setpoint"],
        "usbCActual": {
            "voltageMv": diagnostics["usb_c_actual"]["voltage_mv"],
            "currentMa": diagnostics["usb_c_actual"]["current_ma"],
            "powerMw": diagnostics["usb_c_actual"]["power_mw"],
            "status": diagnostics["usb_c_actual"]["status"],
            "sampleUptimeMs": diagnostics["usb_c_actual"]["sample_uptime_ms"],
        },
        "sw2303VbusMv": diagnostics["sw2303_vbus_mv"],
        "sw2303Request": diagnostics["sw2303_request"],
        "lightLoadMode": config.get("light_load_mode"),
    }


def collect(args: argparse.Namespace) -> Path:
    current_ma = int(round(float(args.current_a) * 1000.0))
    run_started_unix_ms = int(time.time() * 1000)
    run_id = f"{run_started_unix_ms}-{current_ma}ma"
    run_dir = Path(args.output_dir) / run_id
    run_dir.mkdir(parents=True, exist_ok=False)
    samples_path = run_dir / "samples.ndjson"
    summary_path = run_dir / "run.json"

    ensure_target_binding()
    if not args.use_existing_isolapurr:
        isolapurr_manual(current_ma)
    iso_initial = isolapurr_show()
    validate_isolapurr_status(iso_initial.payload, current_ma)

    samples: list[dict[str, Any]] = []
    stop_reason = "max_runtime"
    threshold_sample_index = None
    heater_started = False
    last_iso = iso_initial
    last_iso_monotonic = 0.0
    iso_failures = 0
    with FluxPurrSerial() as flux_session:
        flux_initial = flux_session.runtime(
            heater_enabled=False,
            active_cooling_enabled=True,
            manual_pps_enabled=False,
        )
        if flux_initial.payload["currentTempC"] > args.max_start_temp_c:
            raise RuntimeError(
                f"start temperature too high: {flux_initial.payload['currentTempC']}C "
                f"> {args.max_start_temp_c}C"
            )

        try:
            if not args.no_heat:
                flux_session.runtime(
                    target_temp_c=TARGET_TEMP_C,
                    heater_enabled=True,
                    active_cooling_enabled=not args.disable_active_cooling,
                    manual_pps_enabled=False,
                )
                heater_started = True

            start = time.monotonic()
            deadline = start + args.max_runtime_s
            next_tick = start
            with samples_path.open("w", encoding="utf-8") as handle:
                index = 0
                while time.monotonic() < deadline:
                    sample_started = time.monotonic()
                    captured_unix_ms = int(time.time() * 1000)
                    flux = flux_session.status()
                    if (
                        sample_started - last_iso_monotonic
                        >= args.isolapurr_sample_interval_s
                    ):
                        try:
                            last_iso = isolapurr_show()
                            validate_isolapurr_status(last_iso.payload, current_ma)
                            last_iso_monotonic = sample_started
                            iso_failures = 0
                        except Exception:
                            iso_failures += 1
                            if iso_failures > args.max_isolapurr_failures:
                                raise

                    elapsed_ms = int((sample_started - start) * 1000)
                    flux_temp = float(flux.payload["currentTempC"])
                    phase = "dry_run" if args.no_heat else "warmup"
                    sample = {
                        "runId": run_id,
                        "sampleIndex": index,
                        "capturedAtUnixMs": captured_unix_ms,
                        "elapsedMs": elapsed_ms,
                        "phase": phase,
                        "heaterControl": {
                            "commanded": not args.no_heat,
                            "limitPercent": None if not args.no_heat else 0,
                            "mode": "fully_on" if not args.no_heat else "off",
                        },
            "source": {
                            "deviceId": ISOLAPURR_DEVICE,
                            "mode": "manual_cc",
                            "currentLimitMa": current_ma,
                            "voltageMv": 20000,
                            "usbCPath": "forced-on",
                        },
                        "commandLatencyMs": {
                            "fluxStatus": flux.duration_ms,
                            "isolapurrShow": last_iso.duration_ms,
                        },
                        "errors": {
                            "isolapurrConsecutiveFailures": iso_failures,
                        },
                        "flux": flux.payload,
                        "isolapurr": compact_isolapurr(last_iso.payload),
                    }
                    samples.append(sample)
                    handle.write(json.dumps(sample, separators=(",", ":")) + "\n")
                    handle.flush()

                    if not args.no_heat and not flux.payload["heaterEnabled"]:
                        stop_reason = "heater_disabled"
                        break

                    if not args.no_heat and flux_temp >= args.stop_temp_c:
                        stop_reason = "temperature_threshold"
                        threshold_sample_index = index
                        break

                    index += 1
                    next_tick = max(next_tick + args.sample_interval_s, time.monotonic())
                    sleep_s = next_tick - time.monotonic()
                    if sleep_s > 0:
                        time.sleep(sleep_s)
        except BaseException:
            if heater_started:
                flux_session.runtime(
                    heater_enabled=False,
                    active_cooling_enabled=True,
                    manual_pps_enabled=False,
                )
            raise

        if heater_started:
            flux_session.runtime(
                heater_enabled=False,
                active_cooling_enabled=True,
                manual_pps_enabled=False,
            )
        final_flux = flux_session.status().payload

    final_iso = isolapurr_show().payload
    summary = {
        "ok": True,
        "runId": run_id,
        "currentA": float(args.current_a),
        "currentLimitMa": current_ma,
        "startedAtUnixMs": run_started_unix_ms,
        "sampleCount": len(samples),
        "stopReason": stop_reason,
        "complete": args.no_heat or stop_reason == "temperature_threshold",
        "thresholdSampleIndex": threshold_sample_index,
        "parameters": {
            "targetTempC": TARGET_TEMP_C,
            "stopTempC": args.stop_temp_c,
            "sampleIntervalMs": int(args.sample_interval_s * 1000),
            "maxRuntimeS": args.max_runtime_s,
            "maxStartTempC": args.max_start_temp_c,
            "noHeat": args.no_heat,
            "heaterOutputMode": "fully_on" if not args.no_heat else "off",
            "activeCoolingEnabled": not args.disable_active_cooling,
        },
        "devices": {
            "flux": {
                "device": FLUX_DEVICE,
                "allowedPort": FLUX_ALLOWED_PORT,
                "devd": FLUX_DEVD,
            },
            "isolapurr": {
                "deviceId": ISOLAPURR_DEVICE,
                "url": ISOLAPURR_URL,
            },
        },
        "files": {
            "runDir": str(run_dir),
            "samples": str(samples_path),
            "summary": str(summary_path),
        },
        "initial": {
            "flux": flux_initial.payload,
            "isolapurr": compact_isolapurr(iso_initial.payload),
        },
        "final": {
            "flux": final_flux,
            "isolapurr": compact_isolapurr(final_iso),
        },
        "stats": summarize(samples),
    }
    summary_path.write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return run_dir


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--current-a", required=True, type=float)
    parser.add_argument("--output-dir", default="calibration-runs")
    parser.add_argument("--sample-interval-s", default=SAMPLE_INTERVAL_S, type=float)
    parser.add_argument(
        "--isolapurr-sample-interval-s", default=ISOLAPURR_SAMPLE_INTERVAL_S, type=float
    )
    parser.add_argument("--max-runtime-s", default=MAX_RUNTIME_S, type=float)
    parser.add_argument("--stop-temp-c", default=STOP_TEMP_C, type=float)
    parser.add_argument("--max-start-temp-c", default=40.0, type=float)
    parser.add_argument("--max-isolapurr-failures", default=10, type=int)
    parser.add_argument(
        "--use-existing-isolapurr",
        action="store_true",
        help="Do not write IsolaPurr settings; only verify current readback.",
    )
    parser.add_argument("--heat-pulse-on-s", default=2.0, type=float)
    parser.add_argument("--heat-pulse-off-s", default=2.0, type=float)
    parser.add_argument(
        "--disable-active-cooling",
        action="store_true",
        help="Disable active cooling during heat; intended only for diagnosis.",
    )
    parser.add_argument("--no-heat", action="store_true")
    args = parser.parse_args()
    try:
        collect(args)
    except BaseException as exc:  # noqa: BLE001
        print(f"error: {exc}", file=sys.stderr)
        stop_heater()
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
