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
- `POST /api/v1/artifacts/verify`
- `POST /api/v1/devices/:id/flash`

Mutating device endpoints require a valid `lease_id`.

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
    "network": {
      "state": "saving",
      "ssid": "FluxPurr-Lab"
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
