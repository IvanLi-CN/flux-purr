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

- `persistenceSource`: `eeprom | none`
- `recordState`: `valid | blank | corrupt | incompatible | unavailable`; only EEPROM contributes persistent configuration.
- `setupReason`: `blank_persistence | corrupt_persistence | eeprom_required | explicit_reset | sensor_unready | calibration_required | null`
- Old valid records without `commissioningRequired` decode as `false`.
- A blank writable EEPROM may be initialized from the approved hardware profile. Corrupt, incompatible, or unavailable EEPROM enters `EEPROM_REQUIRED` with `commissioningRequired=true` and `heaterLocked=true`; MCU Flash is never a persistence source.
- `complete_setup` clears the flag only after existing sensor and calibration gates pass.
- High-level `reset_persistence` verifies EEPROM only, then returns setup-required. Raw bytes never enter logs or diagnostics.
