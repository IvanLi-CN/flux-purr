//! Qualification continuity across a bounded PPS sampling pause.
//! Rejected observations never become temperature or timing witnesses.

#[derive(Debug, Clone, Copy)]
pub struct QualificationObservation {
    pub elapsed_ms: u64,
    pub requested_mv: Option<u16>,
    pub valid: bool,
    pub accepted: bool,
    pub temperature_centi_c: i16,
    pub raw_temperature_centi_c: Option<i16>,
    pub target_c: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualificationMeasurement {
    Fresh,
    FreshAfterGap,
    PpsPause,
    Rejected,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QualificationContinuity {
    last_request_mv: Option<u16>,
    last_accepted_at_ms: Option<u64>,
    pps_edge_at_ms: Option<u64>,
}

impl QualificationContinuity {
    pub const fn new() -> Self {
        Self {
            last_request_mv: None,
            last_accepted_at_ms: None,
            pps_edge_at_ms: None,
        }
    }

    pub fn observe(&mut self, sample: QualificationObservation) -> QualificationMeasurement {
        if self
            .last_request_mv
            .zip(sample.requested_mv)
            .is_some_and(|(a, b)| a != b)
        {
            self.pps_edge_at_ms = Some(sample.elapsed_ms);
        }
        self.last_request_mv = sample.requested_mv;
        if sample.valid && sample.accepted {
            let pause_expired = self.pps_edge_at_ms.is_some()
                && self
                    .last_accepted_at_ms
                    .is_some_and(|last| sample.elapsed_ms.saturating_sub(last) > 1_000);
            // The request edge can precede the ADC pause by one control tick.
            if self
                .last_accepted_at_ms
                .is_some_and(|last| sample.elapsed_ms > last)
                && self.pps_edge_at_ms != Some(sample.elapsed_ms)
            {
                self.pps_edge_at_ms = None;
            }
            self.last_accepted_at_ms = Some(sample.elapsed_ms);
            return if pause_expired {
                QualificationMeasurement::FreshAfterGap
            } else {
                QualificationMeasurement::Fresh
            };
        }
        let in_band = |temperature: i16| {
            (i32::from(temperature) - i32::from(sample.target_c) * 100).abs() <= 150
        };
        let recent = |time| {
            sample
                .elapsed_ms
                .checked_sub(time)
                .is_some_and(|age| age <= 1_000)
        };
        if sample.valid
            && self.pps_edge_at_ms.is_some_and(recent)
            && self.last_accepted_at_ms.is_some_and(recent)
            && in_band(sample.temperature_centi_c)
            && sample
                .raw_temperature_centi_c
                .is_some_and(|raw| in_band(raw) && raw.abs_diff(sample.temperature_centi_c) <= 50)
        {
            return QualificationMeasurement::PpsPause;
        }
        self.pps_edge_at_ms = None;
        self.last_accepted_at_ms = None;
        QualificationMeasurement::Rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(time: u64, request: u16, accepted: bool) -> QualificationObservation {
        QualificationObservation {
            elapsed_ms: time,
            requested_mv: Some(request),
            valid: true,
            accepted,
            temperature_centi_c: 22_000,
            raw_temperature_centi_c: Some(21_997),
            target_c: 220,
        }
    }

    #[test]
    fn request_edge_may_precede_the_rejected_observation() {
        let mut continuity = QualificationContinuity::new();
        for sample in [observation(0, 20_500, true), observation(200, 21_000, true)] {
            assert_eq!(continuity.observe(sample), QualificationMeasurement::Fresh);
        }
        assert_eq!(
            continuity.observe(observation(300, 21_000, false)),
            QualificationMeasurement::PpsPause
        );
        assert_eq!(
            continuity.observe(observation(600, 21_000, false)),
            QualificationMeasurement::PpsPause
        );
        assert_eq!(
            continuity.observe(observation(800, 21_000, true)),
            QualificationMeasurement::Fresh
        );
        assert_eq!(
            continuity.observe(observation(900, 21_000, false)),
            QualificationMeasurement::Rejected
        );
    }

    #[test]
    fn unknown_bad_or_long_pauses_cannot_supply_continuity() {
        for scenario in 0..7 {
            let mut continuity = QualificationContinuity::new();
            continuity.observe(observation(0, 20_500, true));
            let mut rejected = observation(200, 21_000, false);
            match scenario {
                0 => rejected.requested_mv = Some(20_500),
                1 => rejected.raw_temperature_centi_c = None,
                2 => rejected.raw_temperature_centi_c = Some(22_051),
                3 => {
                    rejected.temperature_centi_c = 22_140;
                    rejected.raw_temperature_centi_c = Some(22_151);
                }
                4 => rejected.valid = false,
                5 => rejected.elapsed_ms = 1_001,
                _ => rejected.requested_mv = None,
            }
            assert_eq!(
                continuity.observe(rejected),
                QualificationMeasurement::Rejected,
                "scenario {scenario}"
            );
        }
    }

    #[test]
    fn repeated_edges_do_not_refresh_the_last_accepted_sample() {
        let mut continuity = QualificationContinuity::new();
        continuity.observe(observation(0, 20_500, true));
        assert_eq!(
            continuity.observe(observation(500, 21_000, false)),
            QualificationMeasurement::PpsPause
        );
        assert_eq!(
            continuity.observe(observation(1_001, 20_500, false)),
            QualificationMeasurement::Rejected
        );
    }

    #[test]
    fn late_resume_requires_a_new_hold_origin() {
        let mut continuity = QualificationContinuity::new();
        continuity.observe(observation(0, 20_500, true));
        continuity.observe(observation(200, 21_000, false));
        assert_eq!(
            continuity.observe(observation(1_001, 21_000, true)),
            QualificationMeasurement::FreshAfterGap
        );
    }
}
