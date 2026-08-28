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

## Product Release

**Release Repair PR**:
A PR that restores the release pipeline without changing the already-approved product source or creating a new product release intent.
_Avoid_: Product patch, feature release

**Release Recovery**:
An explicit product-release workflow dispatch that publishes the existing frozen release snapshot for a specified `main` commit when its original release run did not complete publication. It preserves the snapshot's source, channel, version, and tag.
_Avoid_: Re-release, workflow retry

**Release Promotion**:
An explicit product-release workflow dispatch that publishes a stable release from an already qualified pre-release candidate at the same source commit and effective version. It records a separate immutable promotion intent instead of changing the candidate's frozen release snapshot.
_Avoid_: Retagging, channel override

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

**Lease**:
Temporary exclusive authority to perform mutating Control Plane operations on one Device.
_Avoid_: Connection, pairing, device ownership

**Pairing**:
The process that authorizes a Control Console to use the Device LAN Control Plane.
_Avoid_: Lease, Wi-Fi provisioning
