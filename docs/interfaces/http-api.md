# Flux Purr Control Plane HTTP + USB Contract

Source of truth for this implementation scope:

- `docs/specs/m8r4q-real-control-plane-runtime/SPEC.md`
- `docs/solutions/device-control/web-native-wifi-bridge-console.md`

## Shared Models

All transports expose the same domain model. Field names use `camelCase` on HTTP/JSON.

### `Identity`

```json
{
  "deviceId": "flux-purr-s3-001",
  "firmwareVersion": "fw/v0.4.0-dev",
  "buildId": "s3-7f31c9",
  "gitSha": "8b8b17c",
  "board": "esp32-s3",
  "apiVersion": "2026-05-23",
  "protocolVersion": "flux-purr.usb.v1",
  "hostname": "flux-purr-s3-001",
  "capabilities": ["identity", "status", "network", "wifi_config", "monitor", "firmware_check"]
}
```

### `NetworkSummary`

```json
{
  "state": "connected",
  "ssid": "FluxPurr-Lab",
  "ip": "192.168.31.42",
  "gateway": "192.168.31.1",
  "dns": ["192.168.31.1"],
  "wifiRssi": -54,
  "lastError": null
}
```

`state`: `disabled | idle | saving | connecting | connected | error | timeout`.

### `Status`

```json
{
  "mode": "sampling",
  "uptimeSeconds": 123,
  "currentTempC": 183.6,
  "targetTempC": 220,
  "heaterEnabled": true,
  "heaterOutputPercent": 22,
  "activeCoolingEnabled": true,
  "fanDisplayState": "AUTO",
  "fanEnabled": true,
  "fanPwmPermille": 500,
  "voltageMv": 20010,
  "currentMa": 840,
  "boardTempCenti": 3840,
  "pdRequestMv": 20000,
  "pdContractMv": 20000,
  "pdState": "ready",
  "frontpanelKey": null,
  "network": { "state": "connected", "wifiRssi": -54 }
}
```

`pdState`: `negotiating | ready | fallback_5v | fault`.
`fanDisplayState`: `OFF | AUTO | RUN`.

### `FirmwareArtifact`

```json
{
  "artifactId": "local-esp32s3-release",
  "name": "Local ESP32-S3 release",
  "version": "local-build",
  "gitSha": "unknown",
  "buildId": "54362508abf2",
  "targetChip": "esp32s3",
  "profile": "release + web_serial",
  "features": ["web_serial"],
  "protocol": "flux-purr.usb.v1",
  "files": [
    {
      "kind": "elf",
      "path": "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
      "sha256": "sha256:54362508abf2a6148b6aecba23032c7b67bf346bf288a7ae1aaccf24c68af113",
      "size": 741452,
      "flashAddress": null
    }
  ]
}
```

`devd` computes file size and `sha256` from local build outputs before returning catalog entries. Paths are repo-relative and must not expose unrelated host paths in errors. The local ESP32-S3 release artifact is an ELF and is flashed with `espflash flash`; `flashAddress` is only set for raw app binaries flashed with `espflash write-bin`.

### `ApiError`

```json
{
  "error": {
    "code": "lease_required",
    "message": "A valid device lease is required.",
    "retryable": true,
    "details": null
  }
}
```

Errors must not include WiFi passwords, PSK values, or unrelated host paths.

## Device HTTP

Direct device HTTP is a planned transport surface for a future firmware `net_http` server. Current ESP32-S3 release artifacts do not implement or advertise this transport; real hardware control uses Native `devd` HTTP over USB JSONL.

Base URL: `http://<device-ip>`.

- `GET /api/v1/identity`
- `GET /api/v1/network`
- `GET /api/v1/status`
- `PUT /api/v1/wifi`
- `POST /api/v1/reboot`
- `GET /api/v1/events`

`PUT /api/v1/wifi` body:

```json
{
  "op": "set",
  "ssid": "FluxPurr-Lab",
  "password": "<secret>",
  "autoReconnect": true,
  "telemetryIntervalMs": 500
}
```

The response reports a redacted summary only:

```json
{
  "accepted": true,
  "network": {
    "state": "saving",
    "ssid": "FluxPurr-Lab",
    "wifiRssi": null,
    "lastError": null
  }
}
```

## Native `devd` HTTP

Base URL: `http://127.0.0.1:<port>`. Default bind is `127.0.0.1:30080`.
Native serial discovery is constrained to the configured authorized port. The project default is `/dev/cu.usbmodem21221401`; if that path is absent, `devd` must not expose another native serial device.

