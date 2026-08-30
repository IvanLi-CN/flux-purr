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
  "adcDiagnostics": {
    "calibrationSource": "efuse",
    "efuseVersion": 1,
    "attenuationDb": 6,
    "initCode": 1850,
    "referenceCode": 1600,
    "referenceMv": 850,
    "rtdRawCodeMean": 2100,
    "rtdRawCodeMin": 2098,
    "rtdRawCodeMax": 2102,
    "rtdRawCodeSpread": 4,
    "vinRawCodeMean": 1800
  },
  "pdRequestMv": 20000,
  "pdContractMv": 20000,
  "pdState": "ready",
  "pdController": "fusb302b",
  "pdContractKind": "pps",
  "pdContractCurrentMa": 3000,
  "pdContractPowerMw": 60000,
  "pdPerformanceGuaranteed": true,
  "pdDegradedReason": null,
  "manualPpsEnabled": false,
  "manualPpsMv": null,
  "manualPpsMa": null,
  "ppsCapabilityMinMv": 5000,
  "ppsCapabilityMaxMv": 21000,
  "ppsCapabilityMaxMa": 5000,
  "manualPpsError": null,
  "thermalControlProfilePreview": false,
  "frontpanelKey": null,
  "network": { "state": "connected", "wifiRssi": -54 }
}
```

`pdState`: `negotiating | ready | fallback_5v | fault`.
`fanDisplayState`: `OFF | AUTO | RUN`.
`presetsC` has exactly 10 entries; a numeric entry is an enabled preset temperature in Celsius, and `null` means the slot is disabled (`---` on the front panel).
`voltageMv` is the calibrated measured VIN input voltage. `pdContractMv` is the accepted PD contract voltage. `currentMa` retains its legacy controller telemetry/capability meaning and is not a verified live VBUS-load measurement. New `pdContractCurrentMa` and `pdContractPowerMw` are contractual upper-bound fields; neither is a physical current measurement or hardware over-current guarantee. `pdController` is `ch224q | fusb302b | unknown`; `pdContractKind` is `fixed | pps | none`. `pdPerformanceGuaranteed=true` requires a ready PPS contract of at least `20V` and `3A`. A lower-voltage PPS or fixed contract can operate in degraded mode, identified by `pdDegradedReason`, but cannot be used for performance or calibration claims. FUSB302BMPX selects a `5V..21V` PPS APDO when available and exposes its capability fields; a failed identity, reset, detach, I2C fault, or absent `PS_RDY` leaves heating interlocked.
`rtdRawAdcMv` and `vinRawAdcMv` retain their existing contract names but expose eFuse curve-calibrated millivolt readings before the project-level A/B calibration fit. They are not hardware ADC codes. `adcDiagnostics` is a read-only, optional diagnostic object so hosts remain compatible with older firmware. Its RTD mean/min/max/spread and VIN mean are 12-bit codes obtained through `AdcCalBasic` from the same conversions used for curve-calibrated mV; firmware always masks off upper SAR status bits before diagnostics and curve conversion. `calibrationSource=runtime_fallback` means required eFuse calibration data is missing; temperature-accuracy validation must stop, and firmware does not substitute the assumed 1100 mV gain reference. VBUS, VIN, ambient temperature, uptime, and an initial reading are never calibration references for this object.
`faultAttentionPending=true` only means a `temp >= 420°C` thermal-runaway event has fallen below `420°C` and still awaits acknowledgement. RTD open/short and ADC read failure do not set this field. Owner-facing temperature remains the last valid RTD value while a measurement fault is active; unavailable transport state must not synthesize `0°C`.
`manualPps*` remains the debug-only PPS override surface. Owner-facing calibration mode control uses `status.calibration` / `runtime_config.calibration` as its semantic source of truth. `thermalControlProfilePreview=true` means the firmware is using a RAM-only thermal profile preview; `clear_preview` returns to the persistent saved profile or factory default curve. `thermalControl` is the resolved controller input for the current target, not an echo of the last request: it reports whether a profile is active and covers the target, its source (`default` / `preview` / `saved`), and the effective power, damping, PI, lead, filter, warmup-reentry, adjustable-voltage-floor, and `heaterCurrentReserveMa` parameters after interpolation, legacy-profile inflate when importing old data, and safety clamps are applied. On a PPS/AVS backend, the selected APDO's voltage and current contract bounds production power; `R(T)` is used for heater-watt estimation but neither it nor `heaterCurrentReserveMa` lowers the adjustable-voltage request ceiling. The current-reserve field remains relevant only to the fixed-PD PWM fallback.

### `CalibrationState`

```json
{
  "rtdAdc": {
    "samples": [{ "observedMv": 1120, "expectedMv": 1118, "referenceTempC": 25.0, "targetAdcMv": 1118 }, null, null, null, null, null, null, null],
    "fittedFit": { "gain": 1.0, "offsetMv": -2.0, "sampleCount": 1 },
    "slots": { "a": { "gain": 1.0, "offsetMv": 0.0 }, "b": { "gain": 0.9982, "offsetMv": 5.4 } },
    "activeSlot": "a"
  },
  "vinAdc": {
    "samples": [{ "observedMv": 1670, "expectedMv": 1820, "referenceVinMv": 20000 }, null, null, null, null, null, null, null],
    "fittedFit": { "gain": 1.0, "offsetMv": 150.0, "sampleCount": 1 },
    "slots": { "a": { "gain": 1.0, "offsetMv": 0.0 }, "b": { "gain": 1.0, "offsetMv": 150.0 } },
    "activeSlot": "b"
  }
}
```

Calibration channels are `rtd_adc` and `vin_adc`. Each channel stores up to eight ADC-domain samples, a derived `fittedFit`, persistent A/B slots, and its `activeSlot`. Capture preserves the owner-entered physical reference (`referenceTempC` for RTD, `referenceVinMv` for VIN). Import replaces the complete state; sample, slot, and active-slot operations persist immediately. There is no draft, apply, or promotion stage.

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

`thermal_plant` is an internal runtime state used only while the protected
`thermal_plant_auto` job runs from the heater-curve workspace. It is not a
fourth owner-facing calibration mode and cannot be selected through the
manual calibration control.

Calibration live control requires an active adjustable PPS contract. FUSB302BMPX exposes calibration only while its selected `5V..21V` PPS contract meets the `>=20V @ >=3A` performance tier; its fixed-PDO fallback remains heat-only. This path remains subject to the authorized HIL interoperability gate.

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

`state` is `missing`, `active`, or `invalid`. The persistent source of truth is a bounded raw transient trace: ambient RTD ADC, `50ms` timestamps, RTD ADC, measured heater voltage, and duty. `thermal_plant_auto` samples the heater-resistance curve during the same run, requests the selected APDO's maximum voltage with `100%` PWM until `220C`, then immediately disarms and records natural cooling to `80C`. The APDO must cover `20V` at `>=3A`; a `5V..21V / 3A` APDO therefore runs at `21V`, while a `5V..20V / 3A` APDO runs at `20V`. Heater-curve data and production-profile current reserve settings do not reduce the calibration request. The device fits the coefficients locally, writes a physically valid model directly to `active`, and leaves heating disarmed. There is no candidate, promotion, cross-current comparison, or user acceptance operation. Production heating requires an active model, a PPS APDO covering `20V` at at least `3A`, and the curve captured by that same transient run.

The calibration state machine uses the current valid RTD measurement directly for transient sampling, the `220C` cutoff, and the passive-cooling endpoint. Guarded control temperature remains a production PID input only. There is no separate `225C` calibration failure threshold; once the live measurement first reaches `220C`, logical duty and physical PWM are cleared in that control cycle. The selected calibration-owned PPS contract remains unchanged through passive cooling so PD renegotiation cannot disturb the RTD trace, and is cleared only after the model transaction completes, is canceled, or fails. General sensor and absolute-overtemperature protections remain authoritative.

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

`devd` computes file size and `sha256` from local build outputs before returning catalog entries. Paths are repo-relative and must not expose unrelated host paths in errors. The local ESP32-S3 release artifact is an ELF and is flashed with `espflash flash`; an authorized native USB Serial/JTAG `cu.usbmodem*` port uses `--before usb-reset`. A connection failure waits one second and retries that USB reset once before a final `default-reset` fallback; no retry changes the authorized port. Other serial paths retain `default-reset`. `flashAddress` is only set for raw app binaries. For a raw app, devd writes the checked-in `firmware/partitions.bin` at `0x8000`, writes the app at its explicit address, then explicitly resets the target so the `flux_cfg` layout is installed and the application starts.

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

- `GET /health` is the anonymous, low-frequency daemon summary. It returns the product `version`, derived `channel`, Release/build `sourceSha`, and `buildId` alongside the daemon bind/device counts; it never returns runtime telemetry, a code, or a bearer token. Development daemons report `nextPatch(VERSION)-dev.<short-sha>` with channel `local`, while release daemons report the exact root `VERSION` and its stable/RC channel.
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
- `calibration_config`, `calibration_job`, `heater_curve_config`, and `heater_curve_save`

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
- `GET /api/v1/devices/:id/calibration/thermal-plant/run?lease_id=...&after_sample=<cursor>`
- `GET /api/v1/devices/:id/events`
- `PUT /api/v1/devices/:id/wifi`: WiFi provisioning is USB/devd-only. The live Web Settings form remains visible for a selected native `devd` device with `wifi_config`; without `wifi_state_v2` it locks every configuration control and reports that a protocol update is required. Submission requires `wifi_config`, `wifi_state_v2`, and an active USB lease. Its response is a redacted WiFi receipt with the device-published `NetworkSummary`; `devd` must reject an unversioned or malformed receipt. The browser retains the password through waiting and terminal failure. On device-confirmed `connected`, it clears only the password and displays the confirmed `NetworkSummary.ssid`; on `disabled`, it clears both fields. It never sends credentials over direct LAN or Web Serial.
- `PUT /api/v1/devices/:id/runtime`
- `PUT /api/v1/devices/:id/calibration`
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

All runtime fields are optional except `leaseId`; the response is the updated `Status`. Status temperature fields `boardTempCenti` and `currentTempC` preserve the firmware RTD measurement at `0.01°C` resolution; front-panel rounding to `0.1°C` does not reduce API precision. `manualPpsEnabled=false` clears the debug override. Enabling manual PPS requires `manualPpsMv` within the selected controller's advertised PPS capability, on a `100mV` step; `manualPpsMa` must be within its advertised APDO current capability and on a `50mA` step. FUSB302BMPX accepts manual PPS within its advertised `5V..21V` capability and rejects AVS requests. `runtime_config.calibration` controls the owner-facing calibration modes and requires FUSB302BMPX to hold a qualifying PPS contract. `thermalControlProfile` is legacy-record compatibility only and cannot arm production heating. Current remains read-only contract metadata. `manualPpsMa` does not operate a direct VBUS-current register.

`PUT /api/v1/devices/:id/calibration` body:

```json
{
  "leaseId": "lease-001",
  "op": "capture",
  "channel": "rtd_adc",
  "referenceTempC": 25.0
}
```

`op` is `capture | delete | clear | import | set_active_slot | set_slot_fit`. `capture` requires `channel` and either a physical reference (`referenceTempC` for `rtd_adc`, `referenceVinMv` for `vin_adc`) or explicit `expectedMv`; `observedMv` is optional and otherwise comes from the latest device ADC reading. `delete` requires `sampleIndex`. `clear` requires `channel`. `import` requires complete `state`. `set_active_slot` requires `channel` and `slot`; `set_slot_fit` also requires `fit`.

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
  "kind": "thermal_plant_auto"
}
```

