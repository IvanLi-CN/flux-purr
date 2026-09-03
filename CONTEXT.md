# Flux Purr

Flux Purr is a temperature-controlled heating device. This glossary defines the language shared by the firmware, Front Panel, Control Console, host tools, specifications, and hardware documents.

## Product And Operation

**Device**:
A physical Flux Purr unit that measures temperature and controls heating and cooling.
_Avoid_: Board, MCU, target, hardware when the complete product is meant

**Operator**:
The person who observes or controls a Device.
_Avoid_: User, owner, customer when the operating role is meant

**Front Panel**:
The local display and keys on the Device.
_Avoid_: Dashboard, Web UI

**Control Console**:
The Web application that observes and controls a Device.
_Avoid_: Dashboard when the complete application is meant

**Dashboard**:
The primary operating view in the Front Panel or Control Console. State which surface is meant when the distinction matters.
_Avoid_: Control Plane

**Control Plane**:
The shared Device operations and state available through supported transports.
_Avoid_: Dashboard, Web API

**Device Status**:
The Device-confirmed snapshot of its current operating state.
_Avoid_: UI state, requested state

**Target Temperature**:
The temperature that the Operator requests the Device to reach.
_Avoid_: Current Temperature, calibration temperature

**Current Temperature**:
The latest valid temperature derived from the RTD Measurement Path.
_Avoid_: Board Temperature, ambient temperature, raw ADC value

**Heater Request**:
Permission for the Device to apply heat. A Heater Request does not prove that the Heater Output is active.
_Avoid_: Heater Output

**Heater Output**:
The heat-control output that the Device reports as physically applied.
_Avoid_: Heater Request

**Active Cooling**:
The Device policy that permits fan operation when cooling is required.
_Avoid_: Fan Output

**Fan Output**:
The Device-reported physical fan state and intensity.
_Avoid_: Active Cooling

## Measurement

**RTD**:
The PT1000 resistance temperature detector used to measure the heated surface.
_Avoid_: NTC, ambient sensor, board sensor

**RTD Measurement Path**:
The complete path from the RTD and its analog network through ADC conversion and temperature projection.
_Avoid_: RTD when the complete measurement system is meant

**RTD Divider Excitation**:
The nominal supply used by the RTD divider's physical model, derived from the populated regulator feedback network. It is neither an independently measured rail nor an ADC Calibration Reference.
_Avoid_: ADC reference, calibrated 3V3, measured 3V3

**VIN Measurement Path**:
The complete path from the Device input-voltage divider through ADC conversion and voltage projection.
_Avoid_: VBUS telemetry, PD Contract

**ADC Raw Code**:
The 12-bit conversion result produced after the MCU ADC applies its basic zero-bias setup and firmware removes upper SAR status bits, before curve conversion to millivolts or physical units.
_Avoid_: Raw ADC millivolts, voltage, temperature

**Curve-Calibrated ADC Millivolts**:
Millivolts calculated from an ADC Raw Code with the MCU's eFuse calibration data.
_Avoid_: ADC Raw Code, calibrated temperature

**Temperature Reading**:
The temperature derived from the RTD Measurement Path and its active calibration.
_Avoid_: ADC Raw Code, Curve-Calibrated ADC Millivolts, ambient temperature

**VIN Reading**:
The Device input voltage derived from the VIN Measurement Path and its active calibration.
_Avoid_: PD Request, PD Contract, IsolaPurr VBUS

**Board Temperature**:
The temperature reported for the Device electronics, separate from the RTD Temperature Reading.
_Avoid_: Current Temperature, ambient temperature

**Measurement Drift**:
A sustained change in a measurement while the quantity under investigation is expected to remain stable. The term does not identify the cause.
_Avoid_: ADC drift unless ADC transfer change is proven

**Common-Mode Movement**:
A same-direction change observed in the RTD and VIN Measurement Paths. It identifies a shared boundary but does not identify a shared cause.
_Avoid_: ADC fault, supply fault unless independently proven

