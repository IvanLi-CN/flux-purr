# Flux Purr Control Plane HTTP + USB Contract

Source of truth for this implementation scope:

- `docs/specs/m8r4q-real-control-plane-runtime/SPEC.md`
- `docs/specs/jt8r2-adc-calibration-control-plane/SPEC.md`
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
  "capabilities": ["identity", "status", "network", "wifi_config", "monitor", "firmware_check", "calibration"]
}
```

### `NetworkSummary`

```json
{
  "state": "connected",
  "configurationGeneration": 7,
  "transitionSequence": 19,
  "failureCode": null,
  "ssid": "FluxPurr-Lab",
  "wifiPasswordLength": 11,
  "ip": "192.168.31.42",
  "gateway": "192.168.31.1",
  "dns": ["192.168.31.1"],
  "wifiRssi": -54,
  "lastError": null
}
```

`state`: `disabled | connecting | connected | error` for device-published WiFi facts. `idle`, `saving`, and `timeout` are not valid firmware WiFi summary states; timeout is settled as `error`. `configurationGeneration` changes for every accepted set/clear configuration; `transitionSequence` increases on every accepted state transition. `failureCode` is absent for nonterminal states and is one of `disconnect_timed_out | configuration_failed | association_rejected | association_timed_out | ipv4_timed_out | station_disconnected | lan_startup_failed`. A configuration transaction makes at most three attempts in 30 seconds: disconnect is bounded at 3 seconds, association at 8 seconds per attempt, and IPv4/DHCP at 15 seconds per attempt, with the 30-second transaction deadline taking precedence. Recoverable failures remain `connecting`; once the attempt budget or transaction deadline is exhausted, the device publishes one terminal `error`. The same configuration generation never starts a background recovery; a new set/clear configuration is required.

During firmware boot, before EEPROM/flash restoration and WiFi task startup complete, USB `get_network` and `get_status` return the retryable `startup_busy` error instead of a placeholder `disabled` snapshot. `devd` retries that boundary; clients must not persist or display a network state until a versioned `NetworkSummary` is returned by the running device.

`ssid` is the device-confirmed configured network name and is safe to display in a configuration form. `wifiPasswordLength` is the saved WiFi password's UTF-8 byte length. The password itself is never returned by USB, LAN, devd, logs, events, errors, or exports.

### `Status`

```json
{
  "mode": "sampling",
  "uptimeSeconds": 123,
  "currentTempC": 183.6,
  "targetTempC": 220,
  "selectedPresetSlot": 3,
  "presetsC": [50, 100, 120, 150, 180, 200, 210, 220, 250, 300],
  "heaterEnabled": true,
  "heaterOutputPercent": 22,
  "faultAttentionPending": false,
  "activeCoolingEnabled": true,
  "fanDisplayState": "AUTO",
  "fanEnabled": true,
  "fanPwmPermille": 500,
  "voltageMv": 20010,
  "currentMa": 3000,
  "boardTempCenti": 3840,
  "rtdRawAdcMv": 1123,
  "vinRawAdcMv": 1678,
  "pdRequestMv": 20000,
  "pdContractMv": 20000,
  "pdState": "ready",
  "manualPpsEnabled": false,
  "manualPpsMv": null,
  "manualPpsMa": null,
  "ppsCapabilityMinMv": 5000,
  "ppsCapabilityMaxMv": 21000,
  "ppsCapabilityMaxMa": 3000,
  "manualPpsError": null,
  "thermalControlProfilePreview": false,
  "frontpanelKey": null,
  "network": { "state": "connected", "wifiRssi": -54 }
}
```

`pdState`: `negotiating | ready | fallback_5v | fault`.
`fanDisplayState`: `OFF | AUTO | RUN`.
`presetsC` has exactly 10 entries; a numeric entry is an enabled preset temperature in Celsius, and `null` means the slot is disabled (`---` on the front panel).
`voltageMv` is the calibrated measured VIN input voltage. `pdContractMv` remains the PD contract or negotiated target concept. `currentMa` is the current PD/CH224Q capability value surfaced by firmware today; it is not a verified live load-current measurement, and is used as a CC-loop proxy when tooling evaluates the heater temperature/resistance curve.
`rtdRawAdcMv` and `vinRawAdcMv` expose the latest raw RTD/VIN ADC millivolt readings for calibration capture and host-side diagnostics.
`faultAttentionPending=true` only means a `temp >= 420°C` thermal-runaway event has fallen below `420°C` and still awaits acknowledgement. RTD open/short and ADC read failure do not set this field. Owner-facing temperature remains the last valid RTD value while a measurement fault is active; unavailable transport state must not synthesize `0°C`.
`manualPps*` remains the debug-only PPS override surface. Owner-facing calibration mode control uses `status.calibration` / `runtime_config.calibration` as its semantic source of truth. `thermalControlProfilePreview=true` means the firmware is using a RAM-only thermal profile preview; `clear_preview` returns to the persistent saved profile or factory default curve. `thermalControl` is the resolved controller input for the current target, not an echo of the last request: it reports whether a profile is active and covers the target, its source (`default` / `preview` / `saved`), and the effective power, damping, PI, lead, filter, warmup-reentry, adjustable-voltage-floor, and `heaterCurrentReserveMa` parameters after interpolation, legacy-profile inflate when importing old data, and safety clamps are applied. The current reserve is subtracted from the lower of PPS capability current and live CH224Q current before the heater voltage ceiling is calculated, leaving source margin for board power and conversion loss.

### `CalibrationState`

```json
{
  "active": {
    "rtdAdc": [null, null, null, null, null, null, null, null],
    "vinAdc": [null, null, null, null, null, null, null, null]
  },
  "draft": {
    "rtdAdc": [{ "observedMv": 1120, "expectedMv": 1118, "referenceTempC": 25.0 }, null, null, null, null, null, null, null],
    "vinAdc": [{ "observedMv": 1670, "expectedMv": 1820, "referenceVinMv": 20000 }, null, null, null, null, null, null, null]
  },
  "activeFit": {
    "rtdAdc": { "gain": 1.0, "offsetMv": 0.0, "customSampleCount": 0, "defaultSampleCount": 2 },
    "vinAdc": { "gain": 1.0, "offsetMv": 0.0, "customSampleCount": 0, "defaultSampleCount": 2 }
  },
  "draftFit": {
    "rtdAdc": { "gain": 1.0, "offsetMv": 0.0, "customSampleCount": 1, "defaultSampleCount": 2 },
    "vinAdc": { "gain": 1.0, "offsetMv": 0.0, "customSampleCount": 1, "defaultSampleCount": 2 }
  }
}
```

Calibration channels are `rtd_adc` and `vin_adc`. Each channel stores up to eight ADC-domain samples and also preserves the owner-entered physical reference (`referenceTempC` for RTD, `referenceVinMv` for VIN) whenever one was provided. Capture commands accept those physical references and convert them into expected ADC millivolts using the RTD/PT1000 or VIN divider model. Import replaces the full draft package.

### `CalibrationRuntimeState`

```json
{
  "mode": "vin_adc",
  "ppsEnabled": true,
  "ppsMv": 12000,
  "heaterEnabled": false,
  "targetAdcMv": null,
  "stable": true,
  "stabilityErrorMv": 0,
  "error": null,
  "job": {
    "kind": "vin_adc_auto",
    "status": "running",
    "progressPercent": 38,
    "samplesCollected": 3,
    "nextRequestMv": 13000,
    "message": null
  }
}
```

Owner-facing calibration modes are fixed as:

- `vin_adc` => `电压读数标定`
- `rtd_adc` => `温度标定`
- `heater_curve` => `加热曲线标定`
- `thermal_plant` => `双点热模型标定（20V / >=3A PPS）`

Calibration live control is PPS-only. Any requested PPS value must stay within the hardware `5V~28V` safety range and the device's real-time PPS capability. The effective request window is therefore `max(5V, ppsCapabilityMinMv)` through `min(28V, ppsCapabilityMaxMv)`.

### `HeaterCurveState`

```json
{
  "active": {
    "points": [
      { "tempCentiC": 13977, "resistanceMilliohms": 6033 },
      { "tempCentiC": 18153, "resistanceMilliohms": 6522 },
      null,
      null,
      null,
      null,
      null,
      null
    ]
  },
  "preview": null
}
```

Heater curve points store temperature in centi-Celsius and effective resistance in milliohms. `preview` is runtime-only and can be used immediately by heater power limiting logic. `save` copies the preview curve to `active`; only `active` is persisted in device memory and restored after reboot. Firmware uses external EEPROM as the primary memory backend and falls back to an ESP flash data/NVS sector when EEPROM is unavailable.

### `ThermalPlantModel`

```json
{
  "state": "active",
  "activeTransactionId": 1431374617,
  "projectionValid": true,
  "convectionMwPerC": 0.0,
  "radiationMwPerK4": 0.0000014616974,
  "thermalCapacityMjPerC": 42576.72,
  "transportDelayMs": 10000
}
```

`state` is `missing`, `active`, or `invalid`. The persistent source of truth is a complete raw two-anchor transaction: ambient/target RTD ADC, V/I, gate-off and hold power, ramp duration, and delivered energy. The displayed coefficients are derived from the current RTD calibration. Recalibrating RTD rebuilds the projection without rewriting or invalidating raw heat-control observations. Automatic `thermal_plant_auto` calibration writes a physically valid projection directly to `active`; it does not require a user validation or promotion step. Production heating requires an active model, a PPS APDO covering `20V`, and at least `3A` together with the calibrated heater curve needed for power limiting.

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
      "path": "target/xtensa-esp32s3-none-elf/release/flux-purr",
      "sha256": "sha256:54362508abf2a6148b6aecba23032c7b67bf346bf288a7ae1aaccf24c68af113",
      "size": 741452,
      "flashAddress": null
    }
  ]
}
```

