#!/usr/bin/env python3
"""Smoke-test a running flux-purr-devd instance against an authorized device."""

from __future__ import annotations

import argparse
import glob
import json
import os
import socket
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from typing import Any


DEFAULT_BASE_URL = "http://127.0.0.1:30080"
DEFAULT_SERIAL_PORT = "/dev/cu.usbmodem21221401"
DEFAULT_RUNTIME_READBACK_DELAY_SEC = 2.5


class SmokeFailure(Exception):
    def __init__(self, step: str, message: str, details: Any | None = None) -> None:
        super().__init__(message)
        self.step = step
        self.message = message
        self.details = details


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Verify devd discovery, lease, USB JSONL reads, artifact dry-check, "
            "runtime write/readback, and bounded event evidence."
        )
    )
    parser.add_argument("--base-url", default=DEFAULT_BASE_URL)
    parser.add_argument("--serial-port", default=DEFAULT_SERIAL_PORT)
    parser.add_argument(
        "--device-id",
        help=(
            "Target a specific devd device ID. Native serial targets are allowed by default; "
            "mock targets require --allow-mock-device."
        ),
    )
    parser.add_argument(
        "--allow-mock-device",
        action="store_true",
        help=(
            "Allow a mock devd target selected with --device-id. This verifies the localhost "
            "HTTP contract only and must not be treated as hardware validation."
        ),
    )
    parser.add_argument("--timeout-sec", type=float, default=8.0)
    parser.add_argument(
        "--skip-runtime",
        action="store_true",
        help="Only verify discovery, lease, identity, network, and status reads.",
    )
    parser.add_argument(
        "--exercise-runtime-mutation",
        action="store_true",
        help=(
            "Temporarily change target temperature and active cooling, verify the "
            "returned status and a follow-up read, then restore the original "
            "target/cooling values. The heater is kept disabled during this check."
        ),
    )
    parser.add_argument(
        "--skip-artifact",
        action="store_true",
        help="Skip devd artifact catalog, verify, and flash dry-run checks.",
    )
    parser.add_argument(
        "--artifact-id",
        default="local-esp32s3-release",
        help="Artifact ID to dry-check through devd.",
    )
    parser.add_argument(
        "--wifi-ssid",
        help="Provision WiFi credentials over USB. This writes device configuration.",
    )
    parser.add_argument(
        "--wifi-password",
        default="",
        help="WiFi password used with --wifi-ssid. The smoke output never prints it.",
    )
    parser.add_argument(
        "--wifi-telemetry-ms",
        type=int,
        default=500,
        help="Telemetry interval used with --wifi-ssid.",
    )
    parser.add_argument(
        "--exercise-wifi-clear",
        action="store_true",
        help=(
            "After --wifi-ssid provisioning succeeds, clear WiFi credentials, "
            "verify disabled state and redacted event evidence, then restore the "
            "provided credentials."
        ),
    )
    parser.add_argument(
        "--exercise-real-flash",
        action="store_true",
        help=(
            "After all read/write checks pass, execute a real devd flash. This "
            "requires --real-flash-confirm FLASH and a devd instance started with "
            "real flashing enabled."
        ),
    )
    parser.add_argument(
        "--real-flash-confirm",
        choices=["FLASH"],
        help="Required together with --exercise-real-flash.",
    )
    parser.add_argument(
        "--real-flash-timeout-sec",
        type=float,
        default=60.0,
        help="Timeout for the optional real flash step.",
    )
    args = parser.parse_args()
    if args.exercise_wifi_clear and not args.wifi_ssid:
        parser.error("--exercise-wifi-clear requires --wifi-ssid so credentials can be restored.")
    if args.exercise_real_flash and args.skip_artifact:
        parser.error("--exercise-real-flash requires artifact checks; remove --skip-artifact.")
    if args.exercise_real_flash and args.real_flash_confirm != "FLASH":
        parser.error("--exercise-real-flash requires --real-flash-confirm FLASH.")
    if args.exercise_real_flash and args.allow_mock_device:
        parser.error("--exercise-real-flash cannot run against a mock device.")

    started_at = time.time()
    result: dict[str, Any] = {
        "ok": False,
        "baseUrl": args.base_url,
        "serialPort": args.serial_port,
        "allowMockDevice": args.allow_mock_device,
        "steps": [],
    }
    lease_id: str | None = None
    device_id: str | None = None
    artifact: dict[str, Any] | None = None
    exit_code = 0

    try:
        if args.device_id is None and not os.path.exists(args.serial_port):
            raise SmokeFailure(
                "authorized_serial_port",
                "Authorized serial port is not present; hardware smoke cannot select another port.",
                {
                    "authorizedSerialPort": args.serial_port,
                    "candidatePorts": list_usb_modem_candidates(),
                    "nextAction": (
                        "Restore the authorized port or explicitly authorize one of the "
                        "enumerated candidate ports before running hardware smoke."
                    ),
                },
            )

        health = request_json("GET", args.base_url, "/health", timeout=args.timeout_sec)
        add_step(result, "health", True, health)

        devices_payload = request_json(
            "GET", args.base_url, "/api/v1/devices", timeout=args.timeout_sec
        )
        add_step(result, "devices", True, summarize_devices(devices_payload))

        target = select_target_device(devices_payload, args.device_id, args.serial_port)
        if target is None:
            if args.device_id:
                raise SmokeFailure(
                    "target_device",
                    f"Device {args.device_id!r} was not found.",
                    summarize_devices(devices_payload),
                )
            raise SmokeFailure(
                "native_device",
                "No native_serial device matched the authorized serial port.",
                summarize_devices(devices_payload),
            )
        if target.get("transport") == "mock" and not args.allow_mock_device:
            raise SmokeFailure(
                "target_device",
                "Mock device smoke requires --allow-mock-device.",
                summarize_device(target),
            )
        device_id = target["id"]
        add_step(
            result,
            "target_device",
            True,
            summarize_device(target),
        )

        lease = request_json(
            "POST",
            args.base_url,
            f"/api/v1/devices/{quote(device_id)}/leases",
            timeout=args.timeout_sec,
        )
        lease_id = lease["leaseId"]
        add_step(
            result,
            "lease",
            True,
            {"leaseId": lease_id, "deviceId": lease.get("deviceId"), "ttlMs": lease.get("ttlMs")},
        )
        events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
        assert_device_event("lease_event", events, "lease", "lease created", "leaseId", lease_id)
        add_step(result, "lease_event", True, summarize_events(events))
        stream_events = fetch_device_event_stream(
            args.base_url,
            device_id,
            args.timeout_sec,
            stop_kind="lease",
            stop_message="lease created",
            stop_payload_key="leaseId",
            stop_payload_value=lease_id,
        )
        assert_device_event(
            "lease_event_stream",
            stream_events,
            "lease",
            "lease created",
            "leaseId",
            lease_id,
        )
        add_step(result, "lease_event_stream", True, summarize_events(stream_events))
        heartbeat_lease(result, args, lease_id, "lease_heartbeat")

        suffix = f"?{urllib.parse.urlencode({'lease_id': lease_id})}"
        identity = request_step(
            result,
            "identity",
            "GET",
            args.base_url,
            f"/api/v1/devices/{quote(device_id)}/identity{suffix}",
            timeout=args.timeout_sec,
        )
        add_step(result, "identity", True, summarize_identity(identity))

        network = request_step(
            result,
            "network",
            "GET",
            args.base_url,
            f"/api/v1/devices/{quote(device_id)}/network{suffix}",
            timeout=args.timeout_sec,
        )
        add_step(result, "network", True, summarize_network(network))

        status = request_step(
            result,
            "status",
            "GET",
            args.base_url,
            f"/api/v1/devices/{quote(device_id)}/status{suffix}",
            timeout=args.timeout_sec,
        )
        add_step(result, "status", True, summarize_status(status))

        if not args.skip_artifact:
            artifacts_payload = request_step(
                result,
                "artifacts",
                "GET",
                args.base_url,
                "/api/v1/artifacts",
                timeout=args.timeout_sec,
            )
            artifact = find_artifact(artifacts_payload, args.artifact_id)
            if artifact is None:
                raise SmokeFailure(
                    "artifact",
                    f"Artifact {args.artifact_id!r} was not found in devd catalog.",
                    summarize_artifacts(artifacts_payload),
                )
            add_step(result, "artifact", True, summarize_artifact(artifact))

            verify = request_step(
                result,
                "artifact_verify",
                "POST",
                args.base_url,
                "/api/v1/artifacts/verify",
                body={"artifact": artifact},
                timeout=args.timeout_sec,
            )
            add_step(result, "artifact_verify", True, summarize_verify(verify))
            if not verify.get("verified"):
                raise SmokeFailure(
                    "artifact_verify",
                    "Artifact verification did not pass.",
                    summarize_verify(verify),
                )

            dry_run = request_step(
                result,
                "flash_dry_run",
                "POST",
                args.base_url,
                f"/api/v1/devices/{quote(device_id)}/flash",
                body={
                    "leaseId": lease_id,
                    "artifact": artifact,
                    "dryRun": True,
                },
                timeout=args.timeout_sec,
            )
            add_step(result, "flash_dry_run", True, summarize_flash(dry_run))
            heartbeat_lease(result, args, lease_id, "flash_lease_heartbeat")
            events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
            assert_device_event(
                "flash_dry_run_event",
                events,
                "flash",
                "artifact dry-run passed",
                "artifactId",
                artifact.get("artifactId"),
            )
            add_step(result, "flash_dry_run_event", True, summarize_events(events))

        if args.wifi_ssid:
            heartbeat_lease(result, args, lease_id, "wifi_lease_heartbeat")
            provision_wifi(
                result,
                args,
                device_id,
                lease_id,
                "wifi_provision",
                "set",
                args.wifi_ssid,
                args.wifi_password,
                True,
                args.wifi_telemetry_ms,
            )

            if args.exercise_wifi_clear:
                heartbeat_lease(result, args, lease_id, "wifi_clear_lease_heartbeat")
                wifi_clear = provision_wifi(
                    result,
                    args,
                    device_id,
                    lease_id,
                    "wifi_clear",
                    "clear",
                    None,
                    "",
                    False,
                    None,
                )
                assert_network_state("wifi_clear", wifi_clear.get("network"), "disabled")
                heartbeat_lease(result, args, lease_id, "wifi_restore_lease_heartbeat")
                provision_wifi(
                    result,
                    args,
                    device_id,
                    lease_id,
                    "wifi_restore",
                    "set",
                    args.wifi_ssid,
                    args.wifi_password,
                    True,
                    args.wifi_telemetry_ms,
                )

        if not args.skip_runtime:
            runtime_request = {
                "leaseId": lease_id,
                "targetTempC": status["targetTempC"],
                "activeCoolingEnabled": status["activeCoolingEnabled"],
                "heaterEnabled": status["heaterEnabled"],
            }
            runtime_step = "runtime_idempotent"
            restore_request: dict[str, Any] | None = None
            if args.exercise_runtime_mutation:
                runtime_request = {
                    "leaseId": lease_id,
                    "targetTempC": next_target_temp(status["targetTempC"]),
                    "activeCoolingEnabled": not bool(status["activeCoolingEnabled"]),
                    "heaterEnabled": False,
                }
                runtime_step = "runtime_mutation"
                restore_request = {
                    "leaseId": lease_id,
                    "targetTempC": status["targetTempC"],
                    "activeCoolingEnabled": status["activeCoolingEnabled"],
                    "heaterEnabled": False,
                }

            heartbeat_lease(result, args, lease_id, f"{runtime_step}_lease_heartbeat")
            runtime = request_step(
                result,
                runtime_step,
                "PUT",
                args.base_url,
                f"/api/v1/devices/{quote(device_id)}/runtime",
                body=runtime_request,
                timeout=args.timeout_sec,
            )
            assert_runtime_status(runtime_step, runtime, runtime_request)
            add_step(result, runtime_step, True, summarize_status(runtime))
            events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
            assert_runtime_event_status(
                f"{runtime_step}_event",
                events,
                runtime_request,
            )
            add_step(result, f"{runtime_step}_event", True, summarize_events(events))
            heartbeat_lease(result, args, lease_id, f"{runtime_step}_readback_lease_heartbeat")
            time.sleep(DEFAULT_RUNTIME_READBACK_DELAY_SEC)

            runtime_readback = request_step(
                result,
                f"{runtime_step}_readback",
                "GET",
                args.base_url,
                f"/api/v1/devices/{quote(device_id)}/status{suffix}",
                timeout=args.timeout_sec,
            )
            assert_runtime_status(f"{runtime_step}_readback", runtime_readback, runtime_request)
            add_step(result, f"{runtime_step}_readback", True, summarize_status(runtime_readback))

            if restore_request:
                restored = request_step(
                    result,
                    "runtime_restore",
                    "PUT",
                    args.base_url,
                    f"/api/v1/devices/{quote(device_id)}/runtime",
                    body=restore_request,
                    timeout=args.timeout_sec,
                )
                assert_runtime_status("runtime_restore", restored, restore_request)
                add_step(result, "runtime_restore", True, summarize_status(restored))
                events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
                assert_runtime_event_status(
                    "runtime_restore_event",
                    events,
                    restore_request,
                )
                add_step(result, "runtime_restore_event", True, summarize_events(events))

        if args.exercise_real_flash:
            if artifact is None:
                raise SmokeFailure(
                    "real_flash",
                    "Artifact must be discovered before real flash can run.",
                )
            real_flash = request_step(
                result,
                "real_flash",
                "POST",
                args.base_url,
                f"/api/v1/devices/{quote(device_id)}/flash",
                body={
                    "leaseId": lease_id,
                    "artifact": artifact,
                    "dryRun": False,
                    "confirm": "FLASH",
                },
                timeout=args.real_flash_timeout_sec,
            )
            assert_flash_status("real_flash", real_flash, "completed")
            add_step(result, "real_flash", True, summarize_flash(real_flash))
            events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
            assert_device_event(
                "real_flash_event",
                events,
                "flash",
                "real flash completed",
                "artifactId",
                artifact.get("artifactId"),
            )
            add_step(result, "real_flash_event", True, summarize_events(events))

        result["ok"] = True
    except SmokeFailure as error:
        add_step(result, error.step, False, error.details, error.message)
        exit_code = 2
    except urllib.error.HTTPError as error:
        payload = decode_error_payload(error)
        add_step(result, "http", False, payload, f"HTTP {error.code}")
        exit_code = 3
    except Exception as error:  # noqa: BLE001 - smoke output must capture unexpected failures.
        add_step(
            result,
            "unexpected",
            False,
            {"type": type(error).__name__},
            str(error),
        )
        exit_code = 4
    finally:
        if lease_id:
            try:
                release = request_json(
                    "DELETE",
                    args.base_url,
                    f"/api/v1/leases/{quote(lease_id)}",
                    timeout=args.timeout_sec,
                )
                add_step(result, "release_lease", True, release)
            except Exception as error:  # noqa: BLE001
                add_step(
                    result,
                    "release_lease",
                    False,
                    {"type": type(error).__name__},
                    str(error),
                )
                if exit_code == 0:
                    exit_code = 5
            else:
                if device_id:
                    try:
                        events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
                        if exit_code == 0:
                            assert_device_event(
                                "release_lease_event",
                                events,
                                "lease",
                                "lease released",
                                "leaseId",
                                lease_id,
                            )
                        add_step(result, "release_lease_event", True, summarize_events(events))
                    except Exception as error:  # noqa: BLE001
                        add_step(
                            result,
                            "release_lease_event",
                            False,
                            {"type": type(error).__name__},
                            str(error),
                        )
                        if exit_code == 0:
                            exit_code = 6

    if exit_code != 0:
        result["ok"] = False
    return emit(result, started_at, exit_code)


