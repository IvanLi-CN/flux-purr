# Flux Purr HTTP + WS Contract

Base URL: `http://<device-ip>`

## Overview

- Version prefix: `/api/v1`
- Auth: `TBD` (local network bootstrap mode for init phase)
- Content type: `application/json`
- Time format: RFC3339 with timezone offset

## REST APIs

### `GET /api/v1/device/info`

Returns firmware identity and runtime mode.

Response example:

```json
{
  "deviceId": "flux-purr-s3-001",
  "fwVersion": "fw/v0.1.0-dev",
  "board": "esp32-s3",
  "mode": "sampling"
}
```

### `GET /api/v1/device/status`

Returns latest sampled electrical and thermal metrics.

Response example:

```json
{
  "voltageMv": 12080,
  "currentMa": 830,
  "boardTempCenti": 3460,
  "wifiRssi": -58,
  "lastSync": "2026-03-02T18:05:00+08:00"
}
```

### `PUT /api/v1/config/wifi`

Updates STA/AP behavior and telemetry interval.

Request example:

```json
{
  "ssid": "FluxPurr-Lab",
  "password": "<secret>",
  "autoReconnect": true,
  "telemetryIntervalMs": 500
}
```

Response:

- `200 OK` on success.
- `400 Bad Request` for invalid payload.

### `POST /api/v1/device/reboot`

Asks MCU control plane to reboot.

Response:

- `202 Accepted` when reboot job is queued.

## WebSocket

### `GET /api/v1/telemetry/ws`

Streams incremental telemetry frames.

Frame example:

```json
{
  "ts": "18:05",
  "voltage": 12.08,
  "current": 0.83
}
```

## Compatibility

- Firmware S3 baseline and Web console must preserve field names defined above.
- If MCU target switches from S3 to C3, contract version remains `/api/v1` unless a breaking change is approved.
