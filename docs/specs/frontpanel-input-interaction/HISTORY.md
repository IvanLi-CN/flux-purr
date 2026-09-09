# Frontpanel input interaction history

## Legacy identity

- Former legacy ID: `fk3u7`.

## Lifecycle

- `active`: input and navigation behavior remains the canonical frontpanel interaction contract.

- A pending EEPROM fault prompt clears its attention overlay without swallowing menu navigation. `PersistenceRequired` remains an independent heater/PPS/calibration gate, and the EEPROM error page owns the explicit long-press retry action.

## Partial replacement

- Heater, fan, and dashboard runtime truth is locally superseded by `heater-pid-frontpanel-runtime`; the input and navigation contract remains here.
