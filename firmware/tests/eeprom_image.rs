//! Decode an exported EEPROM image with the firmware's own record codec.
//!
//! This is the check the snapshot claim depends on: a byte-identical image only
//! proves transport fidelity, not that the bytes are a valid FPR2 snapshot.
//! Point `FLUX_PURR_EEPROM_IMAGE` at a `backup-*.bin` and run
//! `cargo test --manifest-path firmware/Cargo.toml --test eeprom_image -- --ignored --nocapture`.

use flux_purr_firmware::memory::{
    FPR2_LAYOUT_A_OFFSET, FPR2_LAYOUT_B_OFFSET, FPR2_NETWORK_OFFSET, FPR2_PREFERENCES_OFFSET,
    FPR2_SAFETY_A_OFFSET, FPR2_SAFETY_B_OFFSET, FPR2_THERMAL_A_OFFSET, FPR2_THERMAL_B_OFFSET,
    FPR2_THERMAL_PLANT_OFFSET, FPR2_THERMAL_SLOT_SIZE, LayoutMarker, LayoutMarkerStatus,
    MemoryConfig, PersistDomain, PersistDomainData, PersistRecord, decode_persist_record,
    fpr2_snapshot_is_complete,
};

const DATA_SLOTS: [(&str, u16); 7] = [
    ("safety A", FPR2_SAFETY_A_OFFSET),
    ("safety B", FPR2_SAFETY_B_OFFSET),
    ("thermal A", FPR2_THERMAL_A_OFFSET),
    ("thermal B", FPR2_THERMAL_B_OFFSET),
    ("preferences", FPR2_PREFERENCES_OFFSET),
    ("network", FPR2_NETWORK_OFFSET),
    ("thermal plant", FPR2_THERMAL_PLANT_OFFSET),
];

type Records = [Option<PersistRecord>; 6];

fn decode_data_slots(image: &[u8]) -> Records {
    let mut records: Records = Default::default();
    for (name, offset) in DATA_SLOTS {
        let slot = &image[usize::from(offset)..];
        let record = decode_persist_record(slot)
            .unwrap_or_else(|error| panic!("{name} at 0x{offset:04x} failed: {error:?}"));
        assert_eq!(
            record.data.domain() as u8,
            slot[5],
            "{name}: decoded domain must match the header"
        );
        assert_ne!(
            record.data.domain(),
            PersistDomain::LayoutMarker,
            "{name}: unexpected layout marker in a data slot"
        );
        println!(
            "{name:14} seq={:<4} domain={:?} slot_bytes={}",
            record.sequence,
            record.data.domain(),
            slot.len()
        );
        let index = record.data.domain() as usize - 1;
        records[index] = Some(record);
    }
    records
}

fn decode_markers(image: &[u8]) -> Vec<LayoutMarker> {
    [
        ("layout A", FPR2_LAYOUT_A_OFFSET),
        ("layout B", FPR2_LAYOUT_B_OFFSET),
    ]
    .into_iter()
    .map(|(name, offset)| {
        let record = decode_persist_record(&image[usize::from(offset)..])
            .unwrap_or_else(|error| panic!("{name} failed to decode: {error:?}"));
        assert_eq!(record.data.domain(), PersistDomain::LayoutMarker);
        let PersistDomainData::LayoutMarker(marker) = record.data else {
            panic!("{name} is not a layout marker");
        };
        println!(
            "{name:14} seq={:<4} status={:?} kind={:?} generation={}",
            record.sequence, marker.status, marker.kind, marker.generation
        );
        marker
    })
    .collect()
}

fn apply_records(records: &Records) -> MemoryConfig {
    let mut config = MemoryConfig::default();
    for record in records.iter().flatten() {
        match &record.data {
            PersistDomainData::SafetyCalibration(value) => value.apply_to_config(&mut config),
            PersistDomainData::ThermalPolicy(value) => value.apply_to_config(&mut config),
            PersistDomainData::UserPreferences(value) => value.apply_to_config(&mut config),
            PersistDomainData::NetworkAndPairing(value) => value.apply_to_config(&mut config),
            PersistDomainData::LayoutMarker(_) => {}
            PersistDomainData::ThermalPlant(value) => value.apply_to_config(&mut config),
        }
    }
    config
}

fn report_config(config: &MemoryConfig) {
    println!("--- decoded configuration ---");
    println!("commissioning_required = {}", config.commissioning_required);
    println!("target_temp_c = {}", config.target_temp_c);
    println!("selected_preset_slot = {}", config.selected_preset_slot);
    println!("presets_c = {:?}", config.presets_c);
    println!(
        "post_heat_cooling = {:?} heating_fan_guard = {:?}",
        config.post_heat_cooling_mode, config.heating_fan_guard_mode
    );
    println!("thermal_profile_mode = {:?}", config.thermal_profile_mode);
    println!(
        "r20_profile_points = {} pps5a_points = {}",
        config.active_thermal_control_profile.points.len(),
        config.thermal_control_profile_pps5a.points.len()
    );
    println!(
        "supply_boundary = {} thermal_transient = {}",
        config.heater_supply_boundary.is_some(),
        config.thermal_plant_transient_active.is_some()
    );
    // Wi-Fi secrets must never be printed.
    println!(
        "wifi_ssid = {:?} wifi_password = <redacted {} bytes>",
        config.wifi_ssid,
        config.wifi_password.len()
    );
}

fn active_generation(markers: &[LayoutMarker]) -> u32 {
    assert!(
        markers
            .iter()
            .any(|marker| marker.status == LayoutMarkerStatus::Active),
        "no ACTIVE layout marker; the snapshot would not be restorable"
    );
    markers
        .iter()
        .filter(|marker| marker.status == LayoutMarkerStatus::Active)
        .map(|marker| marker.generation)
        .max()
        .expect("active generation")
}

#[test]
#[ignore = "requires FLUX_PURR_EEPROM_IMAGE to point at an exported 8192-byte image"]
fn exported_eeprom_image_decodes_as_a_complete_fpr2_snapshot() {
    let path = std::env::var("FLUX_PURR_EEPROM_IMAGE").expect("image path");
    let image = std::fs::read(&path).expect("read image");
    assert_eq!(
        image.len(),
        8192,
        "an M24C64 image must be exactly 8192 bytes"
    );

    let records = decode_data_slots(&image);
    let markers = decode_markers(&image);
    let generation = active_generation(&markers);

    assert!(
        fpr2_snapshot_is_complete(&records, generation),
        "domains are not a complete snapshot at generation {generation}"
    );
    for record in records.iter().flatten() {
        assert!(
            record.sequence <= generation,
            "{:?} sequence {} is newer than the active generation {generation}",
            record.data.domain(),
            record.sequence
        );
    }
    // Every byte after the last FPR2 slot must stay blank: the firmware never
    // writes the legacy FPM1 region or the tail of the device.
    let blank_from = usize::from(FPR2_THERMAL_PLANT_OFFSET) + FPR2_THERMAL_SLOT_SIZE;
    assert!(
        image[blank_from..].iter().all(|byte| *byte == 0xff),
        "region 0x{blank_from:04x}.. is not blank"
    );

    let config = apply_records(&records);
    report_config(&config);
    assert!(
        (0..=400).contains(&config.target_temp_c),
        "decoded target temperature is outside the firmware clamp range"
    );
    assert!(
        config.selected_preset_slot < config.presets_c.len(),
        "decoded preset slot is out of range"
    );

    println!("OK: complete FPR2 snapshot at generation {generation}; legacy region blank");
}