def request_json(
    method: str,
    base_url: str,
    path: str,
    body: dict[str, Any] | None = None,
    timeout: float = 8.0,
) -> Any:
    data = None if body is None else json.dumps(body).encode()
    headers = {"content-type": "application/json"} if body is not None else {}
    request = urllib.request.Request(base_url + path, data=data, headers=headers, method=method)
    with urllib.request.urlopen(request, timeout=timeout) as response:
        raw = response.read().decode()
        return json.loads(raw) if raw else None


def request_step(
    result: dict[str, Any],
    step: str,
    method: str,
    base_url: str,
    path: str,
    body: dict[str, Any] | None = None,
    timeout: float = 8.0,
) -> Any:
    try:
        return request_json(method, base_url, path, body=body, timeout=timeout)
    except urllib.error.HTTPError as error:
        raise SmokeFailure(step, f"HTTP {error.code}", decode_error_payload(error)) from error


def heartbeat_lease(
    result: dict[str, Any],
    args: argparse.Namespace,
    lease_id: str,
    step: str,
) -> dict[str, Any]:
    heartbeat = request_step(
        result,
        step,
        "POST",
        args.base_url,
        f"/api/v1/leases/{quote(lease_id)}/heartbeat",
        timeout=args.timeout_sec,
    )
    add_step(
        result,
        step,
        True,
        {
            "leaseId": heartbeat.get("leaseId"),
            "deviceId": heartbeat.get("deviceId"),
            "ttlMs": heartbeat.get("ttlMs"),
        },
    )
    return heartbeat


