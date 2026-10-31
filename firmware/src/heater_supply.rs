//! Whole-device PPS observations. Requests are setpoints, never meter readings.
//! This module deliberately has no resistance or auxiliary-power estimator.

use serde::{Deserialize, Serialize};

use core::sync::atomic::{AtomicU8, Ordering};

// Actual driver lifecycle, deliberately independent of association/IP state.
static WIFI_DRIVER_STATE: AtomicU8 = AtomicU8::new(0);

pub fn wifi_hardware_started() -> Option<bool> {
    match WIFI_DRIVER_STATE.load(Ordering::Acquire) {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

pub fn publish_wifi_hardware_state(started: Option<bool>) {
    WIFI_DRIVER_STATE.store(started.map_or(2, u8::from), Ordering::Release);
}

pub const MAX_BOUNDARY_POINTS: usize = 41;
pub(crate) const MIN_HEATED_CURRENT_MA: u16 = 2_000;
// Version 6 deliberately invalidates the prior boundary schema. Old
// calibration is absent rather than silently reused by automatic heating.
pub const BOUNDARY_FORMAT_VERSION: u8 = 6;
pub const BOUNDARY_POINT_BYTES: usize = 11;
pub const BOUNDARY_HEADER_BYTES: usize = 26;
pub const BOUNDARY_MAX_BYTES: usize =
    BOUNDARY_HEADER_BYTES + MAX_BOUNDARY_POINTS * BOUNDARY_POINT_BYTES;
/// Automatic heating may only use a persisted boundary after the 3 A scan has
/// produced powered observations through the minimum warm-up coverage.
pub(crate) const MIN_BOUNDARY_COVERAGE_CENTI_C: i16 = 10_000;
// The compact 10 C scan bins are stored using rounded ADC temperatures. A
// point near the lower edge of one bin can be followed by the next retained
// point near the upper edge of the following bin, leaving a real cold-point
// gap close to 18 C. Carry that colder proof through the bounded gap instead
// of falling back to the unconstrained model; never use a warmer point before
// the live temperature reaches its recorded observation.
const MAX_OBSERVED_LOOKUP_GAP_CENTI_C: i16 = 1_800;
// A confirmed tracking point may be held across a small RTD quantization dip.
// A larger real temperature drop still selects a colder boundary and forces a
// safe voltage reduction before output resumes.
const MAX_CONFIRMED_REQUEST_HANDOFF_GAP_CENTI_C: i16 = 300;
// The persisted terminal raw-ADC point can project several degrees warmer
// than the live point because the compact trace rounds RTD millivolts. Keep
// the pre-terminal handoff bounded while covering the measured worst-case gap
// without promoting an arbitrary warmer observation.
const MAX_TERMINAL_PRE_HANDOFF_GAP_CENTI_C: i16 = 1_200;
// Automatic PPS transitions move in 500 mV steps. Once the controller has
// issued the first step toward a power-safe terminal point, keep that point
// eligible while the one-step request is being confirmed.
const TERMINAL_HANDOFF_REQUEST_STEP_MV: u16 = 500;
const TERMINAL_TRACKING_CURRENT_CODE: u16 = 30;
const THREE_AMP_TRACKING_CURRENT_CODE: u16 = 31;

fn terminal_tracking_current_ma(source_power_limit_mw: u32, request_mv: u16) -> Option<u16> {
    if source_power_limit_mw == 0 || request_mv == 0 {
        return None;
    }
    let current_ma = (source_power_limit_mw * 1_000 / u32::from(request_mv)) as u16 / 50 * 50;
    (current_ma >= 1_000).then_some(current_ma)
}

#[path = "heater_supply_scan.rs"]
pub mod scan;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryAdcMetadata {
    pub divider_supply_mv: u16,
    pub reference_mv: u16,
    pub reference_code: u16,
    pub init_code: u16,
    pub efuse_version: u8,
    pub attenuation_db: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryPoint {
    pub elapsed_ms: u32,
    pub rtd_code: u16,
    pub rtd_mv: u16,
    pub vin_code: u16,
    pub vin_mv: u16,
    pub request_current_ma: u16,
    pub tracking_request_mv: u16,
    /// Zero means no flat response was observed before the probe ended.
    pub flat_request_mv: u16,
    /// A transient 3 A tracking observation, distinct from a thermal equilibrium.
    pub tracking_only: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SupplyBoundary {
    pub adc: BoundaryAdcMetadata,
    pub wifi_started: bool,
    pub cutoff_reached: bool,
    pub source_min_mv: u16,
    pub source_max_mv: u16,
    pub source_max_ma: u16,
    /// Maximum power of the high-voltage Fixed PDO used to establish the
    /// boundary. Zero means that the source advertised no usable Fixed PDO.
    pub source_power_limit_mw: u32,
    /// Original APDO envelope, distinct from the usable scan range above.
    pub advertised_min_mv: u16,
    pub advertised_max_mv: u16,
    pub point_count: u8,
    pub points: [BoundaryPoint; MAX_BOUNDARY_POINTS],
}

impl Default for SupplyBoundary {
    fn default() -> Self {
        Self {
            adc: BoundaryAdcMetadata::default(),
            wifi_started: false,
            cutoff_reached: false,
            source_min_mv: 0,
            source_max_mv: 0,
            source_max_ma: 0,
            source_power_limit_mw: 0,
            advertised_min_mv: 0,
            advertised_max_mv: 0,
            point_count: 0,
            points: [BoundaryPoint::default(); MAX_BOUNDARY_POINTS],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BoundaryQuery {
    pub temperature_centi_c: i16,
    pub current_rtd_mv: Option<u16>,
    pub current_request_mv: Option<u16>,
    pub current_ma: u16,
    pub wifi_started: bool,
    /// Advertised APDO bounds used to identify the calibrated source.
    pub source_min_mv: u16,
    pub source_max_mv: u16,
    /// Operational scan bounds after applying the source power envelope.
    pub operational_source_min_mv: u16,
    pub operational_source_max_mv: u16,
    pub source_max_ma: u16,
    pub source_power_limit_mw: u32,
}

impl SupplyBoundary {
    fn has_minimum_coverage(
        &self,
        project_temperature: &mut impl FnMut(u16) -> Option<i16>,
    ) -> bool {
        self.points[..usize::from(self.point_count)]
            .iter()
            .any(|point| {
                project_temperature(point.rtd_mv)
                    .is_some_and(|temperature| temperature >= MIN_BOUNDARY_COVERAGE_CENTI_C)
            })
    }

    pub fn is_valid(&self) -> bool {
        (1..=MAX_BOUNDARY_POINTS as u8).contains(&self.point_count)
            && (3_000..=3_600).contains(&self.adc.divider_supply_mv)
            && self.source_min_mv >= 5_500
            && self.source_max_mv <= 21_000
            && self.source_min_mv <= self.source_max_mv
            && self.advertised_min_mv > 0
            && self.advertised_min_mv <= self.source_min_mv
            && self.advertised_max_mv >= self.source_max_mv
            && self.source_max_ma == 3_000
            && (self.source_power_limit_mw == 0
                || ((16_500..=105_000).contains(&self.source_power_limit_mw)
                    && self.source_power_limit_mw.is_multiple_of(10)))
            && self.points[..usize::from(self.point_count)]
                .iter()
                .all(|point| {
                    let terminal_tracking = point.tracking_only
                        && point.tracking_request_mv > self.source_max_mv
                        && point.tracking_request_mv <= self.advertised_max_mv
                        && terminal_tracking_current_ma(
                            self.source_power_limit_mw,
                            point.tracking_request_mv,
                        ) == Some(point.request_current_ma);
                    let regular_current = (MIN_HEATED_CURRENT_MA..=3_000)
                        .contains(&point.request_current_ma)
                        && point.request_current_ma.is_multiple_of(100);
                    let point_voltage_valid = terminal_tracking
                        || (regular_current
                            && (self.source_min_mv..=self.source_max_mv)
                                .contains(&point.tracking_request_mv));
                    point_voltage_valid
                        && point.tracking_request_mv.is_multiple_of(100)
                        && (point.flat_request_mv == 0
                            || (point.flat_request_mv > point.tracking_request_mv
                                && point.flat_request_mv <= self.source_max_mv))
                        && point.rtd_code <= 4_095
                        && point.vin_code <= 4_095
                        && point.rtd_mv > 0
                        && point.vin_mv > 0
                        && point.rtd_mv <= 4_095
                        && point.vin_mv <= 4_095
                        && point.elapsed_ms / 50 < (1 << 19)
                        && point.elapsed_ms.is_multiple_of(50)
                        && (!point.tracking_only
                            || point.request_current_ma == 3_000
                            || terminal_tracking)
                })
    }

    /// Only observations dominated by the current operating conditions qualify.
    /// No current/temperature interpolation or inferred plate resistance is used.
    pub fn safe_point(
        &self,
        query: BoundaryQuery,
        mut project_temperature: impl FnMut(u16) -> Option<i16>,
    ) -> Option<BoundaryPoint> {
        if !self.is_valid()
            || self.wifi_started != query.wifi_started
            // The raw advertised APDO is the calibration identity. The
            // operational scan range is carried separately, so accepting a
            // containing envelope would let a different APDO reuse the
            // persisted whole-device boundary.
            || self.advertised_min_mv != query.source_min_mv
            || self.advertised_max_mv != query.source_max_mv
            || self.source_min_mv != query.operational_source_min_mv
            || self.source_max_mv != query.operational_source_max_mv
            || self.source_max_ma != query.source_max_ma
            || self.source_power_limit_mw != query.source_power_limit_mw
            || !self.has_minimum_coverage(&mut project_temperature)
        {
            return None;
        }
        // A 3 A tracking point is an observed safe full-conduction request,
        // not a resistance estimate. It may authorize only its bounded
        // colder-to-warmer storage-bucket interval; lower-current observations
        // must not clamp a full 3 A contract. A power-safe terminal tracking
        // point is eligible at the same boundary, but carries its lower
        // encoded current.
        // The compact recorder keeps one transient point per 10 C bucket. A
        // A strict one-degree lookup would therefore fall back to the preset
        // model for most of the warm-up and periodically drop the requested
        // voltage. Carry the closest colder observation through the next
        // bucket. The live RTD code is the authoritative edge for a stored
        // observation: its temperature projection can round just below the
        // stored point even though the ADC has already reached that point.
        // A warmer observation is never promoted early because the live RTD
        // code must reach the stored code before its higher voltage is used.
        let current_class = if query.current_ma < 3_000 {
            self.points[..usize::from(self.point_count)]
                .iter()
                .filter(|point| {
                    !point.tracking_only && point.request_current_ma <= query.current_ma
                })
                .map(|point| point.request_current_ma)
                .max()
        } else {
            None
        };
        let mut selected: Option<(BoundaryPoint, i16)> = None;
        for point in self.points[..usize::from(self.point_count)]
            .iter()
            .copied()
            .filter(|point| {
                if query.current_ma >= 3_000 {
                    point.tracking_only
                        && point.request_current_ma <= query.current_ma
                        && (point.request_current_ma == 3_000
                            || terminal_tracking_current_ma(
                                self.source_power_limit_mw,
                                point.tracking_request_mv,
                            ) == Some(point.request_current_ma))
                } else {
                    (!point.tracking_only && Some(point.request_current_ma) == current_class)
                        || (point.tracking_only
                            && point.request_current_ma == query.current_ma
                            && terminal_tracking_current_ma(
                                self.source_power_limit_mw,
                                point.tracking_request_mv,
                            ) == Some(point.request_current_ma))
                }
            })
        {
            let Some(point_temperature) = project_temperature(point.rtd_mv) else {
                continue;
            };
            // A power-safe terminal point is a warmer request than the
            // operational 3 A ceiling. Do not promote it merely because a
            // quantized RTD sample equals the stored terminal code while the
            // lower request is still confirmed; require the same bounded
            // temperature margin used for a confirmed handoff. Once the
            // terminal request is already confirmed, the smaller handoff
            // window remains valid so ordinary RTD dips do not create pulses.
            let terminal_point = point.tracking_only
                && point.request_current_ma < 3_000
                && terminal_tracking_current_ma(
                    self.source_power_limit_mw,
                    point.tracking_request_mv,
                ) == Some(point.request_current_ma);
            let terminal_promotion_requires_margin = terminal_point
                && query.current_request_mv.is_none_or(|current_request_mv| {
                    current_request_mv
                        < point
                            .tracking_request_mv
                            .saturating_sub(TERMINAL_HANDOFF_REQUEST_STEP_MV)
                });
            let temperature_gap =
                i32::from(query.temperature_centi_c) - i32::from(point_temperature);
            let live_rtd_reached = query
                .current_rtd_mv
                .is_some_and(|current_rtd_mv| current_rtd_mv >= point.rtd_mv);
            let within_temperature_gap = !terminal_promotion_requires_margin
                && (0..=i32::from(MAX_OBSERVED_LOOKUP_GAP_CENTI_C)).contains(&temperature_gap);
            // The power-safe terminal pair is the one intentional exception
            // to the warmer-point rule. A 3 A point can thermally stall just
            // below its terminal observation; allowing the bounded handoff
            // window lets 21 V/2.85 A take over without ever exceeding the
            // source power envelope. Ordinary measured points still require
            // the live RTD edge or a colder carry window.
            let terminal_temperature_gap =
                i32::from(point_temperature) - i32::from(query.temperature_centi_c);
            let terminal_handoff_allowed = terminal_point
                && if terminal_promotion_requires_margin {
                    (1..=i32::from(MAX_TERMINAL_PRE_HANDOFF_GAP_CENTI_C))
                        .contains(&terminal_temperature_gap)
                } else {
                    (0..=i32::from(MAX_OBSERVED_LOOKUP_GAP_CENTI_C))
                        .contains(&terminal_temperature_gap)
                };
            // Once the power-safe terminal contract is confirmed, the heater
            // may remain blanked while the PD settle window runs. Its
            // source-power-bounded terminal pair stays eligible across that
            // transient temperature drop; a new capability or a different
            // request still has to pass the normal lookup.
            let confirmed_request_handoff_allowed = point.tracking_only
                && query.current_request_mv == Some(point.tracking_request_mv)
                && (0..=i32::from(MAX_CONFIRMED_REQUEST_HANDOFF_GAP_CENTI_C))
                    .contains(&terminal_temperature_gap);
            if !live_rtd_reached
                && !within_temperature_gap
                && !terminal_handoff_allowed
                && !confirmed_request_handoff_allowed
            {
                continue;
            }
            if selected.is_none_or(|(previous, previous_temperature)| {
                point_temperature > previous_temperature
                    || (point_temperature == previous_temperature
                        && point.tracking_request_mv > previous.tracking_request_mv)
            }) {
                selected = Some((point, point_temperature));
            }
        }
        // The selected point is the latest safe ceiling for the current
        // temperature bucket. If a previous contract is above that ceiling,
        // return the point so the automatic controller can blank the heater,
        // request the lower voltage, and resume only after confirmation.
        // Refusing the lookup here would discard the measured boundary and
        // incorrectly fall back to the much lower theoretical model.
        selected.map(|(point, _)| point)
    }

    pub fn safe_request_mv(
        &self,
        query: BoundaryQuery,
        project_temperature: impl FnMut(u16) -> Option<i16>,
    ) -> Option<u16> {
        self.safe_point(query, project_temperature)
            .map(|point| point.tracking_request_mv)
    }

    /// Keep a valid boundary fail-closed before its first observed temperature.
    /// The coldest retained tracking point is the lower edge of the measured
    /// envelope; using that observed point avoids both the unconstrained
    /// resistance model and a lower APDO floor that the relative VIN guard
    /// cannot validate during cold startup.
    pub fn safe_point_or_cold_start(
        &self,
        query: BoundaryQuery,
        mut project_temperature: impl FnMut(u16) -> Option<i16>,
    ) -> Option<BoundaryPoint> {
        if let Some(point) = self.safe_point(query, &mut project_temperature) {
            return Some(point);
        }
        if !self.has_minimum_coverage(&mut project_temperature) {
            return None;
        }
        if !self.is_valid()
            || self.wifi_started != query.wifi_started
            || self.advertised_min_mv != query.source_min_mv
            || self.advertised_max_mv != query.source_max_mv
            || self.source_min_mv != query.operational_source_min_mv
            || self.source_max_mv != query.operational_source_max_mv
            || self.source_max_ma != query.source_max_ma
            || self.source_power_limit_mw != query.source_power_limit_mw
        {
            return None;
        }
        let current_class = self.points[..usize::from(self.point_count)]
            .iter()
            .filter(|point| point.request_current_ma == 3_000 && point.tracking_only)
            .map(|point| point.request_current_ma)
            .max()?;
        if query.current_ma >= 3_000 && current_class < 3_000 {
            return None;
        }
        let (coldest_temperature, coldest_point) = self.points[..usize::from(self.point_count)]
            .iter()
            .copied()
            .filter(|point| point.request_current_ma == current_class && point.tracking_only)
            .filter_map(|point| {
                project_temperature(point.rtd_mv).map(|temperature| (temperature, point))
            })
            .min_by_key(|(temperature, point)| (*temperature, point.tracking_request_mv))?;
        (query.temperature_centi_c < coldest_temperature).then_some(coldest_point)
    }

    pub fn safe_request_mv_or_cold_start(
        &self,
        query: BoundaryQuery,
        project_temperature: impl FnMut(u16) -> Option<i16>,
    ) -> Option<u16> {
        self.safe_point_or_cold_start(query, project_temperature)
            .map(|point| point.tracking_request_mv.max(query.source_min_mv))
    }

    /// Compare raw ADC response with a retained tracking observation. This is
    /// a relative sag guard, not an absolute voltage or CC-state measurement.
    pub fn voltage_response_matches(point: BoundaryPoint, request_mv: u16, vin_mv: u16) -> bool {
        u64::from(vin_mv) * u64::from(point.tracking_request_mv) * 100
            >= u64::from(point.vin_mv) * u64::from(request_mv) * 95
    }

    pub fn retain(&mut self, point: BoundaryPoint) -> bool {
        if let Some(existing) = self.points[..usize::from(self.point_count)]
            .iter_mut()
            .find(|existing| {
                !point.tracking_only
                    && !existing.tracking_only
                    && existing.request_current_ma == point.request_current_ma
            })
        {
            *existing = point;
            return true;
        }
        let Some(slot) = self.points.get_mut(usize::from(self.point_count)) else {
            return false;
        };
        *slot = point;
        self.point_count += 1;
        true
    }

    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        if !self.is_valid() {
            return None;
        }
        let len = BOUNDARY_HEADER_BYTES + usize::from(self.point_count) * BOUNDARY_POINT_BYTES;
        let out = out.get_mut(..len)?;
        out.fill(0);
        out[0] = BOUNDARY_FORMAT_VERSION;
        out[1] = u8::from(self.wifi_started);
        out[2] = self.point_count;
        out[3] = self.adc.efuse_version;
        out[4] = self.adc.attenuation_db;
        out[5] = u8::from(self.cutoff_reached);
        for (offset, value) in [
            (6, self.adc.divider_supply_mv),
            (8, self.adc.reference_mv),
            (10, self.adc.reference_code),
            (12, self.adc.init_code),
            (14, self.source_min_mv),
            (16, self.source_max_mv),
            (18, self.source_max_ma),
            (20, self.advertised_min_mv),
            (22, self.advertised_max_mv),
        ] {
            out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
        }
        let power_units = u16::try_from(self.source_power_limit_mw / 10).ok()?;
        out[24..26].copy_from_slice(&power_units.to_le_bytes());
        for (index, point) in self.points[..usize::from(self.point_count)]
            .iter()
            .enumerate()
        {
            let start = BOUNDARY_HEADER_BYTES + index * BOUNDARY_POINT_BYTES;
            let bytes = &mut out[start..start + BOUNDARY_POINT_BYTES];
            let current = match (point.tracking_only, point.request_current_ma) {
                (false, request_current_ma) => (request_current_ma - 1_000) / 100,
                (true, 3_000) => THREE_AMP_TRACKING_CURRENT_CODE,
                (true, _) => TERMINAL_TRACKING_CURRENT_CODE,
            };
            let packed = u128::from(point.elapsed_ms / 50)
                | (u128::from(point.rtd_code) << 19)
                | (u128::from(point.rtd_mv) << 31)
                | (u128::from(point.vin_code) << 43)
                | (u128::from(point.vin_mv) << 55)
                | (u128::from(point.tracking_request_mv / 100) << 67)
                | (u128::from(point.flat_request_mv / 100) << 75)
                | (u128::from(current) << 83);
            bytes.copy_from_slice(&packed.to_le_bytes()[..BOUNDARY_POINT_BYTES]);
        }
        Some(len)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < BOUNDARY_HEADER_BYTES
            || bytes[0] != BOUNDARY_FORMAT_VERSION
            || bytes[1] > 1
            || bytes[5] > 1
            || bytes[2] as usize > MAX_BOUNDARY_POINTS
            || bytes.len() != BOUNDARY_HEADER_BYTES + bytes[2] as usize * BOUNDARY_POINT_BYTES
        {
            return None;
        }
        let read = |offset| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        let mut result = Self {
            adc: BoundaryAdcMetadata {
                divider_supply_mv: read(6),
                reference_mv: read(8),
                reference_code: read(10),
                init_code: read(12),
                efuse_version: bytes[3],
                attenuation_db: bytes[4],
            },
            wifi_started: bytes[1] == 1,
            cutoff_reached: bytes[5] == 1,
            source_min_mv: read(14),
            source_max_mv: read(16),
            source_max_ma: read(18),
            advertised_min_mv: read(20),
            advertised_max_mv: read(22),
            source_power_limit_mw: u32::from(u16::from_le_bytes([bytes[24], bytes[25]])) * 10,
            point_count: bytes[2],
            ..Self::default()
        };
        for (index, point) in result.points[..usize::from(result.point_count)]
            .iter_mut()
            .enumerate()
        {
            let start = BOUNDARY_HEADER_BYTES + index * BOUNDARY_POINT_BYTES;
            let mut encoded = [0; 16];
            encoded[..BOUNDARY_POINT_BYTES]
                .copy_from_slice(&bytes[start..start + BOUNDARY_POINT_BYTES]);
            let packed = u128::from_le_bytes(encoded);
            let current = ((packed >> 83) & 31) as u16;
            if current > 20
                && current != TERMINAL_TRACKING_CURRENT_CODE
                && current != THREE_AMP_TRACKING_CURRENT_CODE
            {
                return None;
            }
            let tracking_request_mv = ((packed >> 67) & 0xff) as u16 * 100;
            *point = BoundaryPoint {
                elapsed_ms: (packed as u32 & ((1 << 19) - 1)) * 50,
                rtd_code: ((packed >> 19) & 0xfff) as u16,
                rtd_mv: ((packed >> 31) & 0xfff) as u16,
                vin_code: ((packed >> 43) & 0xfff) as u16,
                vin_mv: ((packed >> 55) & 0xfff) as u16,
                tracking_request_mv,
                flat_request_mv: ((packed >> 75) & 0xff) as u16 * 100,
                request_current_ma: if current == THREE_AMP_TRACKING_CURRENT_CODE {
                    3_000
                } else if current == TERMINAL_TRACKING_CURRENT_CODE {
                    terminal_tracking_current_ma(result.source_power_limit_mw, tracking_request_mv)?
                } else {
                    1_000 + current * 100
                },
                tracking_only: current == TERMINAL_TRACKING_CURRENT_CODE
                    || current == THREE_AMP_TRACKING_CURRENT_CODE,
            };
        }
        result.is_valid().then_some(result)
    }
}

/// Require the measured temperature to remain within a 1 C band for ten
/// seconds. The band bounds the complete window, so normal sensor noise and
/// slow thermal drift inside that band do not restart it.
#[derive(Clone, Copy, Debug, Default)]
pub struct EquilibriumWindow {
    started_ms: Option<u32>,
    min_centi_c: i16,
    max_centi_c: i16,
}

impl EquilibriumWindow {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn observe(&mut self, now_ms: u32, temperature_centi_c: i16) -> bool {
        if self.started_ms.is_none() {
            self.started_ms = Some(now_ms);
            self.min_centi_c = temperature_centi_c;
            self.max_centi_c = temperature_centi_c;
            return false;
        }
        self.min_centi_c = self.min_centi_c.min(temperature_centi_c);
        self.max_centi_c = self.max_centi_c.max(temperature_centi_c);
        if i32::from(self.max_centi_c) - i32::from(self.min_centi_c) > 100 {
            self.reset();
            return self.observe(now_ms, temperature_centi_c);
        }
        let elapsed = now_ms.saturating_sub(self.started_ms.unwrap_or(now_ms));
        if elapsed < 10_000 {
            return false;
        }
        true
    }
}

#[cfg(test)]
#[path = "heater_supply_tests.rs"]
mod tests;
