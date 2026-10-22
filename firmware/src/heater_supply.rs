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
// Version 2 deliberately invalidates boundaries produced by the earlier
// lookup semantics. Old calibration is absent rather than silently reused.
pub const BOUNDARY_FORMAT_VERSION: u8 = 2;
pub const BOUNDARY_POINT_BYTES: usize = 11;
pub const BOUNDARY_HEADER_BYTES: usize = 24;
pub const BOUNDARY_MAX_BYTES: usize =
    BOUNDARY_HEADER_BYTES + MAX_BOUNDARY_POINTS * BOUNDARY_POINT_BYTES;
// A persisted RTD point and a live RTD sample are rounded ADC millivolts.
// Treat the adjacent two-code quantization band as the same edge, but never
// carry a warmer observation farther than the existing 1 C equilibrium window.
const MAX_QUANTIZED_RTD_DELTA_MV: u16 = 2;
const MAX_QUANTIZED_TEMPERATURE_LEAD_CENTI_C: i32 = 100;
// The compact 10 C scan bins are stored using rounded ADC temperatures. A
// point near the lower edge of one bin can be followed by the next retained
// point near the upper edge of the following bin, leaving a real cold-point
// gap close to 18 C. Carry that colder proof through the bounded gap instead
// of falling back to the unconstrained model; never use a warmer point.
const MAX_OBSERVED_LOOKUP_GAP_CENTI_C: i16 = 1_800;

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
    pub source_min_mv: u16,
    pub source_max_mv: u16,
    pub source_max_ma: u16,
}