`op` is `start | cancel`. `start` accepts `kind=vin_adc_auto|thermal_plant_auto`. `vin_adc_auto` writes shared `vin_adc` samples; `thermal_plant_auto` enters its internal runtime state directly and is the protected single-run transient job. It requires a PPS capability covering `20V` at `>=3A`, records the heater curve while heating to `220C`, turns the heater off in that same control cycle, and completes after passive cooling to `80C`. `cancel` stops the running job and clears calibration-owned live PPS / heater state.

`GET /api/v1/devices/:id/calibration/thermal-plant/run?lease_id=...&after_sample=<cursor>` returns the v1 `ThermalPlantRunSnapshot`. `after_sample` defaults to `0`; the response starts at that sample index, returns at most 16 trace points, and includes `nextSample` when more points remain. The response exposes projected temperature, measured heater voltage, duty, elapsed time, and phase only; raw ADC values are excluded. The payload is bounded below 8 KiB. `attempt` reports the live or terminal run and `activeResult` is populated only after the persisted transaction commits. Clients should stop polling after terminal `restartAllowed=true`. A missing `thermal_plant_run` identity capability is an explicit compatibility state, not a retryable endpoint error.

`POST /api/v1/devices/:id/eeprom` is the USB/devd-only advanced raw EEPROM maintenance endpoint. It requires an active lease and native USB serial transport; it is not exposed through the device LAN API or Web console. Physical heater output must already be `0%`.