- `GET /health`
- `GET /api/v1/devices`
- `POST /api/v1/devices/:id/bind`
- `POST /api/v1/devices/:id/connect`
- `POST /api/v1/devices/:id/disconnect`
- `POST /api/v1/devices/:id/leases`
- `POST /api/v1/leases/:lease_id/heartbeat`
- `DELETE /api/v1/leases/:lease_id`
- `GET /api/v1/devices/:id/identity?lease_id=...`
- `GET /api/v1/devices/:id/network?lease_id=...`
- `GET /api/v1/devices/:id/status?lease_id=...`
- `GET /api/v1/devices/:id/events`
- `PUT /api/v1/devices/:id/wifi`
- `PUT /api/v1/devices/:id/runtime`
- `GET /api/v1/artifacts`
- `POST /api/v1/artifacts/verify`
- `POST /api/v1/devices/:id/flash`

Mutating device endpoints require a valid lease. `bind`, `connect`, `disconnect`, and leased read endpoints pass it as `?lease_id=...`; JSON-body write endpoints use `leaseId`.

`POST /api/v1/devices/:id/bind?lease_id=...` body:

```json
{
  "alias": "Bench Alias"
}
```

`POST /api/v1/devices/:id/connect?lease_id=...` and `POST /api/v1/devices/:id/disconnect?lease_id=...` return the updated daemon-local `DeviceRecord`.

`GET /api/v1/devices/:id/events` returns `text/event-stream`. The stream first replays that device's bounded event backlog, then continues with live events. Each SSE event name matches the `kind` field (`serial`, `lease`, `wifi`, `runtime`, `flash`, etc.) and each `data` frame is a `DevdEvent` JSON object. Events are scoped to the requested device ID.

`PUT /api/v1/devices/:id/runtime` body:

```json
{
  "leaseId": "lease-001",
  "targetTempC": 220,
  "activeCoolingEnabled": true,
  "heaterEnabled": true
}
```

All runtime fields are optional except `leaseId`; the response is the updated `Status`.

`GET /api/v1/artifacts` response:

```json
{
  "artifacts": [
    {
      "artifactId": "local-esp32s3-release",
      "targetChip": "esp32s3",
      "files": [
        {
          "kind": "elf",
          "path": "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
          "sha256": "sha256:54362508abf2a6148b6aecba23032c7b67bf346bf288a7ae1aaccf24c68af113",
          "size": 741452,
          "flashAddress": null
        }
      ]
    }
  ]
}
```

`POST /api/v1/artifacts/verify` accepts one `FirmwareArtifact` manifest and validates every file's existence, size, and SHA-256:

```json
{
  "artifact": {
    "artifactId": "local-esp32s3-release",
    "targetChip": "esp32s3",
    "files": [
      {
        "kind": "elf",
        "path": "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
        "sha256": "sha256:54362508abf2a6148b6aecba23032c7b67bf346bf288a7ae1aaccf24c68af113",
        "size": 741452,
        "flashAddress": null
      }
    ]
  }
}
```

Successful response:

```json
{
  "verified": true,
  "artifactId": "local-esp32s3-release",
  "files": [
    {
      "path": "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr",
      "sha256": "sha256:54362508abf2a6148b6aecba23032c7b67bf346bf288a7ae1aaccf24c68af113",
      "size": 741452
    }
  ]
}
```

Web Update dry-check uses the catalog plus verify endpoint. Browser CORS preflight for development must allow `Content-Type` so JSON `POST /api/v1/artifacts/verify` works from Vite.

## USB CDC JSONL

Each frame is UTF-8 JSON followed by `\n`.

### `hello`

```json
{
  "type": "hello",
  "protocolVersion": "flux-purr.usb.v1",
  "framing": "jsonl",
  "identity": { "deviceId": "flux-purr-s3-001" },
  "capabilities": ["identity", "status", "network", "wifi_config", "monitor"]
}
```

### `request`

```json
{
  "type": "request",
  "requestId": "req-001",
  "op": "get_status"
}
```

`op`: `get_identity | get_network | get_status | set_log_level`.

### `wifi_config`

```json
{
  "type": "wifi_config",
  "requestId": "req-002",
  "op": "set",
  "ssid": "FluxPurr-Lab",
  "password": "<secret>",
  "autoReconnect": true,
  "telemetryIntervalMs": 500
}
```

Responses must redact the password:

```json
{
  "type": "response",
  "requestId": "req-002",
  "ok": true,
  "result": {
    "wifi": {
      "op": "set",
      "ssid": "FluxPurr-Lab",
      "password": "<redacted>",
      "autoReconnect": true,
      "telemetryIntervalMs": 500
    }
  }
}
```

### `runtime_config`

```json
{
  "type": "runtime_config",
  "requestId": "req-003",
  "targetTempC": 220,
  "activeCoolingEnabled": true,
  "heaterEnabled": true
}
```

The response returns the updated status:

```json
{
  "type": "response",
  "requestId": "req-003",
  "ok": true,
  "result": {
    "status": {
      "targetTempC": 220,
      "activeCoolingEnabled": true,
      "heaterEnabled": true
    }
  }
}
```

### `error`

```json
{
  "type": "error",
  "requestId": "req-002",
  "error": {
    "code": "bad_frame",
    "message": "Malformed JSONL frame.",
    "retryable": false
  }
}
```
