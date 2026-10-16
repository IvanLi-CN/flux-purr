# Device install status contract

USB JSONL adds `get_install_status`; devd may proxy it but LAN does not gain flashing capability.

```json
{
  "layoutId": "flux-purr.esp32s3fh4r2.factory",
  "layoutVersion": 1,
  "partitionTableSha256": "sha256:...",
  "persistenceSource": "eeprom",
  "recordState": "valid",
  "recordSequence": 42,
  "commissioningRequired": false,
  "setupReason": null,
  "sensorState": "ready",
  "heaterLocked": false,
  "lastPersistenceFault": null,
  "persistenceFaultAttentionPending": false
}
```

- `persistenceSource`: `eeprom | none`
- `recordState`: `valid | blank | corrupt | incompatible | unavailable`; only EEPROM contributes persistent configuration.
- `setupReason`: `blank_persistence | corrupt_persistence | eeprom_required | explicit_reset | sensor_unready | calibration_required | null`
- `lastPersistenceFault`: optional bounded commit context with `code`, `phase`, `attempt`, `sequence`, `slot`, and redacted `message`; it never contains EEPROM bytes or Wi-Fi credentials.
- `persistenceFaultAttentionPending`: `true` while the front-panel fault prompt still awaits its one-key acknowledgement.
- Old valid records without `commissioningRequired` decode as `false`.
- A blank writable EEPROM may be initialized from the approved hardware profile. Corrupt, incompatible, or unavailable EEPROM enters `EEPROM_REQUIRED` with `commissioningRequired=true` and `heaterLocked=true`; MCU Flash is never a persistence source.
- `complete_setup` clears the flag only when the sensor is ready and a heater resistance curve has at least two points. User RTD/VIN ADC calibration is optional: absent user samples remain absent and the default active-slot correction is used. Setup completion must not synthesize user samples, infer physical references, replace explicit user correction, or require a two-point user fit. Thermal-model and ordinary-heating protection gates remain independent.
- Repository CLI `setup status --device <device-id>` reads this status; `setup complete --device <device-id>` calls `POST /api/v1/devices/{deviceId}/setup/complete` through an active native USB lease. The mutation is sent once. The host reports success only after install-status readback confirms a healthy EEPROM record, no heater lock, and a newer generation when commissioning was required; the verification deadline is six seconds.
- High-level `reset_persistence` verifies EEPROM only, then returns setup-required. Raw bytes never enter logs or diagnostics.