**Boot Trace**:
The retained sequence of RTD and VIN ADC Raw Codes from the first Device runtime seconds.
_Avoid_: Calibration curve, correction curve, reference

## Calibration

**Calibration Reference**:
An independently qualified physical value used to determine measurement error.
_Avoid_: Ambient estimate, VIN Reading, first reading, uptime curve

**ADC Calibration**:
The mapping from ADC observations to expected electrical values.
_Avoid_: Temperature calibration, heater calibration

**Temperature Calibration**:
The mapping from RTD observations to a qualified reference temperature.
_Avoid_: Board Temperature correction, ambient offset

**Calibration Sample**:
A saved pair of an observed value and its qualified expected value.
_Avoid_: Boot Trace sample, diagnostic sample

**Calibration Slot**:
One persistent set of calibration parameters that can be selected as active.
_Avoid_: Calibration Sample, draft calibration

**Active Calibration Slot**:
The Calibration Slot currently used by the Device measurement path.
_Avoid_: fitted suggestion, preview

**Fitted Calibration**:
Calibration parameters calculated from Calibration Samples and presented as a suggestion.
_Avoid_: Active Calibration Slot

**Runtime Fallback**:
A diagnostic state in which required eFuse calibration data is unavailable and temperature-accuracy validation must stop.
_Avoid_: Default calibration, valid calibration

**Automatic Thermal-Model Result**:
The last Device-confirmed, persisted result of a successful automatic thermal-model calibration. It is available for later review and identifies the active heater-resistance curve together with its thermal-model summary.
_Avoid_: Temporary progress, preview, candidate

**Calibration Outcome**:
The terminal state of the most recent automatic calibration attempt. A failed or canceled Calibration Outcome does not erase the Automatic Thermal-Model Result from an earlier successful attempt.
_Avoid_: Active thermal-model result

**Active Thermal Model**:
An Automatic Thermal-Model Result whose heater-resistance curve and thermal-model summary are confirmed by the Device as mutually valid and currently applied.
_Avoid_: Cached result, historical result, preview

**Advanced Manual Curve Tool**:
The optional operator workflow for importing, previewing, or saving a heater-resistance curve. It is separate from an Automatic Thermal-Model Result.
_Avoid_: Automatic thermal-model calibration

**Thermal Tuning**:
A supervised, multi-target control optimization workflow that evaluates point-local thermal-control parameters, confirms each accepted point, and produces a reviewable tuning result. It is distinct from Automatic Thermal-Model Calibration and the Advanced Manual Curve Tool.
_Avoid_: Heater Curve Calibration, generic PID adjustment

**Thermal Tuning Core**:
The deterministic, non-interactive state machine that schedules a Thermal Tuning Run, generates and evaluates candidates, and produces the Device-authoritative result. It uses canonical fixed-point values for decisions and candidate hashes, executes in firmware after a run starts, and may be replayed and verified by native and WebAssembly consumers without allowing them to advance a live run.
_Avoid_: Web tuning logic, CLI tuning logic, source coordinator

**Thermal Tuning Run**:
One Device-owned execution of the Thermal Tuning Core. Once started, it continues across Control Console, CLI, and `devd` disconnects; only an Operator cancellation or a Device safety condition can terminate it. A Device reset, power loss, or failed startup terminates the run as `interrupted_reset`; it never resumes automatically and cannot retain a promotable candidate.
_Avoid_: Calibration Job, report generation, host session

**Thermal Tuning Capability**:
The `thermal_tuning_run_v1` Device capability that declares support for the Device-owned Thermal Tuning Run protocol. Control Console tuning operations are unavailable without it and do not fall back to legacy host-driven behavior.
_Avoid_: firmware version inference, host-reference fallback, generic calibration capability

**Tuning Journal**:
The Device's compact two-phase persisted run record. It writes a start marker and one terminal or recovery summary for only the latest Thermal Tuning Run, allowing a reboot to report `interrupted_reset` without persisting raw samples or a promotable candidate.
_Avoid_: telemetry archive, candidate persistence, run history