```json
{
  "leaseId": "lease-001",
  "op": "write",
  "offset": 31,
  "bytes": [0, 255, 17, 34]
}
```

`op` is `read | write | erase`. `read` requires `offset` and `length`; `write` requires `offset` and `bytes`; each transport chunk is `1..32` bytes and must stay within the `8 KiB` EEPROM. `erase` accepts no range or content, writes the full EEPROM as `0xFF`, and the devd CLI verifies the full readback. The endpoint returns `bytes` only for `read`; it acknowledges `write` and `erase`. Before the first raw write or erase byte, firmware clears debug/calibration PPS, latches fixed-PD disarm, and discards pending ordinary record commits. A raw transport failure remains locked for restart because preceding bytes may have changed EEPROM. A successful raw write keeps heating and calibration locked until restart so the firmware can re-evaluate the complete image; erase clears the in-memory model and curve without writing a default record. Export/import clients compose raw `8 KiB` images from those chunks without device identity checks, parsing, filtering, migration, or changes to unknown bytes.

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
- `flux-purr calibration get|capture|delete|clear|set-slot-fit|set-active-slot|import|export|collect --device <id>` or `--hardware <saved-id>`
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

Flux Purr releases use one product tag containing the exact root `VERSION`: `vX.Y.Z` for stable releases and `vX.Y.Z-rc.N` for RC releases. Web, firmware, and host-tools assets attach to the same GitHub Release. The release Web archive contains a same-origin firmware catalog so the Browser never has to call GitHub. Development builds are displayed as `nextPatch(VERSION)-dev.<short-sha>` and are never release tags.