def provision_wifi(
    result: dict[str, Any],
    args: argparse.Namespace,
    device_id: str,
    lease_id: str,
    step: str,
    op: str,
    ssid: str | None,
    password: str,
    auto_reconnect: bool,
    telemetry_ms: int | None,
) -> dict[str, Any]:
    body: dict[str, Any] = {
        "leaseId": lease_id,
        "op": op,
    }
    if ssid is not None:
        body["ssid"] = ssid
    if password:
        body["password"] = password
    if op == "set":
        body["autoReconnect"] = auto_reconnect
        body["telemetryIntervalMs"] = telemetry_ms

    wifi = request_step(
        result,
        step,
        "PUT",
        args.base_url,
        f"/api/v1/devices/{quote(device_id)}/wifi",
        body=body,
        timeout=args.timeout_sec,
    )
    assert_wifi_response_redacted(step, wifi, password)
    add_step(result, step, True, summarize_wifi_response(wifi))

    events = fetch_device_events(args.base_url, device_id, args.timeout_sec)
    assert_wifi_event_redacted(
        f"{step}_event",
        events,
        ssid,
        bool(password),
        password,
        op,
    )
    add_step(result, f"{step}_event", True, summarize_events(events))
    return wifi


def find_native_device(devices_payload: Any, serial_port: str) -> dict[str, Any] | None:
    devices = devices_payload.get("devices", []) if isinstance(devices_payload, dict) else []
    for device in devices:
        if device.get("transport") == "native_serial" and device.get("portPath") == serial_port:
            return device
    return None


