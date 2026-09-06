# Frontpanel input interaction history

## Legacy identity

- Former legacy ID: `fk3u7`.

## Lifecycle

- `active`: input and navigation behavior remains the canonical frontpanel interaction contract.

- A pending EEPROM fault prompt consumes the first physical key as acknowledgement only; the read-only Dashboard remains visible while `PersistenceRequired` keeps heater and persistence-dependent actions locked.

## Partial replacement

- Heater, fan, and dashboard runtime truth is locally superseded by `heater-pid-frontpanel-runtime`; the input and navigation contract remains here.