`devd` computes file size and `sha256` from local build outputs before returning catalog entries. Paths are repo-relative and must not expose unrelated host paths in errors. The local ESP32-S3 release artifact is an ELF and is flashed with `espflash flash`; an authorized native USB Serial/JTAG `cu.usbmodem*` port uses `--before usb-reset`, while other serial paths retain `default-reset`. `flashAddress` is only set for raw app binaries. For a raw app, devd writes the checked-in `firmware/partitions.bin` at `0x8000`, writes the app at its explicit address, then explicitly resets the target so the `flux_cfg` layout is installed and the application starts.

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

Direct device HTTP is the WiFi/LAN control plane. The device uses DHCP by default, sends its MAC-derived `flux-purr-<mac>` hostname as DHCP option 12, and announces `_http._tcp.local` with `api=v1`, `path=/api/v1`, `pairing=frontpanel`, and `device=<mac>` TXT metadata. USB/devd remains the only path for initial WiFi provisioning, firmware flash, static IPv4 configuration, and pairing-token reset.

The WiFi station uses bounded driver buffers sized for control traffic. A WiFi-driver or LAN-task startup failure is published in `NetworkSummary` as `state=error`; it must not prevent the USB JSONL control and recovery loop from becoming available.

Base URL: `http://<device-ip>` or the MAC-derived `http://flux-purr-<mac>.local` hostname. Manual LAN targets must be RFC1918 IPv4 addresses or that device hostname; public and arbitrary DNS targets are rejected by Web and devd clients.