def select_target_device(
    devices_payload: Any,
    device_id: str | None,
    serial_port: str,
) -> dict[str, Any] | None:
    if device_id:
        return find_device_by_id(devices_payload, device_id)
    return find_native_device(devices_payload, serial_port)


def find_device_by_id(devices_payload: Any, device_id: str) -> dict[str, Any] | None:
    devices = devices_payload.get("devices", []) if isinstance(devices_payload, dict) else []
    for device in devices:
        if device.get("id") == device_id:
            return device
    return None


def list_usb_modem_candidates() -> list[str]:
    candidates = set(glob.glob("/dev/cu.usbmodem*"))
    candidates.update(glob.glob("/dev/tty.usbmodem*"))
    return sorted(candidates)


def fetch_device_events(base_url: str, device_id: str, timeout: float) -> list[dict[str, Any]]:
    devices_payload = request_json("GET", base_url, "/api/v1/devices", timeout=timeout)
    device = find_device_by_id(devices_payload, device_id)
    if device is None:
        raise SmokeFailure(
            "device_events",
            "Device disappeared while collecting devd event evidence.",
            summarize_devices(devices_payload),
        )
    events = device.get("events", [])
    return events if isinstance(events, list) else []


def fetch_device_event_stream(
    base_url: str,
    device_id: str,
    timeout: float,
    stop_kind: str | None = None,
    stop_message: str | None = None,
    stop_payload_key: str | None = None,
    stop_payload_value: Any | None = None,
) -> list[dict[str, Any]]:
    request = urllib.request.Request(
        base_url + f"/api/v1/devices/{quote(device_id)}/events",
        headers={"accept": "text/event-stream"},
        method="GET",
    )
    events: list[dict[str, Any]] = []
    current_data: list[str] = []
    deadline = time.time() + timeout
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            while time.time() < deadline:
                try:
                    raw_line = response.readline()
                except TimeoutError:
                    break
                except socket.timeout:
                    break
                if not raw_line:
                    break

                line = raw_line.decode("utf-8", errors="replace").rstrip("\r\n")
                if line.startswith("data:"):
                    current_data.append(line.removeprefix("data:").strip())
                    continue
                if line == "" and current_data:
                    event = decode_sse_event("".join(current_data))
                    if event is not None:
                        events.append(event)
                        if len(events) > 32:
                            events.pop(0)
                        current_data = []
                        if event_matches(
                            event,
                            stop_kind,
                            stop_message,
                            stop_payload_key,
                            stop_payload_value,
                        ):
                            break
                    current_data = []
    except urllib.error.HTTPError as error:
        raise SmokeFailure(
            "device_event_stream",
            f"HTTP {error.code}",
            decode_error_payload(error),
        ) from error
    if current_data:
        event = decode_sse_event("".join(current_data))
        if event is not None:
            events.append(event)
    return events


