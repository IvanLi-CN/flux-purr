use super::*;
use flux_purr_firmware::memory::{
    FPR2_MAX_RECORD_SIZE, LayoutMarker, LayoutMarkerKind, LayoutMarkerStatus, PersistDomainData,
    decode_persist_record, encode_persist_record,
};
use std::cell::{Cell, RefCell};

// Poll both sides of a real suspension point without a wall clock or executor.
fn run_publication<T>(future: impl core::future::Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = core::task::Context::from_waker(core::task::Waker::noop());
    for _ in 0..16 {
        if let core::task::Poll::Ready(result) = future.as_mut().poll(&mut context) {
            return result;
        }
    }
    panic!("publication did not complete within the deterministic poll budget");
}

struct PublicationStore {
    clock: Cell<u64>,
    active_verified_at: u64,
    fault: &'static str,
    writes: RefCell<std::vec::Vec<LayoutMarkerStatus>>,
    bytes: RefCell<std::vec::Vec<u8>>,
}

impl PublicationStore {
    fn new(active_verified_at: u64, fault: &'static str) -> Self {
        Self {
            clock: Cell::new(659_990),
            active_verified_at,
            fault,
            writes: RefCell::new(std::vec::Vec::new()),
            bytes: RefCell::new(std::vec::Vec::new()),
        }
    }

    async fn write_verified(&self, status: LayoutMarkerStatus) -> Result<(), &'static str> {
        self.writes.borrow_mut().push(status);
        let mut suspended = false;
        core::future::poll_fn(|_| {
            if core::mem::replace(&mut suspended, true) {
                core::task::Poll::Ready(())
            } else {
                core::task::Poll::Pending
            }
        })
        .await;
        if status == LayoutMarkerStatus::Prepared && self.fault == "abort" {
            return Err("abort");
        }
        if status == LayoutMarkerStatus::Active && self.fault == "write" {
            return Err("write");
        }
        let data = PersistDomainData::LayoutMarker(LayoutMarker {
            generation: 8,
            status,
            kind: LayoutMarkerKind::Commit,
        });
        let mut bytes = [0; FPR2_MAX_RECORD_SIZE];
        let len = encode_persist_record(8, &data, &mut bytes).unwrap();
        self.bytes.replace(bytes[..len].to_vec());
        if status == LayoutMarkerStatus::Active {
            self.clock.set(self.active_verified_at);
            if self.fault == "readback" {
                return Err("readback");
            }
        }
        assert_eq!(
            decode_persist_record(&self.bytes.borrow()).unwrap().data,
            data
        );
        Ok(())
    }

    fn recovered_generation(&self) -> u32 {
        // The previous slot contains ACTIVE generation 7. Boot only considers
        // ACTIVE markers; PREPARED Commit cannot enter migration recovery.
        let record = decode_persist_record(&self.bytes.borrow()).unwrap();
        match record.data {
            PersistDomainData::LayoutMarker(marker)
                if marker.status == LayoutMarkerStatus::Active =>
            {
                marker.generation
            }
            PersistDomainData::LayoutMarker(marker) => {
                assert_eq!(marker.kind, LayoutMarkerKind::Commit);
                7
            }
            _ => panic!("expected a layout marker"),
        }
    }
}

#[test]
fn publication_receipt_and_boot_marker_follow_verified_deadline() {
    for (verified_at, fault, expected) in [
        (659_999, "", Ok(659_999)),
        (660_000, "", Err(TimedPublicationError::Deadline)),
        (660_006, "", Err(TimedPublicationError::Deadline)),
        (659_999, "write", Err(TimedPublicationError::Io("write"))),
        (
            659_999,
            "readback",
            Err(TimedPublicationError::Io("readback")),
        ),
        (660_006, "abort", Err(TimedPublicationError::Io("abort"))),
    ] {
        let store = PublicationStore::new(verified_at, fault);
        let receipt = run_publication(publish_marker_before_deadline(
            Some(660_000),
            || store.clock.get(),
            async |status| store.write_verified(status).await,
        ));
        assert_eq!(receipt, expected);
        if fault == "abort" {
            // Failed revocation is an I/O fault, never a successful receipt.
            assert_eq!(store.recovered_generation(), 8);
        } else {
            assert_eq!(
                store.recovered_generation(),
                if receipt.is_ok() { 8 } else { 7 }
            );
        }
        let (mut calibration, mut manual, mut job, mut memory) = full_range_pending_job();
        let previous = memory.clone();
        store.clock.set(700_000); // bookkeeping after verification is irrelevant
        let activated = finish_thermal_plant_persistence(
            &mut calibration,
            &mut manual,
            &mut job,
            &mut memory,
            receipt.as_ref().copied().unwrap_or(store.clock.get()),
            receipt.is_ok(),
        );
        assert_eq!(activated, expected.is_ok());
        if !activated {
            assert_eq!(memory, previous);
        }
    }
}

