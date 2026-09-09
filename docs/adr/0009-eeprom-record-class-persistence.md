# EEPROM Record-Class Persistence

## Status

Accepted

## Decision

The external M24C64 stores five FPR2 record classes instead of a whole
`MemoryConfig` snapshot. `SafetyCalibration`, `ThermalPolicy`, and the
`LayoutMarker` use explicit A/B slots. `UserPreferences` and
`NetworkAndPairing` are single-slot CRC records because a torn value in either
domain does not change the heater safety admission decision.

The fixed layout is:

| Domain | Slots | Bytes |
| --- | --- | --- |
| SafetyCalibration | A/B | `0x0000..0x03ff`, 512 B each |
| ThermalPolicy | A/B | `0x0400..0x09ff`, 768 B each |
| UserPreferences | single | `0x0a00..0x0a7f` |
| NetworkAndPairing | single | `0x0a80..0x0b7f` |
| LayoutMarker | A/B | `0x0c00..0x0cff`, 128 B each |
| Reserved | none | `0x0d00..0x0fff` |
| Legacy FPM1 | read-only during migration | `0x1000..0x1fff` |

Every FPR2 record has a 20-byte header, bounded payload, sequence, domain
identifier, and CRC32. Reads and writes use fixed internal RAM and EEPROM
chunks no larger than 16 bytes; each chunk releases the EEPROM adapter and
services the shared PD bus. No PSRAM, heap workspace, MCU Flash, NVS, raw
sector, or `flux_cfg` fallback is involved.

## Migration And Recovery

When a valid v1-v5 FPM1 record exists, the firmware stream-decodes it and
writes all four configuration domains. It then writes a `PREPARED` marker,
invalidates both legacy FPM1 magic values, and writes two `ACTIVE` markers.
Power loss before invalidation leaves the legacy source intact. A reboot that
finds `PREPARED` revalidates the new domains and either completes activation or
keeps heating locked for an explicit retry. Once `ACTIVE` exists, FPM1 is never
read again; older firmware therefore rejects the invalidated data and must not
heat.

## Failure Policy

Preferences and network write failures retain the live heater policy, mark the
domain unsaved, and expose a `PersistenceFault` with `slot: single`. The
EEPROM error page is the retry surface: a center long-press retries the failed
domain or resumes migration, while other keys only clear the attention overlay
and continue navigation. Safety calibration and thermal policy candidates
never become active before readback verification. Their failure leaves the
previous verified slot active and locks heater/PPS/calibration until retry or
discard.

## Consequences

- A single corrupt ordinary-domain record falls back only to that domain's
  defaults on reboot.
- Deprecated thermal-plant snapshot records are decode-only legacy data and
  are not migrated or written.
- Front-panel fault acknowledgement clears the attention overlay without
  swallowing navigation. Heater, PPS, and calibration gates remain separate
  from menu and fan navigation.
- Existing `PersistenceFault` JSON fields and devd serial events remain
  compatible; `code` identifies the domain and `slot` is `A`, `B`, or `single`.

## Related ADRs

- [`0008-eeprom-only-configuration-persistence.md`](0008-eeprom-only-configuration-persistence.md)
