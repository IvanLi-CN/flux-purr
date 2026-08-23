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
  "heaterLocked": false
}
```

- `persistenceSource`: `eeprom | flux_cfg | defaults`
- `recordState`: `valid | blank | corrupt | incompatible`; when neither backend yields a record, current startup distinguishes blank from an incompatible EEPROM record.
- `setupReason`: `blank_persistence | corrupt_persistence | explicit_reset | sensor_unready | calibration_required | null`
- Old valid records without `commissioningRequired` decode as `false`.
- Blank/corrupt persistence and explicit reset materialize safe defaults with `commissioningRequired=true` and `heaterLocked=true`.
- `complete_setup` clears the flag only after existing sensor and calibration gates pass.
- High-level `reset_persistence` verifies EEPROM and `flux_cfg`, then returns setup-required. Raw bytes never enter logs or diagnostics.