#[test]
fn publication_at_deadline_never_starts_active_write() {
    let store = PublicationStore::new(660_005, "");
    store.clock.set(660_000);
    assert_eq!(
        run_publication(publish_marker_before_deadline(
            Some(660_000),
            || store.clock.get(),
            async |status| store.write_verified(status).await,
        )),
        Err(TimedPublicationError::Deadline)
    );
    assert!(store.writes.borrow().is_empty());
}

#[test]
fn pending_mode_off_still_fails_on_sensor_or_fan_safety() {
    for (sensor_fault, fan_enabled, condition) in [
        (true, false, "rtd_sensor_healthy"),
        (false, true, "passive_fan_off"),
    ] {
        let (mut calibration, mut manual, job, mut memory) = full_range_pending_job();
        calibration.mode = CalibrationMode::Off;
        let previous = memory.clone();
        let mut workspace = CalibrationThermalPlantWorkspace {
            job: Some(job),
            ..Default::default()
        };
        assert!(enforce_thermal_plant_safety(
            &mut calibration,
            &mut manual,
            &mut workspace,
            sensor_fault,
            fan_enabled,
        ));
        let job = workspace.job.as_mut().unwrap();
        let frozen_ticks = job.elapsed_ticks;
        let samples = job.sample_count;
        assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
        assert_eq!(job.completion.failure_condition, Some(condition));
        assert!(!finish_thermal_plant_persistence(
            &mut calibration,
            &mut manual,
            job,
            &mut memory,
            650_000,
            true,
        ));
        assert_eq!(memory, previous);
        assert_eq!(calibration.job.status, CalibrationJobStatus::Failed);
        assert_eq!(job.elapsed_ticks, frozen_ticks);
        assert_eq!(job.sample_count, samples);
        assert_eq!(job.completion.failure_condition, Some(condition));
    }
}

#[test]
fn pending_inputs_stay_locked_through_all_transport_write_guards() {
    let (mut calibration, mut manual, _, _) = full_range_pending_job();
    calibration.mode = CalibrationMode::Off;
    let original = calibration;
    let original_manual = manual;
    let response = calibration_config_job_lock_response(
        calibration,
        heapless::String::try_from("pending-input-lock").unwrap(),
    )
    .expect("calibration_config must remain locked while saving");
    let UsbFrame::Response {
        ok,
        error: Some(error),
        ..
    } = response
    else {
        panic!("expected a transport rejection");
    };
    assert!(!ok);
    assert_eq!(
        error.code.as_str(),
        ManualPpsError::CalibrationInProgress.code()
    );
    for request in [
        serde_json::json!({"manualPpsEnabled": true, "manualPpsMv": 20000}),
        serde_json::json!({"manualPpsEnabled": false}),
        serde_json::json!({"heaterEnabled": true}),
        serde_json::json!({"calibration": {"mode": "heater_curve"}}),
    ] {
        let command: RuntimeConfigCommand = serde_json::from_value(request).unwrap();
        let error = runtime_config_validation(&command, calibration).unwrap();
        assert_eq!(
            error.code.as_str(),
            ManualPpsError::CalibrationInProgress.code()
        );
        if command.manual_pps_enabled.is_some() {
            assert_eq!(
                apply_manual_pps_config(&command, calibration, &mut manual),
                Err(ManualPpsError::CalibrationInProgress)
            );
        }
    }
    let command: CalibrationControlCommand = serde_json::from_value(
        serde_json::json!({"mode": "heater_curve", "ppsEnabled": true, "ppsMv": 20000}),
    )
    .unwrap();
    assert_eq!(
        apply_calibration_control_config(&command, &mut calibration, &mut manual),
        Err(ManualPpsError::CalibrationInProgress)
    );
    assert_eq!(calibration, original);
    assert_eq!(manual, original_manual);
    for status in [
        CalibrationJobStatus::Idle,
        CalibrationJobStatus::Completed,
        CalibrationJobStatus::Failed,
        CalibrationJobStatus::Canceled,
    ] {
        calibration.job.status = status;
        assert!(
            calibration_config_job_lock_response(
                calibration,
                heapless::String::try_from("terminal").unwrap(),
            )
            .is_none()
        );
    }
    calibration.job.status = CalibrationJobStatus::Running;
    calibration.job.kind = Some(CalibrationJobKind::VinAdc);
    assert!(!thermal_plant_calibration_job_running(calibration));
}