def event_matches(
    event: dict[str, Any],
    kind: str | None,
    message: str | None,
    payload_key: str | None,
    payload_value: Any | None,
) -> bool:
    if kind is not None and event.get("kind") != kind:
        return False
    if message is not None and event.get("message") != message:
        return False
    if payload_key is not None:
        payload = event.get("payload", {}) if isinstance(event.get("payload"), dict) else {}
        return payload.get(payload_key) == payload_value
    return kind is not None or message is not None


def decode_sse_event(data: str) -> dict[str, Any] | None:
    try:
        event = json.loads(data)
    except json.JSONDecodeError:
        return None
    return event if isinstance(event, dict) else None


def assert_device_event(
    step: str,
    events: list[dict[str, Any]],
    kind: str,
    message: str,
    payload_key: str | None = None,
    payload_value: Any | None = None,
) -> None:
    for event in events:
        payload = event.get("payload", {}) if isinstance(event.get("payload"), dict) else {}
        payload_matches = payload_key is None or payload.get(payload_key) == payload_value
        if event.get("kind") == kind and event.get("message") == message and payload_matches:
            return
    raise SmokeFailure(
        step,
        f"Expected devd event {kind}:{message} was not recorded.",
        summarize_events(events),
    )


def assert_runtime_event_status(
    step: str,
    events: list[dict[str, Any]],
    request: dict[str, Any],
) -> None:
    for event in reversed(events):
        if event.get("kind") != "runtime" or event.get("message") != "runtime config applied":
            continue
        payload = event.get("payload", {}) if isinstance(event.get("payload"), dict) else {}
        status = payload.get("status", {}) if isinstance(payload.get("status"), dict) else {}
        mismatches: dict[str, dict[str, Any]] = {}
        expected_fields = {
            "targetTempC": request.get("targetTempC"),
            "activeCoolingEnabled": request.get("activeCoolingEnabled"),
            "heaterEnabled": request.get("heaterEnabled"),
        }
        for field, expected in expected_fields.items():
            if expected is not None and status.get(field) != expected:
                mismatches[field] = {"expected": expected, "actual": status.get(field)}
        if not mismatches:
            return
    raise SmokeFailure(
        step,
        "Expected runtime config event was not recorded with matching status.",
        summarize_events(events),
    )