Public endpoints:

- `GET /health` is the anonymous, low-frequency connection summary. It returns `api`, MAC-derived `deviceId` and `hostname`, firmware version, and `{ pairing: { mode, active, attemptsRemaining } }`; it never returns runtime telemetry, a code, or a bearer token. `mode` is `required` (current default), `optional` (the device may claim without a code), or `unavailable` (basic public information only).
- `GET /api/v1/pairing` returns the same current policy and whether a code is currently visible, never the code.
- `POST /api/v1/pairing/claim` returns the stable bearer token plus MAC-derived `deviceId` and `hostname`. `required` accepts `{ "code": "4827" }` only after the physical WiFi Info page opens the pairing window; `optional` accepts `{}`; `unavailable` rejects the claim with `pairing_unavailable`.

Each endpoint accepts only the method shown here or in the token endpoint list. A method mismatch returns `405` before bearer authentication, LAN lease validation, or control-mailbox dispatch, so it cannot execute a mutation through an unintended route.

Token endpoints require `Authorization: Bearer <token>`:

- `GET /api/v1/identity|network|status|events`
- `POST /api/v1/leases`, `PUT|DELETE /api/v1/leases`
- `PUT /api/v1/runtime|calibration|heater-curve|thermal-profile` and `POST /api/v1/calibration/job`