Every release includes `flux-purr-release-manifest-vX.Y.Z.json` with this shape:

```json
{
  "schemaVersion": 1,
  "product": "flux-purr",
  "version": "0.22.1",
  "tag": "v0.22.1",
  "sourceSha": "cccccccccccccccccccccccccccccccccccccccc",
  "channel": "stable",
  "components": [
    {
      "id": "firmware",
      "version": "0.22.1",
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

## Web Firmware Release Catalog

The Web app exposes firmware releases as static same-origin files, not as a browser GitHub integration:

- `GET /firmware/releases-manifest.json` returns the strict catalog defined by `docs/specs/web-firmware-install-recovery/contracts/firmware-release-catalog.schema.json`.
- Each entry's `assetPath` resolves to a precise `GET /firmware/releases/<safe-component>/<safe-component>.fluxpurr-fw` resource.
- Browser code validates the catalog and then validates the full selected bundle before a firmware transaction. The catalog bundle hash, version, channel, source SHA and build ID must all agree with the bundle.
- In Vite development, the same origin is served by a fixed `/firmware/**` proxy. The proxy, not Browser code, pages GitHub Releases and overlays current local builds over matching published build identities.

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

`manualPpsEnabled=false` clears the debug override. `calibration` controls the owner-facing calibration workbench. CH224Q rejects PPS requests outside its advertised capability or off the required `100mV / 50mA` steps. FUSB302BMPX accepts manual PPS within its advertised `5V..21V` APDO and requires a qualifying PPS contract for calibration. `thermalControlProfile` supports `preview` / `clear_preview` for RAM-only preview state and `save` / `clear_saved` for persistent active thermal profile state.

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

Supported operations are `capture`, `delete`, `clear`, `import`, `set_active_slot`, and `set_slot_fit`. The response returns `CalibrationState`.

### `calibration_job`

```json
{
  "type": "calibration_job",
  "requestId": "req-005",
  "op": "start",
  "kind": "vin_adc_auto"
}
```

Supported operations are `start` and `cancel`. `start` accepts `vin_adc_auto` and `thermal_plant_auto`. The response returns `CalibrationJobState`.

### `thermal_plant_run`

```json
{
  "type": "thermal_plant_run",
  "requestId": "req-008",
  "afterSample": 48
}
```

The response payload is the same `ThermalPlantRunSnapshot` returned by the native and direct-LAN endpoint. `runId` is stable for one attempt, `sampleIndex` is the merge key, and `phase=cooling` identifies passive natural-cooling samples. The generic `get_calibration_job` request remains available for older firmware and clients.

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