**Tuning Eligibility**:
The Device-confirmed prerequisites for starting a Thermal Tuning Run: an active valid thermal model, valid heater-curve coverage, an available selected Thermal Tuning Power Class, and no current Maintenance Run owner.
_Avoid_: host preflight, estimated compatibility, automatic calibration

**Thermal Tuning Candidate**:
The Device-confirmed profile produced by a review-complete Thermal Tuning Run, identified by its candidate ID, power class, and content hash. Preview applies it only in RAM with a Device readback; a second simple confirmation may save only that unchanged preview to its matching persistent bank.
_Avoid_: saved profile, active profile, automatic persistence

**Maintenance Run Arbiter**:
The Device component that grants exclusive ownership to one heating-affecting maintenance workflow at a time. It rejects a Thermal Tuning Run while manual heating, automatic calibration, or another maintenance run is active and never implicitly stops or resumes the conflicting operation.
_Avoid_: UI disabled state, lease, automatic preemption

**Thermal Tuning Reference Engine**:
The independent host-driven CLI implementation retained to replay, compare, and improve the Thermal Tuning Core before firmware changes. It is not a normal Control Console workflow and may be removed only with explicit Operator approval.
_Avoid_: Production tuning engine, Web fallback, disposable compatibility path

**Tuning Reference Comparison**:
The optional CLI-produced comparison between a Thermal Tuning Report Bundle and the Thermal Tuning Reference Engine. Its `equivalent`, `divergent`, `inconclusive`, or `not_run` result informs algorithm improvement and HIL/release validation but never blocks runtime preview or save of a Device-authoritative candidate.
_Avoid_: Device safety disposition, runtime promotion gate, Web dependency

**Tuning Evidence**:
The Device's compact persisted decision journal together with the complete, monotonically sequenced Device telemetry archive recorded by a local Tuning Recorder. It contains only Device-local temperature, VIN, PPS-contract, and control-output evidence; it does not include external VBUS-current telemetry. A detected `trace_gap` or missing archive makes a run `review-incomplete` and prevents preview or save of its candidate profile.
_Avoid_: Device raw-trace storage, external source telemetry, terminal summary, saved profile

**Thermal Tuning Report Bundle**:
The versioned, cross-surface audit export for a Thermal Tuning Run. Version `thermal-tuning-v2` contains `index.html`, `run.bundle.json`, `samples.ndjson`, `thermal-profile.candidate.json`, and `decision-ledger.ndjson`; old `thermal-profile.accepted.json` is import-only compatibility data.
_Avoid_: EEPROM image, active profile, Web-only export format

**Tuning Host Runner**:
The `flux-purr` CLI process used in a CLI-initiated workflow. It records detailed telemetry, builds reports, and runs reference comparisons. It observes a Device-owned Thermal Tuning Run but does not make live tuning decisions.
_Avoid_: devd session, Control Console service, production tuning engine

**Tuning Recorder**:
A local persistence component owned by one host surface. The CLI writes native run artifacts and the Control Console automatically writes browser-local persistent artifacts, with no file-selection, upload, or credential step. The two surfaces do not communicate or relay tuning data to one another.
_Avoid_: Device raw-trace storage, devd report service, shared host session

**Thermal Tuning Power Class**:
The explicitly selected PPS source capability class used by a Thermal Tuning Run. `pps3a` is the 3A-class PPS tier and includes the existing 65W / `20V @ 3250mA` capability; `pps5a` is the 5A-class tier. This workflow supports only these PPS classes and never automatically resolves or downgrades between them.
_Avoid_: USB-C power rating, non-PPS tuning mode

## Product Release

**Product Release Version**:
The exact SemVer identity declared solely by the root `VERSION` file, including a prerelease identifier when applicable; a matching immutable Flux Purr product Git tag records a published instance but does not establish the version.
_Avoid_: Cargo package version, NPM package version, build ID, release label