The device accepts two simultaneous HTTP connections. Authenticated identity and network reads are served from published snapshots, while status, SSE, and mutations share the bounded control workspace; each response carries `X-Flux-Purr-Revision: <u32>`. Control mutations are serialized by the main-loop mailbox and must carry both `X-Flux-Purr-Lease: <lease-id>` and the revision returned by a serial `GET /api/v1/status` immediately before that write. A missing revision returns `428 revision_required`; an outdated revision returns `409 stale_write` without executing the command. The mailbox retains the admission-checked lease ID and the main loop revalidates it immediately before execution; an expired queued write returns `409 lease_expired` without touching runtime state. A successful mutation increments the revision and returns the new value. Direct-LAN clients read a current snapshot immediately before a mutation; if `stale_write` still occurs, they may read once to reconcile displayed state but must not automatically replay a side-effecting request. Lease create, heartbeat, and release are lease coordination operations and do not require a control revision. A lease has a `30s` TTL and only one LAN writer can own it. After a device restart, a paired browser target may reacquire its expired lease only when the operator explicitly reselects that connection; heartbeat expiry remains read-only, and an explicit conflict is never automatically stolen. Token reset is intentionally absent from device HTTP.

The device reflects `https://flux-purr.ivanli.cc` and explicit localhost development origins in CORS responses. Chromium private-network preflight receives `Access-Control-Allow-Private-Network: true`; Safari and other browsers without Chromium PNA support must not offer direct-LAN control. `GET /api/v1/events` returns one authorized `text/event-stream` status frame per connection, and the browser fetch-stream client reconnects without putting the token in a URL.

### Browser direct-LAN flow

The production Web app exposes direct LAN pairing only in `demo=false`, from the Add device page. It accepts only an RFC1918 HTTP root address or a MAC-derived `flux-purr-<mac>.local` hostname plus the four-digit code currently visible on the physical WiFi Info page. A successful claim stores the bearer token only in the current Web origin's local device record, probes `identity`, `network`, and `status`, selects the resulting LAN target, and creates a 30-second lease before enabling writes.

On reload the Web app may probe only locally saved LAN records; it must not perform browser mDNS, CIDR, or background subnet discovery. The last operator-entered CIDR may be restored as a local form preference, but it must not start a scan by itself; the CIDR input, scan action, progress and discovered results stay visible throughout the direct-LAN workflow. A `401` invalidates only the affected bearer credential, retains its minimal device identity for route memory, and requires physical pairing before that route can control again. PNA/CORS rejection, unreachable address, expired or locked pairing code, and lease conflict/expiry are connection states rather than WiFi station failures. Direct LAN targets do not expose WiFi credential setup, firmware flashing, or token reset.

## Browser Web Serial

The Web app has two isolated browser variants selected by URL parameter:

- `?demo=true`: demo-only scenario; no `devd`, Web Serial, or real backend requests.
- `?demo=false`: live scenario; no demo fixtures, degraded demo data, or daemon mock devices.

The selected demo flag is stored in browser storage. A later load without `demo` uses the remembered value and rewrites the URL to include the explicit parameter. The app must not switch variants unless the URL explicitly asks for `demo=true` or `demo=false`.

Browser Web Serial uses the same USB CDC JSONL frames listed below. The Web app opens a port only from an explicit operator action with `navigator.serial.requestPort()`, then writes one newline-delimited JSON frame per request and waits for a matching `response.requestId`.

Direct browser targets are represented in the Web app as `transport=serial`, `baseUrl=webserial://selected`, and `leaseState=active`. That state means the browser owns the selected serial port; it is not a `devd` lease.

Supported direct operations:

- `request` with `op=get_identity|get_network|get_status|get_calibration|get_calibration_job|get_heater_curve`
- `runtime_config` for `targetTempC`, `selectedPresetSlot`, `presetsC`, `activeCoolingEnabled`, `heaterEnabled`, and `calibration`
- `calibration_config`, `calibration_apply`, `calibration_job`, `heater_curve_config`, and `heater_curve_save`

Unsupported direct operations:

- firmware artifact catalog and verification
- dry-run and real flash
- daemon-local bind/connect/disconnect

Those unsupported operations require Native `devd` HTTP capability gates.

## Native `devd` HTTP

Base URL: `http://127.0.0.1:<port>`. Default bind is `127.0.0.1:30080`; loopback binds enable development CORS for local `localhost` / loopback origins so the Vite console can call the daemon from its own local port.
Start the daemon with `flux-purr-devd serve`. Flags override environment variables; `--serial-port` and `FLUX_PURR_DEVD_SERIAL_PORT` override the user default USB port saved by `flux-purr usb-port set`. If no configured port is present, the project fallback is `/dev/cu.usbmodem21221401`.