def assert_wifi_event_redacted(
    step: str,
    events: list[dict[str, Any]],
    ssid: str | None,
    password_present: bool,
    password: str,
    op: str,
) -> None:
    for event in reversed(events):
        if event.get("kind") != "wifi" or event.get("message") != "wifi config accepted":
            continue
        payload = event.get("payload", {}) if isinstance(event.get("payload"), dict) else {}
        if (
            payload.get("op") != op
            or payload.get("ssid") != ssid
            or payload.get("passwordPresent") != password_present
        ):
            continue
        encoded = json.dumps(payload, sort_keys=True)
        if password and password in encoded:
            raise SmokeFailure(
                step,
                "WiFi event leaked the raw password.",
                summarize_events(events),
            )
        return
    raise SmokeFailure(
        step,
        "Expected WiFi config event was not recorded with matching redacted summary.",
        summarize_events(events),
    )


def assert_wifi_response_redacted(step: str, wifi: Any, password: str) -> None:
    wifi_summary = wifi.get("wifi", {}) if isinstance(wifi, dict) else {}
    encoded = json.dumps(wifi, sort_keys=True)
    if password and password in encoded:
        raise SmokeFailure(
            step,
            "WiFi response leaked the raw password.",
            summarize_wifi_response(wifi) if isinstance(wifi, dict) else {"response": wifi},
        )
    if password and wifi_summary.get("password") != "<redacted>":
        raise SmokeFailure(
            step,
            "WiFi response did not include the expected redacted password marker.",
            summarize_wifi_response(wifi) if isinstance(wifi, dict) else {"response": wifi},
        )
    if not password and wifi_summary.get("password") not in (None, "<redacted>"):
        raise SmokeFailure(
            step,
            "WiFi response included an unexpected password marker.",
            summarize_wifi_response(wifi) if isinstance(wifi, dict) else {"response": wifi},
        )


