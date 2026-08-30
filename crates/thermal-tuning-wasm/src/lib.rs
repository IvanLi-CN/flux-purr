use flux_purr_thermal_tuning_core::{
    CANDIDATE_PROFILE_CANONICAL_BYTES, CandidateProfile, EXECUTION_ORDER_C, PHYSICAL_TARGETS_C,
    PpsPowerClass, sha256,
};
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
