# Flux Purr HTTP + WS Contract

Source of truth for this initialization scope:
`docs/specs/n6csh-flux-purr-init/contracts/http-apis.md`

Base URL: `http://<device-ip>`

## `GET /api/v1/device/info`

Returns firmware identity and runtime mode.

```json
{
  "deviceId": "flux-purr-s3-001",
  "fwVersion": "fw/v0.1.0-dev",
  "board": "esp32-s3",
  "mode": "sampling"
}
```

## `GET /api/v1/device/status`

Returns latest sampled electrical and thermal metrics.

```json
{
  "voltageMv": 12080,
  "currentMa": 830,
  "boardTempCenti": 3460,
  "wifiRssi": -58,
  "lastSync": "2026-03-02T18:05:00+08:00"
}
```

## `PUT /api/v1/config/wifi`

Updates STA/AP behavior and telemetry interval.

```json
{
  "ssid": "FluxPurr-Lab",
  "password": "<secret>",
  "autoReconnect": true,
  "telemetryIntervalMs": 500
}
```

## `POST /api/v1/device/reboot`

Asks MCU control plane to reboot.

## `GET /api/v1/telemetry/ws`

WebSocket stream for incremental telemetry frames.

```json
{
  "ts": "18:05",
  "voltage": 12.08,
  "current": 0.83
}
```
