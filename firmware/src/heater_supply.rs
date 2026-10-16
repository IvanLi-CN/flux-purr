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
pub const BOUNDARY_FORMAT_VERSION: u8 = 1;
pub const BOUNDARY_POINT_BYTES: usize = 11;
pub const BOUNDARY_HEADER_BYTES: usize = 24;
pub const BOUNDARY_MAX_BYTES: usize =
    BOUNDARY_HEADER_BYTES + MAX_BOUNDARY_POINTS * BOUNDARY_POINT_BYTES;

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
                    (1_000..=3_000).contains(&point.request_current_ma)
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
        // Use the highest observed current class eligible for this contract.
        // A low-current equilibrium must not clamp a 3 A warm-up into a stall.
        let current_class = self.points[..usize::from(self.point_count)]
            .iter()
            .filter(|point| point.request_current_ma <= query.current_ma)
            .map(|point| point.request_current_ma)
            .max()?;
        self.points[..usize::from(self.point_count)]
            .iter()
            .copied()
            .filter(|point| point.request_current_ma == current_class)
            .filter(|point| {
                // A retained sample covers its observed 1 C window only.
                // Extending an old low-voltage probe through all warmer
                // temperatures would turn missing coverage into a power cap.
                project_temperature(point.rtd_mv).is_some_and(|temp| {
                    (temp..=temp.saturating_add(100)).contains(&query.temperature_centi_c)
                })
            })
            .max_by_key(|point| point.tracking_request_mv)
    }

    pub fn safe_request_mv(
        &self,
        query: BoundaryQuery,
        project_temperature: impl FnMut(u16) -> Option<i16>,
    ) -> Option<u16> {
        self.safe_point(query, project_temperature)
            .map(|point| point.tracking_request_mv)
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

/// A 1 C band alone accepts slow ramps. Require both the full band and a small
/// end-to-end drift over the same ten-second window.
#[derive(Clone, Copy, Debug, Default)]
pub struct EquilibriumWindow {
    started_ms: Option<u32>,
    first_centi_c: i16,
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
            self.first_centi_c = temperature_centi_c;
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
        let stable = (i32::from(temperature_centi_c) - i32::from(self.first_centi_c)).abs() <= 20;
        if !stable {
            self.reset();
        }
        stable
    }
}

#[cfg(test)]
#[path = "heater_supply_tests.rs"]
mod tests;