Native serial discovery is constrained to the configured authorized port. If that path is absent, `devd` must not expose another native serial device.

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
- `GET /api/v1/devices/:id/lan-pairing/code?lease_id=...`
- `GET /api/v1/devices/:id/calibration?lease_id=...`
- `GET /api/v1/devices/:id/calibration/job?lease_id=...`
- `GET /api/v1/devices/:id/events`
- `PUT /api/v1/devices/:id/wifi`: WiFi provisioning is USB/devd-only. The live Web Settings form remains visible for a selected native `devd` device with `wifi_config`; without `wifi_state_v2` it locks every configuration control and reports that a protocol update is required. Submission requires `wifi_config`, `wifi_state_v2`, and an active USB lease. Its response is a redacted WiFi receipt with the device-published `NetworkSummary`; `devd` must reject an unversioned or malformed receipt. The browser retains the password through waiting and terminal failure. On device-confirmed `connected`, it clears only the password and displays the confirmed `NetworkSummary.ssid`; on `disabled`, it clears both fields. It never sends credentials over direct LAN or Web Serial.
- `PUT /api/v1/devices/:id/runtime`
- `PUT /api/v1/devices/:id/calibration`
- `POST /api/v1/devices/:id/calibration/apply`
- `POST /api/v1/devices/:id/calibration/job`
- `GET /api/v1/devices/:id/heater-curve?lease_id=...`
- `PUT /api/v1/devices/:id/heater-curve`
- `POST /api/v1/devices/:id/heater-curve/save`
- `GET /api/v1/artifacts`
- `POST /api/v1/artifacts/verify`
- `POST /api/v1/devices/:id/flash`

Mutating device endpoints require a valid lease. `bind`, `connect`, `disconnect`, and leased read endpoints pass it as `?lease_id=...`; JSON-body write endpoints use `leaseId`.

`GET /api/v1/devices/:id/lan-pairing/code?lease_id=...` is USB/devd-only and requires an active native-serial lease. It returns `{ "active": true, "code": "4827" }` only while the physical WiFi Info page remains open; otherwise it returns `{ "active": false }`. The daemon does not persist the code and redacts it from USB transport events. `flux-purr lan pairing-code --device <id>` presents this response for operator-assisted or agent-assisted Web pairing.

`POST /api/v1/devices/:id/bind?lease_id=...` body:

```json
{
  "alias": "Bench Alias"
}
```

`POST /api/v1/devices/:id/connect?lease_id=...` and `POST /api/v1/devices/:id/disconnect?lease_id=...` return the updated daemon-local `DeviceRecord`.

`GET /api/v1/devices` is the bounded polling snapshot for the Web device picker and live summary. It returns summary device status plus a small inline event slice. For native serial targets, those inline events keep only the fields needed for polling and transport-issue surfacing; full redacted transport frame payloads stay on the device-scoped event stream so the polling response does not balloon during calibration or monitor-heavy sessions.

`GET /api/v1/devices/:id/events` returns `text/event-stream`. The stream first replays that device's bounded event backlog, then continues with live events. Each SSE event name matches the `kind` field (`serial`, `lease`, `wifi`, `runtime`, `flash`, `transport`, etc.) and each `data` frame is a `DevdEvent` JSON object. Events are scoped to the requested device ID. Native USB JSONL exchanges emit paired `transport` events with direction, transport, request ID, frame type, and a redacted frame payload so the Web Runtime trace can show complete TX/RX data without leaking WiFi passwords.

`PUT /api/v1/devices/:id/runtime` body:

```json
{
  "leaseId": "lease-001",
  "targetTempC": 220,
  "selectedPresetSlot": 3,
  "presetsC": [50, 100, 120, 150, 180, 200, 210, 220, 250, 300],
  "activeCoolingEnabled": true,
  "heaterEnabled": true,
  "faultAttentionAcknowledged": true,
  "manualPpsEnabled": true,
  "manualPpsMv": 10400,
  "manualPpsMa": 2500,
  "calibration": {
    "mode": "vin_adc",
    "ppsEnabled": true,
    "ppsMv": 12000,
    "heaterEnabled": false
  },
  "thermalControlProfile": {
    "op": "preview",
    "profile": {
      "points": [
        {"targetTempC": 50, "brakeDistanceCentiC": 450, "approachPowerPermille": 380, "holdPowerPermille": 180},
        {"targetTempC": 100, "brakeDistanceCentiC": 450, "approachPowerPermille": 380, "holdPowerPermille": 180},
        {"targetTempC": 120, "brakeDistanceCentiC": 700, "approachPowerPermille": 320, "holdPowerPermille": 220},
        null,
        null,
        null,
        null,
        null,
        null,
        null
      ]
    }
  }
}
```