def assert_network_state(step: str, network: Any, state: str) -> None:
    actual = network.get("state") if isinstance(network, dict) else None
    if actual != state:
        raise SmokeFailure(
            step,
            f"Expected network state {state!r}.",
            {"expected": state, "actual": actual},
        )


def assert_flash_status(step: str, flash: Any, status: str) -> None:
    actual = flash.get("status") if isinstance(flash, dict) else None
    if actual != status:
        raise SmokeFailure(
            step,
            f"Expected flash status {status!r}.",
            {"expected": status, "actual": actual},
        )


def summarize_devices(devices_payload: Any) -> dict[str, Any]:
    devices = devices_payload.get("devices", []) if isinstance(devices_payload, dict) else []
    return {
        "count": len(devices),
        "devices": [summarize_device(device) for device in devices],
    }


def summarize_device(device: dict[str, Any]) -> dict[str, Any]:
    identity = device.get("identity", {}) if isinstance(device.get("identity"), dict) else {}
    return {
        "id": device.get("id"),
        "displayName": device.get("displayName"),
        "transport": device.get("transport"),
        "connection": device.get("connection"),
        "portPath": device.get("portPath"),
        "capabilities": identity.get("capabilities", []),
    }


def summarize_events(events: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "count": len(events),
        "events": [summarize_event(event) for event in events[-8:]],
    }


def summarize_event(event: dict[str, Any]) -> dict[str, Any]:
    payload = event.get("payload", {}) if isinstance(event.get("payload"), dict) else {}
    status = payload.get("status", {}) if isinstance(payload.get("status"), dict) else {}
    return {
        "kind": event.get("kind"),
        "message": event.get("message"),
        "stage": payload.get("stage"),
        "code": payload.get("code"),
        "op": payload.get("op"),
        "artifactId": payload.get("artifactId"),
        "leaseId": payload.get("leaseId"),
        "ssid": payload.get("ssid"),
        "passwordPresent": payload.get("passwordPresent"),
        "targetTempC": status.get("targetTempC"),
        "activeCoolingEnabled": status.get("activeCoolingEnabled"),
        "heaterEnabled": status.get("heaterEnabled"),
    }


def summarize_identity(identity: dict[str, Any]) -> dict[str, Any]:
    return {
        "deviceId": identity.get("deviceId"),
        "firmwareVersion": identity.get("firmwareVersion"),
        "buildId": identity.get("buildId"),
        "protocolVersion": identity.get("protocolVersion"),
        "capabilities": identity.get("capabilities", []),
    }


def summarize_network(network: dict[str, Any]) -> dict[str, Any]:
    return {
        "state": network.get("state"),
        "ssid": network.get("ssid"),
        "ip": network.get("ip"),
        "wifiRssi": network.get("wifiRssi"),
        "lastError": network.get("lastError"),
    }


def summarize_status(status: dict[str, Any]) -> dict[str, Any]:
    return {
        "mode": status.get("mode"),
        "uptimeSeconds": status.get("uptimeSeconds"),
        "currentTempC": status.get("currentTempC"),
        "targetTempC": status.get("targetTempC"),
        "heaterEnabled": status.get("heaterEnabled"),
        "heaterOutputPercent": status.get("heaterOutputPercent"),
        "activeCoolingEnabled": status.get("activeCoolingEnabled"),
        "fanDisplayState": status.get("fanDisplayState"),
        "pdState": status.get("pdState"),
    }


