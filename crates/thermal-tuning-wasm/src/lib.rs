use flux_purr_thermal_tuning_core::{
    CANDIDATE_POINT_CANONICAL_BYTES, CANDIDATE_PROFILE_CANONICAL_BYTES, CandidatePoint,
    CandidateProfile, EXECUTION_ORDER_C, PHYSICAL_TARGETS_C, PpsPowerClass, sha256,
};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn schema() -> String {
    "thermal_tuning_core_v1".to_string()
}

#[wasm_bindgen]
pub fn physical_targets() -> Vec<i16> {
    PHYSICAL_TARGETS_C.to_vec()
}

#[wasm_bindgen]
pub fn execution_order() -> Vec<i16> {
    EXECUTION_ORDER_C.to_vec()
}

#[wasm_bindgen]
pub fn candidate_hash(power_class: &str) -> Result<String, JsValue> {
    let class = PpsPowerClass::from_str(power_class)
        .ok_or_else(|| JsValue::from_str("power class must be pps3a or pps5a"))?;
    let hash = CandidateProfile::baseline(class).hash();
    Ok(hex(&hash))
}

#[wasm_bindgen]
pub fn verify_candidate_hash(power_class: &str, expected_hash: &str) -> bool {
    candidate_hash(power_class).is_ok_and(|actual| actual == expected_hash)
}

#[wasm_bindgen]
pub fn verify_candidate_profile(
    power_class: &str,
    canonical_profile_hex: &str,
    expected_hash: &str,
) -> bool {
    let Some(power_class) = PpsPowerClass::from_str(power_class) else {
        return false;
    };
    let Some(canonical) = decode_hex::<CANDIDATE_PROFILE_CANONICAL_BYTES>(canonical_profile_hex)
    else {
        return false;
    };
    let expected_marker = match power_class {
        PpsPowerClass::Pps3a => 3,
        PpsPowerClass::Pps5a => 5,
    };
    canonical[0] == expected_marker && hex(&sha256(&canonical)) == expected_hash
}

#[wasm_bindgen]
pub fn decode_candidate_point(canonical_point_hex: &str) -> Result<JsValue, JsValue> {
    let canonical = decode_hex::<CANDIDATE_POINT_CANONICAL_BYTES>(canonical_point_hex)
        .ok_or_else(|| JsValue::from_str("candidate point canonical hex is invalid"))?;
    serde_wasm_bindgen::to_value(&CandidatePointWire::from(
        CandidatePoint::from_canonical_bytes(&canonical),
    ))
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidatePointWire {
    target_temp_c: i16,
    brake_distance_centi_c: u16,
    warmup_power_permille: u16,
    warmup_reenter_centi_c: u16,
    approach_power_permille: u16,
    approach_floor_power_permille: u16,
    approach_damping_exponent_permille: u16,
    approach_tail_window_centi_c: u16,
    hold_power_permille: u16,
    hold_reheat_power_permille: u16,
    hold_entry_centi_c: u16,
    hold_exit_centi_c: u16,
    hold_on_centi_c: u16,
    hold_off_centi_c: u16,
    overshoot_cutoff_centi_c: u16,
    hold_kp_permille_per_c: u16,
    hold_ki_permille_per_c_tick: u16,
    hold_blend_ticks: u16,
    approach_lead_ticks: u16,
    hold_lead_ticks: u16,
}

impl From<CandidatePoint> for CandidatePointWire {
    fn from(point: CandidatePoint) -> Self {
        Self {
            target_temp_c: point.target_c,
            brake_distance_centi_c: point.brake_distance_centi_c,
            warmup_power_permille: point.warmup_power_permille,
            warmup_reenter_centi_c: point.warmup_reenter_centi_c,
            approach_power_permille: point.approach_power_permille,
            approach_floor_power_permille: point.approach_floor_power_permille,
            approach_damping_exponent_permille: point.approach_damping_exponent_permille,
            approach_tail_window_centi_c: point.approach_tail_window_centi_c,
            hold_power_permille: point.hold_power_permille,
            hold_reheat_power_permille: point.hold_reheat_power_permille,
            hold_entry_centi_c: point.hold_entry_centi_c,
            hold_exit_centi_c: point.hold_exit_centi_c,
            hold_on_centi_c: point.hold_on_centi_c,
            hold_off_centi_c: point.hold_off_centi_c,
            overshoot_cutoff_centi_c: point.overshoot_cutoff_centi_c,
            hold_kp_permille_per_c: point.hold_kp_permille_per_c,
            hold_ki_permille_per_c_tick: point.hold_ki_permille_per_c_tick,
            hold_blend_ticks: point.hold_blend_ticks,
            approach_lead_ticks: point.approach_lead_ticks,
            hold_lead_ticks: point.hold_lead_ticks,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[usize::from(byte >> 4)] as char);
        output.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    output
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 {
        return None;
    }
    let mut output = [0u8; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_purr_thermal_tuning_core::{CandidateProfile, PpsPowerClass};

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[test]
    fn facade_matches_native_candidate_and_schedule_vectors() {
        assert_eq!(
            physical_targets(),
            vec![60, 80, 100, 120, 140, 160, 180, 220, 240]
        );
        assert_eq!(
            execution_order(),
            vec![60, 240, 140, 100, 80, 120, 180, 160, 220]
        );
        let native_hash = CandidateProfile::baseline(PpsPowerClass::Pps3a).hash();
        assert_eq!(candidate_hash("pps3a").unwrap(), hex(&native_hash));
        assert!(verify_candidate_hash("pps3a", &hex(&native_hash)));
        assert!(!verify_candidate_hash("pps5a", &hex(&native_hash)));
        let mut canonical = [0u8; CANDIDATE_PROFILE_CANONICAL_BYTES];
        CandidateProfile::baseline(PpsPowerClass::Pps3a).canonical_bytes(&mut canonical);
        let canonical_hex = hex(&canonical);
        assert!(verify_candidate_profile(
            "pps3a",
            &canonical_hex,
            &hex(&native_hash)
        ));
        assert!(!verify_candidate_profile(
            "pps5a",
            &canonical_hex,
            &hex(&native_hash)
        ));
    }

    #[cfg(all(test, target_arch = "wasm32"))]
    #[wasm_bindgen_test::wasm_bindgen_test]
    #[test]
    fn facade_rejects_unsupported_power_classes() {
        assert!(candidate_hash("auto").is_err());
        assert!(!verify_candidate_hash("65w", ""));
    }
}