All runtime fields are optional except `leaseId`; the response is the updated `Status`. Status temperature fields `boardTempCenti` and `currentTempC` preserve the firmware RTD measurement at `0.01°C` resolution; front-panel rounding to `0.1°C` does not reduce API precision. `manualPpsEnabled=false` clears the debug override. Enabling manual PPS requires `manualPpsMv` within the hardware `5V~28V` range, within the advertised PPS capability, and on a `100mV` step; `manualPpsMa` must be within the advertised APDO current capability and on a `50mA` step. `runtime_config.calibration` controls the owner-facing calibration modes and follows the same PPS legality rules. `thermalControlProfile` is legacy-record compatibility only and cannot arm production heating. Calibration control only accepts PPS voltage requests; current remains read-only and is surfaced as the PPS current capability / CC-loop proxy used by firmware and tooling. CH224Q applies the PPS voltage request through its voltage register; `manualPpsMa` is a requested contract value for validation and status, not a direct chip current-register write.

`PUT /api/v1/devices/:id/calibration` body:

```json
{
  "leaseId": "lease-001",
  "op": "capture",
  "channel": "rtd_adc",
  "referenceTempC": 25.0
}
```

`op` is `capture | delete | clear | import`. `capture` requires `channel` and either a physical reference (`referenceTempC` for `rtd_adc`, `referenceVinMv` for `vin_adc`) or explicit `expectedMv`; `observedMv` is optional and otherwise comes from the latest device ADC reading. `delete` requires `sampleIndex`. `clear` requires `channel`. `import` requires a complete `package` with `rtdAdc` and `vinAdc` arrays.

`POST /api/v1/devices/:id/calibration/apply` body:

```json
{
  "leaseId": "lease-001"
}
```

Apply copies draft calibration to active calibration and returns the updated `CalibrationState`. It is rejected with `calibration_apply_heater_active` when the heater is enabled or output is nonzero.

`GET /api/v1/devices/:id/calibration/job?lease_id=...` returns the current auto-job state:

```json
{
  "kind": "vin_adc_auto",
  "status": "running",
  "progressPercent": 38,
  "samplesCollected": 3,
  "nextRequestMv": 13000,
  "message": null
}
```

`POST /api/v1/devices/:id/calibration/job` body:

```json
{
  "leaseId": "lease-001",
  "op": "start",
  "kind": "heater_curve_auto"
}
```

`op` is `start | cancel`. `start` accepts `kind=vin_adc_auto|heater_curve_auto|thermal_plant_auto`. `vin_adc_auto` writes samples into `vin_adc draft`; `heater_curve_auto` writes raw electrical observations and a derived curve; `thermal_plant_auto` is the protected `20V / >=3A` `80C/220C` two-anchor job. `cancel` stops the running job and clears calibration-owned live PPS / heater state.

`PUT /api/v1/devices/:id/heater-curve` body:

```json
{
  "leaseId": "lease-001",
  "op": "preview",
  "package": {
    "points": [
      { "tempCentiC": 13977, "resistanceMilliohms": 6033 },
      { "tempCentiC": 18153, "resistanceMilliohms": 6522 },
      null,
      null,
      null,
      null,
      null,
      null
    ]
  }
}
```

`op` is `preview | clear_preview`. Preview updates runtime heater curve state without writing EEPROM.

`POST /api/v1/devices/:id/heater-curve/save` body:

```json
{
  "leaseId": "lease-001"
}
```