def next_target_temp(value: Any) -> int:
    current = int(value)
    return current + 1 if current < 400 else current - 1


def assert_runtime_status(step: str, status: dict[str, Any], request: dict[str, Any]) -> None:
    mismatches: dict[str, dict[str, Any]] = {}
    expected_fields = {
        "targetTempC": request.get("targetTempC"),
        "activeCoolingEnabled": request.get("activeCoolingEnabled"),
        "heaterEnabled": request.get("heaterEnabled"),
    }
    for field, expected in expected_fields.items():
        if expected is not None and status.get(field) != expected:
            mismatches[field] = {"expected": expected, "actual": status.get(field)}
    if mismatches:
        raise SmokeFailure(
            step,
            "Runtime status did not reflect the requested configuration.",
            mismatches,
        )


def find_artifact(artifacts_payload: Any, artifact_id: str) -> dict[str, Any] | None:
    artifacts = artifacts_payload.get("artifacts", []) if isinstance(artifacts_payload, dict) else []
    for artifact in artifacts:
        if artifact.get("artifactId") == artifact_id:
            return artifact
    return None


def summarize_artifacts(artifacts_payload: Any) -> dict[str, Any]:
    artifacts = artifacts_payload.get("artifacts", []) if isinstance(artifacts_payload, dict) else []
    return {
        "count": len(artifacts),
        "artifacts": [
            {
                "artifactId": artifact.get("artifactId"),
                "targetChip": artifact.get("targetChip"),
                "profile": artifact.get("profile"),
                "protocol": artifact.get("protocol"),
            }
            for artifact in artifacts
        ],
    }


def summarize_artifact(artifact: dict[str, Any]) -> dict[str, Any]:
    files = artifact.get("files", [])
    return {
        "artifactId": artifact.get("artifactId"),
        "targetChip": artifact.get("targetChip"),
        "profile": artifact.get("profile"),
        "features": artifact.get("features", []),
        "fileCount": len(files),
    }


def summarize_verify(verify: dict[str, Any]) -> dict[str, Any]:
    return {
        "artifactId": verify.get("artifactId"),
        "verified": verify.get("verified"),
        "files": [
            {
                "kind": file.get("kind"),
                "ok": file.get("ok"),
                "size": file.get("size"),
                "sha256": file.get("sha256"),
            }
            for file in verify.get("files", [])
        ],
    }


def summarize_flash(flash: dict[str, Any]) -> dict[str, Any]:
    return {
        "artifactId": flash.get("artifactId"),
        "dryRun": flash.get("dryRun"),
        "status": flash.get("status"),
        "message": flash.get("message"),
    }


def summarize_wifi_response(wifi: dict[str, Any]) -> dict[str, Any]:
    network = wifi.get("network", {}) if isinstance(wifi, dict) else {}
    wifi_summary = wifi.get("wifi", {}) if isinstance(wifi, dict) else {}
    return {
        "accepted": wifi.get("accepted") if isinstance(wifi, dict) else None,
        "network": summarize_network(network),
        "wifi": {
            "op": wifi_summary.get("op"),
            "ssid": wifi_summary.get("ssid"),
            "password": wifi_summary.get("password"),
            "autoReconnect": wifi_summary.get("autoReconnect"),
            "telemetryIntervalMs": wifi_summary.get("telemetryIntervalMs"),
        },
    }


def decode_error_payload(error: urllib.error.HTTPError) -> Any:
    raw = error.read().decode()
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        return {"body": raw}


def add_step(
    result: dict[str, Any],
    name: str,
    ok: bool,
    details: Any | None = None,
    error: str | None = None,
) -> None:
    step: dict[str, Any] = {"name": name, "ok": ok}
    if details is not None:
        step["details"] = details
    if error is not None:
        step["error"] = error
    result["steps"].append(step)


def quote(value: str) -> str:
    return urllib.parse.quote(value, safe="")


def emit(result: dict[str, Any], started_at: float, code: int) -> int:
    result["durationMs"] = int((time.time() - started_at) * 1000)
    print(json.dumps(result, indent=2, sort_keys=True))
    return code


if __name__ == "__main__":
    sys.exit(main())
