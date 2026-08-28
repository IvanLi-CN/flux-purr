# Thermal plant run snapshot contract

## Status

Accepted

## Context

The automatic thermal-model calibration already owns the complete transient run: it records the heater curve while heating to `220C`, disarms in that control cycle, records passive cooling to `80C`, fits locally, and atomically commits valid results. The generic calibration job state is intentionally small and must remain compatible with existing USB, LAN, devd, and Web clients. A page that only reads that generic state cannot show useful evidence of the run or distinguish a provisional fit from a persisted model.

## Decision

Expose a separate read-only `ThermalPlantRunSnapshot` projection through the `thermal_plant_run` capability.

- `runId` identifies an automatic attempt and never replaces the persisted `transactionId`.
- `attempt` carries live or terminal state, phase, progress, elapsed time, current temperature, heater voltage, duty, sample count, restart permission, and an optional error.
- `tracePage` is cursor based. `afterSample` is the first `sampleIndex` for the requested page (the initial cursor is `0`); `nextSample` is the next page cursor. Each page contains at most 16 points, and each point exposes only elapsed time, projected temperature, measured heater voltage, duty, and `ambient|heating|cooling` phase.
- `provisionalCurve` is display-only while a run is in progress. `activeResult` is populated only from the successfully persisted transaction, so it cannot be confused with a preview or candidate.
- The wire projection is bounded below 8 KiB and is identical across USB JSONL, direct LAN, native devd serial, and native devd LAN adapters.
- Clients merge pages by `runId` and `sampleIndex`, drain terminal pages, then stop polling once `restartAllowed` is true. Devices without the capability are rendered as explicitly incompatible and are not repeatedly queried.

## Consequences

The Web console can show meaningful progress, both heating and natural-cooling evidence, a provisional R(T) curve, and the final model without exposing raw ADC storage or coupling the UI to the persistence format. The existing `CalibrationJobState` endpoint and command remain unchanged for compatibility. The additional projection requires bounded pagination and capability negotiation in each transport adapter, plus deterministic mock data for UI and adapter tests.

## Alternatives considered

- Extending `CalibrationJobState` would make a generic compatibility contract carry transient trace data and would force older readers to understand fields they cannot use.
- Returning the full transient transaction would expose raw ADC and persistence details, exceed the bounded response budget, and make the UI dependent on firmware storage layout.
- SSE/WebSocket push would add a new transport and lifecycle surface; bounded 500ms reads are sufficient for the desktop calibration workflow and preserve existing request/lease semantics.
