use crate::memory::ThermalPlantProjection;

pub const APPROACH_MIN_PERMILLE: u16 = 50;

/// Passive heat loss fitted from the calibration cooling trace, in model units.
/// This is an estimate, not an electrical power measurement.
pub fn natural_cooling_loss_mw(
    model: ThermalPlantProjection,
    temperature_c: f32,
    ambient_c: f32,
) -> f32 {
    let temperature_k = (temperature_c + 273.15).max(1.0);
    let ambient_k = (ambient_c + 273.15).max(1.0);
    let temperature_k2 = temperature_k * temperature_k;
    let ambient_k2 = ambient_k * ambient_k;
    (model.convection_mw_per_c * (temperature_c - ambient_c).max(0.0)
        + model.radiation_mw_per_k4
            * (temperature_k2 * temperature_k2 - ambient_k2 * ambient_k2).max(0.0))
    .max(0.0)
}

pub fn natural_cooling_rate_c_per_s(
    model: ThermalPlantProjection,
    temperature_c: f32,
    ambient_c: f32,
) -> Option<f32> {
    if !temperature_c.is_finite()
        || !ambient_c.is_finite()
        || !model.thermal_capacity_mj_per_c.is_finite()
        || model.thermal_capacity_mj_per_c <= 0.0
        || !model.convection_mw_per_c.is_finite()
        || model.convection_mw_per_c < 0.0
        || !model.radiation_mw_per_k4.is_finite()
        || model.radiation_mw_per_k4 < 0.0
        || model.transport_delay_ms > 60_000
    {
        return None;
    }
    let rate =
        -natural_cooling_loss_mw(model, temperature_c, ambient_c) / model.thermal_capacity_mj_per_c;
    rate.is_finite().then_some(rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> ThermalPlantProjection {
        ThermalPlantProjection {
            convection_mw_per_c: 100.0,
            radiation_mw_per_k4: 0.0,
            thermal_capacity_mj_per_c: 800.0,
            transport_delay_ms: 500,
        }
    }

    #[test]
    fn passive_loss_and_cooling_rate_preserve_calibration_energy_units() {
        let model = model();
        assert_eq!(natural_cooling_loss_mw(model, 65.0, 25.0), 4_000.0);
        assert_eq!(natural_cooling_rate_c_per_s(model, 65.0, 25.0), Some(-5.0));
        assert_eq!(natural_cooling_rate_c_per_s(model, 25.0, 25.0), Some(0.0));
        assert_eq!(natural_cooling_loss_mw(model, 20.0, 25.0), 0.0);
    }

    #[test]
    fn invalid_thermal_projection_cannot_provide_cooling_guidance() {
        assert!(
            natural_cooling_rate_c_per_s(
                ThermalPlantProjection {
                    thermal_capacity_mj_per_c: 0.0,
                    ..model()
                },
                60.0,
                25.0
            )
            .is_none()
        );
        assert!(
            natural_cooling_rate_c_per_s(
                ThermalPlantProjection {
                    transport_delay_ms: 60_001,
                    ..model()
                },
                60.0,
                25.0
            )
            .is_none()
        );
        assert!(natural_cooling_rate_c_per_s(model(), f32::NAN, 25.0).is_none());
    }
}