**Version File**:
The root `VERSION` file that declares the Product Release Version. It remains unchanged during development and is updated only by a VERSION-only preparation commit on an already-open product PR.
_Avoid_: Git tag, Cargo package version, NPM package version, PR label

**Build Identity**:
The non-release display identity deterministically generated from the Version File without modifying it, optionally qualified by Git source revision data.
_Avoid_: Product Release Version, PR label, release channel

**Version Preparation Commit**:
The VERSION-only preparation commit appended to a verified product PR. Its normal protected merge carries the Product Release Version into `main`.
_Avoid_: Feature commit, product tag, direct main write

**Tag Reservation**:
The ownership gate for the `v< Product Release Version >` name. It must pass before a preparation commit is written; an existing tag is reusable only when an explicit recovery proves that it points to the same merged `main` commit.
_Avoid_: Tag-derived version, retagging, tag overwrite

**Migration Reconciliation Release**:
The first normal product release after an historical tag or release is preserved as audit history but cannot be associated with the current `main` chain. It establishes the next patch boundary without rewriting or reissuing the historical release.
_Avoid_: Retroactive release, history rewrite, release compression

**Release Repair PR**:
A PR that restores the release pipeline without changing the already-approved product source or creating a new Product Release Version.
_Avoid_: Product patch, feature release

**Release Recovery**:
An explicit product-release operation that republishes the Product Release Version recorded by an existing prepared main merge without changing that commit.
_Avoid_: Re-release, workflow retry

**Release Promotion**:
A stable product PR with its own protected merge boundary and exact stable `VERSION`; it must not retag the prerelease commit.
_Avoid_: Retagging, channel override, direct main write

## Power And Communication

**PD Request**:
The input voltage that the Device asks a USB-C power source to provide.
_Avoid_: PD Contract, VIN Reading

**PD Contract**:
The USB-C power agreement reported by the Device.
_Avoid_: PD Request, VIN Reading

**PD Controller Variant**:
The uniquely read-only-identified USB-C PD controller on a Device board: `CH224Q`, `FUSB302B`, or `unknown` when it is unsafe to select either driver.
_Avoid_: I2C address, PD Contract

**Contractual Current Limit**:
The maximum current granted by the active USB-C PD contract and used to bound heater power. It is not a measured VBUS load current.
_Avoid_: Current Reading, hardware over-current protection

**Performance-Guaranteed PD Contract**:
A ready PD contract of at least `20V` and `3A`. Contracts below this threshold may operate in degraded mode but are not valid for calibration or performance claims.
_Avoid_: PD Request, nominal source rating

**IsolaPurr**:
The external USB-C power source and link controller used during Device validation. It is not part of the Flux Purr Device.
_Avoid_: Device, MCU, calibration reference

**devd**:
The local daemon that mediates supported host access to a Device.
_Avoid_: Device firmware, Control Console

**Transport**:
A supported communication path that carries the shared Control Plane.
_Avoid_: Control Plane

**WiFi Provisioning Access**:
The Device-specific authority to read or change stored WiFi credentials. It is read-write only through an active USB Configuration Transport and read-only through a WiFi/LAN Transport.
_Avoid_: WiFi connection, WiFi permission

**USB Configuration Transport**:
An active Browser Web Serial connection or native `devd` USB bridge that can safely write Device WiFi credentials.
_Avoid_: WiFi/LAN Transport, generic devd target

**WiFi/LAN Transport**:
A direct LAN connection or native `devd` bridge over the Device network. It exposes current network facts but is not a WiFi credential write path.
_Avoid_: WiFi Provisioning Access

**Lease**:
Temporary exclusive authority to perform mutating Control Plane operations on one Device.
_Avoid_: Connection, pairing, device ownership

**Pairing**:
The process that authorizes a Control Console to use the Device LAN Control Plane.
_Avoid_: Lease, Wi-Fi provisioning
