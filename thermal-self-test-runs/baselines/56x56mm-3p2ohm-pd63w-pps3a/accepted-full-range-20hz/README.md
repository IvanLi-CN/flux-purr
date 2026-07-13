# Accepted full-range 20Hz thermal baseline

This bundle is the committed real-HIL reference package for the `56mm x 56mm`, `3.2Ω` heater plate on the PD PPS `3A` class source path (`5-21V`, nominal `60-65W` envelope).

## Hardware class

- Heater plate: `56mm x 56mm`
- Heater resistance: `3.2Ω`
- Source class: USB PD PPS
- PPS range: `5-21V`
- Current limit: `3A max`
- Calibration envelope: nominal `60-65W`
- Control cadence: `20Hz`
- RTD conversions per control cycle: `64`
- Host sample interval in this bundle: `100ms`

## Coverage

- Validation targets: `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`
- Accepted anchor profile: `60 / 100 / 140 / 180 / 220 / 250°C`
- Authorized source: `856a141cdbd4`
- Source endpoint: `http://192.168.31.122`
- Source mode: `auto_follow`

## Contents

- `index.html`: canonical full-range calibration report with working charts
- `run.bundle.json`: bundle manifest, target coverage, validation result, and source-run mapping
- `samples.ndjson`: aggregated raw full-range HIL samples used by the report
- `thermal-profile.accepted.json`: preview/save-compatible accepted profile
- `source-run-summaries/`: copied provenance summaries for the underlying accepted runs

## Report format

The final deliverable format for this bundle is HTML.

Local MHTML is intentionally not part of the committed baseline package because browser sandboxing can block script execution and leave the charts unusable. If a future archival format is needed, it should be generated as a separate static artifact rather than replacing the canonical HTML report.

## Power-path observations

This bundle stays inside the same PPS hardware class used for calibration:

- highest recorded PPS contract: `20.5V`
- highest recorded source voltage: about `20.45V`
- highest recorded source current: about `2.995A`
- highest recorded measured source power: about `58.1W`

The measured peak power does not need to saturate the advertised source envelope to remain part of the same calibration class. Treat this bundle as the accepted `60-65W` PPS reference for this exact heater and source combination.

## Reuse rule

Use this bundle as the default regression and retuning reference only for the same heater geometry, heater resistance, and PD PPS source class. If any of those boundaries change, freeze a separate baseline bundle instead of comparing against this one directly.
