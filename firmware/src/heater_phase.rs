//! Physical phase admission shared by production control and tuning evidence.

pub const HOLD_ENTRY_BELOW_C: f32 = 0.5;
pub const HOLD_ENTRY_ABOVE_C: f32 = 3.0;

/// Candidate curve parameters cannot widen the physical Hold entry band.
pub fn hold_entry_ready(error_c: f32) -> bool {
    (-HOLD_ENTRY_ABOVE_C..=HOLD_ENTRY_BELOW_C).contains(&error_c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_hold_entry_rejects_under_temperature_and_invalid_measurements() {
        for error in [0.51, 2.51, 4.14, -3.01, f32::NAN, f32::INFINITY] {
            assert!(!hold_entry_ready(error));
        }
        for error in [0.5, 0.0, -3.0] {
            assert!(hold_entry_ready(error));
        }
    }
}