Save copies the preview curve to active curve and schedules persistent memory commit. If no preview exists, the request is rejected with `heater_curve_preview_required`.

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
          "path": "target/xtensa-esp32s3-none-elf/release/flux-purr",
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
        "path": "target/xtensa-esp32s3-none-elf/release/flux-purr",
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
      "path": "target/xtensa-esp32s3-none-elf/release/flux-purr",
      "sha256": "sha256:54362508abf2a6148b6aecba23032c7b67bf346bf288a7ae1aaccf24c68af113",
      "size": 741452
    }
  ]
}
```

Web Update dry-check uses the catalog plus verify endpoint. Browser CORS preflight for development must allow `Content-Type` so JSON `POST /api/v1/artifacts/verify` works from Vite.

## User CLI

The released command-line control surface is `flux-purr`. It talks to `flux-purr-devd`, creates a device lease, heartbeats it during long operations, releases it before exit, and supports `--json` for machine-readable output.

Core commands:

- `flux-purr devices`
- `flux-purr identity --device <id>` or `--hardware <saved-id>`
- `flux-purr status --device <id>` or `--hardware <saved-id>`
- `flux-purr runtime get|set --device <id> ...`
- `flux-purr pd pps set --volts <decimal> --device <id>` or `--hardware <saved-id>`
- `flux-purr pd pps clear --device <id>` or `--hardware <saved-id>`
- `flux-purr thermal profile preview|clear-preview|save|clear-saved --device <id>` or `--hardware <saved-id>`
- `flux-purr thermal model calibrate --device <id>` or `--hardware <saved-id>`
- `flux-purr thermal self-test --device <id> [--source-kind isolapurr] --source-id <bench-source-id> --source-url <lan-url> [--profile-mode auto|65w|100w] [--source-power-watts <watts>] [--source-mode auto-follow|manual-forced] [--runtime-rearm-attempts <n>]`
- `flux-purr thermal tune --device <id> --source-id <bench-source-id> --source-url <lan-url> --profile-mode 100w --source-power-watts 100` runs the Rust-owned 5A full-batch preliminary review workflow. The default same-rank tuning target set is `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`; the canonical execution order is the recursive split `60, 240, 140, 100, 80, 120, 180, 160, 220`. The bundle reports physical-order `tuningTargetsC`, actual-order `tuningExecutionOrderC`, and one owner-facing card/tab per physical target.
- `flux-purr thermal retune --run-dir <dir> [--apply-preview --device <id>|--hardware <saved-id>]`
- `flux-purr thermal report rerender-legacy --legacy-bundle-dir <dir> [--output-dir <dir>]`
- Batch profile comparison repeats `--candidate-profile-file <path>` for one `--targets-c` value; candidates share one source/lease session, use `max(40C, target-30C)` as the restart threshold, produce separate reports, and never write EEPROM.
- `flux-purr calibration get|capture|delete|clear|import|export|apply|collect --device <id>` or `--hardware <saved-id>`
- `flux-purr calibration-mode status|exit --device <id>` or `--hardware <saved-id>`
- `flux-purr calibration-mode voltage|temperature|heater-curve ...`
- `flux-purr wifi set|clear --device <id> ...`
- `flux-purr flash --device <id> [--artifact-id <id>] [--manifest-path <path>]`
- `flux-purr monitor --device <id>`
- `flux-purr hardware available|recent|list|save|forget|path`
- `flux-purr usb-port show|set <port>`

`hardware` stores USB targets. LAN records are stored separately in the same user configuration with their token excluded from CLI, daemon, trace, and error output. `flux-purr lan devices|refresh|scan|pair|status|runtime-set` operates a saved LAN target. `flux-purr lan request --id <id> --method get|post|put|delete --path <api-path> [--body|--body-file]` exposes the remaining authorized runtime, calibration, heater-curve, and thermal-profile API; every write creates and releases the device LAN lease around the request.

`usb-port set` writes user configuration in the OS config directory, or under `FLUX_PURR_HOME` when set. A running daemon reads the default port only during startup, so it must be restarted after the default USB port changes.

## Product Release Manifest

Flux Purr releases use one product tag: `vX.Y.Z` for stable releases and `vX.Y.Z-rc.<sha7>` for RC releases. Web, firmware, and host-tools assets attach to the same GitHub Release.

Every release includes `flux-purr-release-manifest-vX.Y.Z.json` with this shape:

```json
{
  "schemaVersion": 1,
  "product": "flux-purr",
  "version": "0.2.0",
  "tag": "v0.2.0",
  "sourceSha": "cccccccccccccccccccccccccccccccccccccccc",
  "components": [
    {
      "id": "firmware",
      "version": "0.2.0",
      "sourceSha": "cccccccccccccccccccccccccccccccccccccccc",
      "protocolVersions": ["flux-purr.usb.v1"],
      "assets": [
        {
          "name": "flux-purr-firmware-v0.2.0.tar.gz",
          "path": "flux-purr-firmware-v0.2.0.tar.gz",
          "size": 741452,
          "sha256": "..."
        }
      ],
      "contentSha256": "...",
      "changedSincePrevious": true,
      "updateReason": "content_changed"
    }
  ]
}
```

Update UX and CLI guidance must use `changedSincePrevious`, `contentSha256`, and `updateReason` to avoid asking users to upgrade unchanged components.

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

`op`: `get_identity | get_network | get_status | get_calibration | get_calibration_job | get_heater_curve | set_log_level`.

The boot-time USB recovery loop may answer identity, but defers network and runtime status with retryable `startup_busy` until the main loop owns the restored configuration and live WiFi snapshot. This prevents a USB-open reset from being reported as lost WiFi credentials.

### `wifi_config`

```json
{
  "type": "wifi_config",
  "requestId": "req-002",
  "op": "set",
  "ssid": "FluxPurr-Lab",
  "password": "<secret>",
  "staticIpv4": null,
  "telemetryIntervalMs": 500
}
```

WiFi automatic reconnect is a fixed firmware policy and is not a request parameter.
`staticIpv4` is optional: omit it to preserve the stored addressing mode, provide an object to set a static address, or send `null` to clear a previous static address and return to DHCP.

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
  "selectedPresetSlot": 3,
  "presetsC": [50, 100, 120, 150, 180, 200, 210, 220, 250, 300],
  "activeCoolingEnabled": true,
  "heaterEnabled": true,
  "manualPpsEnabled": true,
  "manualPpsMv": 10400,
  "manualPpsMa": 2500,
  "calibration": {
    "mode": "rtd_adc",
    "ppsEnabled": true,
    "ppsMv": 12000,
    "heaterEnabled": true,
    "targetAdcMv": 1120
  },
  "thermalControlProfile": {
    "op": "clear_preview"
  }
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
      "selectedPresetSlot": 3,
      "presetsC": [50, 100, 120, 150, 180, 200, 210, 220, 250, 300],
      "activeCoolingEnabled": true,
      "heaterEnabled": true,
      "faultAttentionPending": false,
      "manualPpsEnabled": true,
      "manualPpsMv": 10400,
      "manualPpsMa": 2500,
      "thermalControlProfilePreview": false,
      "calibration": {
        "mode": "rtd_adc",
        "ppsEnabled": true,
        "ppsMv": 12000,
        "heaterEnabled": true,
        "targetAdcMv": 1120,
        "stable": false,
        "stabilityErrorMv": 18,
        "error": null,
        "job": {
          "kind": null,
          "status": "idle",
          "progressPercent": 0,
          "samplesCollected": 0,
          "nextRequestMv": null,
          "message": null
        }
      }
    }
  }
}
```

