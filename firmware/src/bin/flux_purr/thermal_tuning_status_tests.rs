use super::*;

#[test]
fn ordinary_status_reports_tuning_activity_without_an_owner_proof() {
    let ui = FrontPanelUiState::new(FrontPanelRuntimeMode::App);
    for active in [false, true, false] {
        let status = usb_runtime_status(
            &ui,
            &MemoryConfig::default(),
            UsbRuntimeStatusContext {
                thermal_tuning_active: active,
                ..test_usb_runtime_status_context()
            },
        );
        assert_eq!(status.thermal_tuning_active, active);
        let wire = serde_json::to_value(&*status).unwrap();
        assert_eq!(wire["thermalTuningActive"], active);
        assert!(wire.get("ownerProof").is_none());
    }
}