impl SupplyBoundary {
    pub fn is_valid(&self) -> bool {
        (1..=MAX_BOUNDARY_POINTS as u8).contains(&self.point_count)
            && (3_000..=3_600).contains(&self.adc.divider_supply_mv)
            && self.source_min_mv >= 5_500
            && self.source_max_mv <= 21_000
            && self.source_min_mv <= self.source_max_mv
            && self.advertised_min_mv > 0
            && self.advertised_min_mv <= self.source_min_mv
            && self.advertised_max_mv >= self.source_max_mv
            && self.source_max_ma >= 3_000
            && self.points[..usize::from(self.point_count)]
                .iter()
                .all(|point| {
                    (MIN_HEATED_CURRENT_MA..=3_000).contains(&point.request_current_ma)
                        && point.request_current_ma.is_multiple_of(100)
                        && (self.source_min_mv..=self.source_max_mv)
                            .contains(&point.tracking_request_mv)
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
                        && (!point.tracking_only || point.request_current_ma == 3_000)
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
            || self.advertised_min_mv != query.source_min_mv
            || self.advertised_max_mv != query.source_max_mv
            || self.source_max_ma != query.source_max_ma
        {
            return None;
        }
        // A 3 A tracking point is an observed safe full-conduction request,
        // not a resistance estimate. It may authorize only its bounded
        // colder-to-warmer storage-bucket interval; lower-current observations
        // must not clamp a full 3 A contract. This lets the compact calibration
        // scan provide continuous warm-up coverage without falling back to a
        // lower theoretical voltage at every lookup gap.
        let current_class = self.points[..usize::from(self.point_count)]
            .iter()
            .filter(|point| {
                point.request_current_ma <= query.current_ma
                    && (!point.tracking_only
                        || (query.current_ma >= 3_000 && point.request_current_ma == 3_000))
            })
            .map(|point| point.request_current_ma)
            .max()?;
        if query.current_ma >= 3_000 && current_class < 3_000 {
            return None;
        }
        // The compact recorder keeps one transient point per 10 C bucket. A
        // strict one-degree lookup would therefore fall back to the preset
        // model for most of the warm-up and periodically drop the requested
        // voltage. Carry the closest colder observation through the next
        // bucket and its rounded-ADC edge. Once the confirmed request has
        // reached that tracking point, promote exactly one next observed 3 A
        // point within the same bounded window; this is a measured staircase,
        // not a resistance extrapolation.
        let mut selected: Option<(BoundaryPoint, i16)> = None;
        for point in self.points[..usize::from(self.point_count)]
            .iter()
            .copied()
            .filter(|point| {
                point.request_current_ma == current_class
                    && (!point.tracking_only
                        || (query.current_ma >= 3_000 && point.request_current_ma == 3_000))
            })
        {
            let Some(point_temperature) = project_temperature(point.rtd_mv) else {
                continue;
            };
            let temperature_gap =
                i32::from(query.temperature_centi_c) - i32::from(point_temperature);
            let adjacent_quantized_rtd = query.current_rtd_mv.is_some_and(|current_rtd_mv| {
                point.rtd_mv > current_rtd_mv
                    && point.rtd_mv - current_rtd_mv <= MAX_QUANTIZED_RTD_DELTA_MV
                    && temperature_gap >= -MAX_QUANTIZED_TEMPERATURE_LEAD_CENTI_C
            });
            if !(0..=i32::from(MAX_OBSERVED_LOOKUP_GAP_CENTI_C)).contains(&temperature_gap)
                && !adjacent_quantized_rtd
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
        let (selected_point, selected_temperature) = selected?;
        if selected_point.tracking_only
            && query.current_request_mv.is_some_and(|current_request_mv| {
                current_request_mv >= selected_point.tracking_request_mv
            })
        {
            let current_request_mv = query.current_request_mv.unwrap_or_default();
            let next_point = self.points[..usize::from(self.point_count)]
                .iter()
                .copied()
                .filter(|point| {
                    point.tracking_only
                        && point.request_current_ma == current_class
                        && point.tracking_request_mv > selected_point.tracking_request_mv
                        && point.tracking_request_mv >= current_request_mv
                })
                .filter_map(|point| {
                    let temperature = project_temperature(point.rtd_mv)?;
                    let gap = i32::from(temperature) - i32::from(query.temperature_centi_c);
                    (temperature > selected_temperature
                        && (0..=i32::from(MAX_OBSERVED_LOOKUP_GAP_CENTI_C)).contains(&gap))
                    .then_some((temperature, point))
                })
                .min_by_key(|(temperature, point)| (*temperature, point.tracking_request_mv));
            if let Some((_, next_point)) = next_point {
                return Some(next_point);
            }
            if current_request_mv > selected_point.tracking_request_mv {
                // A request above the selected point needs its own observed
                // authorization; never silently step it down through an
                // unrelated colder observation.
                return None;
            }
        }
        Some(selected_point)
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
    /// envelope; using that observed voltage avoids both the unconstrained
    /// resistance model and a lower APDO floor that the relative VIN guard
    /// cannot validate during cold startup.
    pub fn safe_request_mv_or_cold_start(
        &self,
        query: BoundaryQuery,
        mut project_temperature: impl FnMut(u16) -> Option<i16>,
    ) -> Option<u16> {
        if let Some(request_mv) = self.safe_request_mv(query, &mut project_temperature) {
            return Some(request_mv);
        }
        if !self.is_valid()
            || self.wifi_started != query.wifi_started
            || self.advertised_min_mv != query.source_min_mv
            || self.advertised_max_mv != query.source_max_mv
            || self.source_max_ma != query.source_max_ma
        {
            return None;
        }
        let current_class = self.points[..usize::from(self.point_count)]
            .iter()
            .filter(|point| {
                point.request_current_ma <= query.current_ma
                    && (!point.tracking_only
                        || (query.current_ma >= 3_000 && point.request_current_ma == 3_000))
            })
            .map(|point| point.request_current_ma)
            .max()?;
        if query.current_ma >= 3_000 && current_class < 3_000 {
            return None;
        }
        let (coldest_temperature, coldest_point) = self.points[..usize::from(self.point_count)]
            .iter()
            .copied()
            .filter(|point| {
                point.request_current_ma == current_class
                    && (!point.tracking_only
                        || (query.current_ma >= 3_000 && point.request_current_ma == 3_000))
            })
            .filter_map(|point| {
                project_temperature(point.rtd_mv).map(|temperature| (temperature, point))
            })
            .min_by_key(|(temperature, point)| (*temperature, point.tracking_request_mv))?;
        (query.temperature_centi_c < coldest_temperature)
            .then_some(coldest_point.tracking_request_mv.max(query.source_min_mv))
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
        for (index, point) in self.points[..usize::from(self.point_count)]
            .iter()
            .enumerate()
        {
            let start = BOUNDARY_HEADER_BYTES + index * BOUNDARY_POINT_BYTES;
            let bytes = &mut out[start..start + BOUNDARY_POINT_BYTES];
            let current = if point.tracking_only {
                31
            } else {
                (point.request_current_ma - 1_000) / 100
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
            if current > 20 && current != 31 {
                return None;
            }
            *point = BoundaryPoint {
                elapsed_ms: (packed as u32 & ((1 << 19) - 1)) * 50,
                rtd_code: ((packed >> 19) & 0xfff) as u16,
                rtd_mv: ((packed >> 31) & 0xfff) as u16,
                vin_code: ((packed >> 43) & 0xfff) as u16,
                vin_mv: ((packed >> 55) & 0xfff) as u16,
                tracking_request_mv: ((packed >> 67) & 0xff) as u16 * 100,
                flat_request_mv: ((packed >> 75) & 0xff) as u16 * 100,
                request_current_ma: if current == 31 {
                    3_000
                } else {
                    1_000 + current * 100
                },
                tracking_only: current == 31,
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