`manualPpsEnabled=false` clears the debug override. `calibration` controls the owner-facing calibration workbench. Both paths must reject any PPS request outside the hardware `5V~28V` range, outside the advertised capability, or off the required `100mV / 50mA` steps. `thermalControlProfile` supports `preview` / `clear_preview` for RAM-only preview state and `save` / `clear_saved` for persistent active thermal profile state.

### `calibration_config`

```json
{
  "type": "calibration_config",
  "requestId": "req-004",
  "op": "capture",
  "channel": "vin_adc",
  "referenceVinMv": 20000
}
```

Supported operations are `capture`, `delete`, `clear`, and `import`. The response returns `CalibrationState`.

### `calibration_apply`

```json
{
  "type": "calibration_apply",
  "requestId": "req-005"
}
```

The response returns `CalibrationState`, or `calibration_apply_heater_active` when applying would change active calibration while heater output is active.

### `calibration_job`

```json
{
  "type": "calibration_job",
  "requestId": "req-005",
  "op": "start",
  "kind": "vin_adc_auto"
}
```

Supported operations are `start` and `cancel`. `start` accepts `vin_adc_auto`, `heater_curve_auto`, and `thermal_plant_auto`. The response returns `CalibrationJobState`.

### `heater_curve_config`

```json
{
  "type": "heater_curve_config",
  "requestId": "req-006",
  "op": "preview",
  "heaterCurve": {
    "points": [
      { "tempCentiC": 13977, "resistanceMilliohms": 6033 },
      null,
      null,
      null,
      null,
      null,
      null,
      null
    ]
  }
}
```

Supported operations are `preview` and `clear_preview`. The response returns `HeaterCurveState`.

### `heater_curve_save`

```json
{
  "type": "heater_curve_save",
  "requestId": "req-007"
}
```

The response returns `HeaterCurveState`. Saved `active` curve is restored from persistent device memory after reboot; preview is not restored.

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
