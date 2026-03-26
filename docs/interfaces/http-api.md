# Flux Purr HTTP + WS Contract

Source of truth for this implementation scope:
`docs/specs/233y7-c3-ch224q-ch442e-frontpanel/SPEC.md`

Base URL: `http://<device-ip>`

## `GET /api/v1/device/info`

Returns firmware identity and runtime mode.

```json
{
  "deviceId": "flux-purr-c3-001",
  "fwVersion": "fw/v0.2.0-dev",
  "board": "esp32-c3",
  "mode": "sampling"
}
```

## `GET /api/v1/device/status`

Returns latest sampled electrical, PD, and thermal metrics.

```json
{
  "voltageMv": 28010,
  "currentMa": 840,
  "boardTempCenti": 3460,
  "pdRequestMv": 28000,
  "pdContractMv": 28000,
  "pdState": "ready",
  "fanEnabled": true,
  "fanPwmPermille": 720,
  "frontpanelKey": "center",
  "wifiRssi": -58,
  "lastSync": "2026-03-03T20:05:00+08:00"
}
```

Field notes:

- `voltageMv`: reconstructed input voltage from the `GPIO1` divider (`56 kOhm / 5.1 kOhm` nominal)
- `pdState`: `negotiating | ready | fallback_5v | fault`
- `frontpanelKey`: `center | right | down | left | up | null`

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
  "ts": "20:05",
  "voltage": 28.01,
  "current": 0.84,
  "pdContractMv": 28000
}
```
