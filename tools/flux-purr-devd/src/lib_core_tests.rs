use super::*;
use std::process::ExitStatus;
use tempfile::tempdir;

#[test]
fn local_control_endpoint_rejects_network_transports() {
    assert!(validate_local_control_endpoint("http://127.0.0.1:30080").is_err());
    assert!(validate_local_control_endpoint("tcp:127.0.0.1:30080").is_err());
    assert!(validate_local_control_endpoint("127.0.0.1:30080").is_err());
    #[cfg(unix)]
    assert!(validate_local_control_endpoint("flux-purr-devd.sock").is_err());
    #[cfg(windows)]
    assert!(validate_local_control_endpoint(r"\\.\pipe\flux-purr-devd").is_ok());
    #[cfg(unix)]
    assert!(validate_local_control_endpoint("/tmp/flux-purr-devd.sock").is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn local_control_managed_socket_round_trip_returns_one_response() {
    let directory = tempdir().unwrap();
    let endpoint = directory.path().join("control.sock");
    let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
    let server = tokio::spawn(serve_local_control(listener, AppState::test()));

    let response = local_control_request(endpoint.to_str().unwrap(), "GET", "/health", None)
        .await
        .unwrap();
    assert_eq!(response.version, LOCAL_CONTROL_PROTOCOL_VERSION);
    assert_eq!(response.status, 200);
    assert_eq!(response.body["name"], "flux-purr-devd");

    server.abort();
}

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[cfg(unix)]
fn test_flash_artifact() -> FirmwareArtifact {
    FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec![],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "elf".to_string(),
            path: "firmware.elf".to_string(),
            sha256: "sha256:test".to_string(),
            size: 4,
            flash_address: None,
        }],
    }
}

#[test]
fn dev_cors_origin_guard_allows_only_local_development_origins() {
    for origin in [
        "http://localhost:43690",
        "http://127.0.0.1:43690",
        "http://[::1]:43690",
        "https://localhost:43690",
    ] {
        assert!(
            is_allowed_dev_origin(&origin.parse::<HeaderValue>().unwrap()),
            "{origin} should be allowed"
        );
    }

    for origin in [
        "https://example.com",
        "http://localhost.evil.test:43690",
        "http://127.0.0.1.evil.test:43690",
        "file://localhost/console.html",
    ] {
        assert!(
            !is_allowed_dev_origin(&origin.parse::<HeaderValue>().unwrap()),
            "{origin} should be rejected"
        );
    }
}

#[test]
fn lease_conflict_and_expiry_are_enforced() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let conflict = state.lease_device("mock-fp-lab-01").unwrap_err();
    assert_eq!(conflict.status, StatusCode::CONFLICT);

    {
        let mut inner = state.lock().unwrap();
        inner.leases.get_mut(&lease.lease_id).unwrap().expires_at =
            Instant::now() - Duration::from_millis(1);
        inner.cleanup_leases();
    }

    assert!(state.lease_device("mock-fp-lab-01").is_ok());
}

#[tokio::test]
async fn release_lease_records_device_event() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();

    let response = delete_lease(State(state.clone()), AxumPath(lease.lease_id.clone()))
        .await
        .unwrap()
        .0;

    assert_eq!(response["released"], true);
    let inner = state.lock().unwrap();
    let device = inner.devices.get("mock-fp-lab-01").unwrap();
    assert!(device.events.iter().any(|event| {
        event.kind == "lease"
            && event.message == "lease released"
            && event.payload["leaseId"] == lease.lease_id
    }));
}

#[test]
fn device_event_backlog_replays_existing_bounded_events() {
    let state = AppState::test();
    for index in 0..(DEVICE_EVENT_REPLAY_LIMIT + 7) {
        state.emit(event(
            "mock-fp-lab-01",
            "lease",
            "lease created",
            json!({ "leaseId": format!("lease-{index}") }),
        ));
    }
    state.emit(event(
        "other-device",
        "lease",
        "lease created",
        json!({ "leaseId": "lease-other" }),
    ));

    let backlog = device_event_backlog(&state, "mock-fp-lab-01").unwrap();

    assert_eq!(backlog.len(), DEVICE_EVENT_REPLAY_LIMIT);
    assert_eq!(backlog[0].kind, "lease");
    assert_eq!(backlog[0].payload["leaseId"], "lease-7");
    assert_eq!(
        backlog[DEVICE_EVENT_REPLAY_LIMIT - 1].payload["leaseId"],
        format!("lease-{}", DEVICE_EVENT_REPLAY_LIMIT + 6)
    );
}

#[tokio::test]
async fn list_devices_trims_inline_event_backlog_for_polling_clients() {
    let state = AppState::test();
    {
        let mut inner = state.lock().unwrap();
        let device = inner.devices.get_mut("mock-fp-lab-01").unwrap();
        for index in 0..(DEVICE_LIST_EVENT_LIMIT + 7) {
            push_bounded(
                &mut device.events,
                event(
                    "mock-fp-lab-01",
                    "transport",
                    "transport frame",
                    json!({
                        "direction": "rx",
                        "transport": "usb_jsonl",
                        "frameType": "response",
                        "requestId": format!("req-{index}"),
                        "frame": {
                            "type": "response",
                            "requestId": format!("req-{index}"),
                            "ok": true,
                            "result": {
                                "calibration": {
                                    "active": {
                                        "vinAdc": [
                                            {
                                                "expectedMv": 417,
                                                "observedMv": 279
                                            }
                                        ]
                                    }
                                }
                            }
                        },
                    }),
                ),
                DEFAULT_EVENT_LIMIT,
            );
        }
    }

    let response = list_devices(State(state)).await.unwrap().0;
    let devices = response["devices"].as_array().unwrap();
    let device = devices
        .iter()
        .find(|device| device["id"] == "mock-fp-lab-01")
        .unwrap();
    let events = device["events"].as_array().unwrap();

    assert_eq!(events.len(), DEVICE_LIST_EVENT_LIMIT);
    assert!(device.get("calibration").is_none());
    assert!(device.get("heaterCurve").is_none());
    assert!(device.get("logs").is_none());
    assert!(device.get("trace").is_none());
    assert_eq!(events[0]["payload"]["requestId"], "req-7");
    assert!(events[0]["payload"].get("frame").is_none());
    assert_eq!(
        events[DEVICE_LIST_EVENT_LIMIT - 1]["payload"]["requestId"],
        format!("req-{}", DEVICE_LIST_EVENT_LIMIT + 6)
    );
}

#[test]
fn bounded_queue_rotates_oldest_entries() {
    let mut values = VecDeque::new();
    push_bounded(&mut values, 1, 2);
    push_bounded(&mut values, 2, 2);
    push_bounded(&mut values, 3, 2);
    assert_eq!(values.into_iter().collect::<Vec<_>>(), vec![2, 3]);
}

#[test]
fn event_ids_are_unique_inside_same_millisecond_window() {
    let first = event(
        "mock-fp-lab-01",
        "runtime",
        "runtime config applied",
        json!({}),
    );
    let second = event(
        "mock-fp-lab-01",
        "runtime",
        "runtime config applied",
        json!({}),
    );

    assert_ne!(first.id, second.id);
}

#[test]
fn transport_events_preserve_frame_data_and_redact_passwords() {
    let state = AppState::test();
    record_transport_event(
        &state,
        "mock-fp-lab-01",
        "tx",
        "usb_jsonl",
        "req-1",
        r#"{"type":"wifi_config","requestId":"req-1","ssid":"FluxPurr-Lab","password":"secret-pass","result":{"wifi":{"psk":"nested-secret"}}}"#,
    );

    let inner = state.lock().unwrap();
    let device = inner.devices.get("mock-fp-lab-01").unwrap();
    let transport_event = device
        .events
        .iter()
        .find(|event| event.kind == "transport")
        .unwrap();

    assert_eq!(transport_event.payload["direction"], "tx");
    assert_eq!(transport_event.payload["frame"]["ssid"], "FluxPurr-Lab");
    assert_eq!(transport_event.payload["frame"]["password"], "<redacted>");
    assert_eq!(
        transport_event.payload["frame"]["result"]["wifi"]["psk"],
        "<redacted>"
    );
    assert!(
        !serde_json::to_string(&transport_event.payload)
            .unwrap()
            .contains("secret-pass")
    );
    assert!(
        !serde_json::to_string(&transport_event.payload)
            .unwrap()
            .contains("nested-secret")
    );
}

#[test]
fn transport_events_redact_lan_pairing_codes() {
    let state = AppState::test();
    record_transport_event(
        &state,
        "mock-fp-lab-01",
        "rx",
        "usb_jsonl",
        "pairing-code-1",
        r#"{"type":"response","requestId":"pairing-code-1","ok":true,"result":{"lan_pairing_code":{"active":true,"code":"4827"}}}"#,
    );

    let inner = state.lock().unwrap();
    let event = inner.devices["mock-fp-lab-01"]
        .events
        .iter()
        .find(|event| event.kind == "transport")
        .unwrap();
    assert_eq!(
        event.payload["frame"]["result"]["lan_pairing_code"]["code"],
        "<redacted>"
    );
    assert!(
        !serde_json::to_string(&event.payload)
            .unwrap()
            .contains("4827")
    );
}

#[test]
fn lan_pairing_code_requires_a_consistent_active_state() {
    assert!(
        validate_lan_pairing_code(LanPairingCode {
            active: true,
            code: Some("4827".to_string()),
        })
        .is_ok()
    );
    assert!(
        validate_lan_pairing_code(LanPairingCode {
            active: false,
            code: None,
        })
        .is_ok()
    );
    assert!(
        validate_lan_pairing_code(LanPairingCode {
            active: true,
            code: None,
        })
        .is_err()
    );
    assert!(
        validate_lan_pairing_code(LanPairingCode {
            active: false,
            code: Some("abcd".to_string()),
        })
        .is_err()
    );
}

#[test]
fn serial_scan_ignores_missing_authorized_port() {
    let dir = tempdir().unwrap();
    let missing_port = dir.path().join("missing-usbmodem");

    let devices = scan_serial_devices(Some(&missing_port));

    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].connection, ConnectionState::Error);
    assert_eq!(devices[0].network.state, NetworkState::Error);
    assert!(
        devices[0]
            .network
            .last_error
            .as_deref()
            .is_some_and(|message| {
                message.starts_with(&format!(
                    "Authorized serial port {} is missing.",
                    missing_port.display()
                ))
            })
    );
    assert_eq!(devices[0].events.len(), 1);
    assert_eq!(
        devices[0].events[0].message,
        "authorized serial port missing"
    );
    assert_eq!(
        devices[0].events[0].payload["code"],
        "authorized_port_missing"
    );
}

#[test]
fn serial_scan_without_fixed_target_lists_all_espressif_candidates() {
    let ports = vec![
        serialport::SerialPortInfo {
            port_name: "/dev/cu.usbmodem-a".to_string(),
            port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x303a,
                pid: 0x1001,
                serial_number: Some("candidate-a".to_string()),
                manufacturer: Some("Espressif".to_string()),
                product: Some("USB JTAG/serial debug unit".to_string()),
            }),
        },
        serialport::SerialPortInfo {
            port_name: "/dev/cu.usbmodem-b".to_string(),
            port_type: serialport::SerialPortType::UsbPort(serialport::UsbPortInfo {
                vid: 0x303a,
                pid: 0x1001,
                serial_number: Some("candidate-b".to_string()),
                manufacturer: Some("Espressif".to_string()),
                product: Some("USB JTAG/serial debug unit".to_string()),
            }),
        },
        serialport::SerialPortInfo {
            port_name: "/dev/cu.other".to_string(),
            port_type: serialport::SerialPortType::Unknown,
        },
    ];

    let devices = scan_serial_devices_from_available(None, &ports);

    assert_eq!(devices.len(), 2);
    assert_eq!(devices[0].port_path.as_deref(), Some("/dev/cu.usbmodem-a"));
    assert_eq!(devices[1].port_path.as_deref(), Some("/dev/cu.usbmodem-b"));
    assert!(
        devices
            .iter()
            .all(|device| device.identity.device_id.is_empty())
    );
    assert!(
        devices
            .iter()
            .all(|device| device.identity.hostname.is_empty())
    );
}

#[test]
fn native_serial_devices_advertise_devd_flash_capabilities() {
    let device = serial_device_record("/dev/cu.usbmodem-test", None);

    assert_eq!(device.transport, DeviceTransport::NativeSerial);
    assert_eq!(device.connection, ConnectionState::Disconnected);
    assert_eq!(device.identity.device_id, "");
    assert_eq!(device.identity.hostname, "");
    assert_eq!(device.identity.build_id, "native-serial-placeholder");
    assert_eq!(device.identity.board, "unknown");
    assert_eq!(device.status.current_temp_c, -1.0);
    assert!(!device.status.heater_enabled);
    assert_eq!(device.status.pd_contract_mv, 0);
    assert_eq!(device.network.state, NetworkState::Idle);
    assert_eq!(device.network.ssid, None);
    assert!(
        device
            .identity
            .capabilities
            .contains(&"firmware_check".to_string())
    );
    assert!(device.identity.capabilities.contains(&"flash".to_string()));
}

#[test]
fn native_serial_placeholder_does_not_reuse_mock_hot_state() {
    let device = serial_device_record("/dev/cu.usbmodem-test", None);

    assert_ne!(device.identity.build_id, "devd-mock");
    assert_ne!(device.status.current_temp_c, 183.6);
    assert_ne!(device.network.ssid.as_deref(), Some("FluxPurr-Lab"));
    assert_eq!(device.status.mode, "idle");
    assert_eq!(device.status.network.state, NetworkState::Idle);
}

#[tokio::test]
async fn create_lease_refreshes_authorized_serial_device_before_lookup() {
    let dir = tempdir().unwrap();
    let port_path = dir.path().join("authorized-usbmodem");
    fs::write(&port_path, b"placeholder").unwrap();
    let config = AppConfig {
        serial_port: Some(port_path.clone()),
        ..AppConfig::default()
    };
    let state = AppState::new(config);
    let device = serial_device_record(port_path.to_str().unwrap(), None);

    let lease = create_lease(State(state.clone()), AxumPath(device.id.clone()))
        .await
        .unwrap()
        .0;

    assert_eq!(lease.device_id, device.id);
    let state_lock = state.lock().unwrap();
    assert!(state_lock.devices.contains_key(&lease.device_id));
}

#[test]
fn serial_refresh_removes_stale_native_devices_and_leases() {
    let mut state = DevdState::default();
    state.seed_mock_device();
    let mut serial_device = DeviceRecord::mock("serial-stale", DeviceTransport::NativeSerial);
    serial_device.port_path = Some("/dev/tty.Bluetooth-Incoming-Port".to_string());
    state
        .devices
        .insert(serial_device.id.clone(), serial_device.clone());
    state.leases.insert(
        "lease-stale".to_string(),
        WebLease {
            lease_id: "lease-stale".to_string(),
            device_id: serial_device.id,
            expires_at: Instant::now() + Duration::from_secs(1),
            ttl_ms: DEFAULT_LEASE_TTL_MS,
        },
    );

    refresh_serial_devices(&mut state, Vec::new());

    assert!(state.devices.contains_key("mock-fp-lab-01"));
    assert!(!state.devices.contains_key("serial-stale"));
    assert!(state.leases.is_empty());
}

#[test]
fn serial_refresh_preserves_native_error_diagnostics() {
    let mut state = DevdState::default();
    let mut existing = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
    existing.port_path = Some("/dev/cu.usbmodem-test".to_string());
    existing.connection = ConnectionState::Error;
    existing.network.state = NetworkState::Timeout;
    existing.network.last_error = Some("Timed out waiting for USB response.".to_string());
    existing.events.push_back(event(
        "serial-known",
        "serial",
        "native serial RPC failed",
        json!({ "code": "usb_response_timeout" }),
    ));
    state.devices.insert(existing.id.clone(), existing);

    let mut refreshed = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
    refreshed.display_name = "USB JTAG/serial debug unit".to_string();
    refreshed.port_path = Some("/dev/cu.usbmodem-test".to_string());
    refreshed.connection = ConnectionState::Disconnected;

    refresh_serial_devices(&mut state, vec![refreshed]);

    let device = state.devices.get("serial-known").unwrap();
    assert_eq!(device.display_name, "USB JTAG/serial debug unit");
    assert_eq!(device.connection, ConnectionState::Error);
    assert_eq!(device.network.state, NetworkState::Timeout);
    assert_eq!(
        device.network.last_error.as_deref(),
        Some("Timed out waiting for USB response.")
    );
    assert_eq!(device.events.len(), 1);
}

#[test]
fn serial_bridge_error_preserves_wifi_state_and_records_event() {
    let state = AppState::test();
    let mut serial_device = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
    serial_device.port_path = Some("/dev/cu.usbmodem-test".to_string());
    serial_device.network.state = NetworkState::Connected;
    serial_device.network.ssid = Some("FluxPurr-Lab".to_string());
    serial_device.network.wifi_rssi = Some(-47);
    serial_device.status.network = serial_device.network.clone();
    {
        let mut inner = state.lock().unwrap();
        inner
            .devices
            .insert(serial_device.id.clone(), serial_device);
    }

    let error = HttpError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "usb_response_timeout",
        "Timed out waiting for a matching USB JSONL response.",
        true,
    );

    record_serial_bridge_error(&state, "serial-known", "identity", &error);

    let inner = state.lock().unwrap();
    let device = inner.devices.get("serial-known").unwrap();
    assert_eq!(device.connection, ConnectionState::Error);
    assert_eq!(device.network.state, NetworkState::Connected);
    assert_eq!(device.network.wifi_rssi, Some(-47));
    assert_eq!(device.network.last_error, None);
    assert_eq!(device.status.network.state, NetworkState::Connected);
    assert_eq!(device.status.network.wifi_rssi, Some(-47));
    assert_eq!(device.status.network.last_error, None);
    assert_eq!(device.events.len(), 1);
    assert_eq!(device.events[0].kind, "serial");
    assert_eq!(device.events[0].payload["stage"], "identity");
    assert_eq!(device.events[0].payload["code"], "usb_response_timeout");
}

#[test]
fn serial_monitor_log_line_records_serial_event_without_overwriting_errors() {
    let state = AppState::test();
    let mut serial_device = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
    serial_device.port_path = Some("/dev/cu.usbmodem-test".to_string());
    {
        let mut inner = state.lock().unwrap();
        inner
            .devices
            .insert(serial_device.id.clone(), serial_device);
    }

    emit_serial_log_line(
        &state.inner,
        &state.events,
        "serial-known",
        b"INFO heater runtime disabled by safety gate",
    );

    let inner = state.lock().unwrap();
    let device = inner.devices.get("serial-known").unwrap();
    assert_eq!(device.events.len(), 1);
    assert_eq!(device.events[0].kind, "serial");
    assert_eq!(device.events[0].message, "native serial monitor line");
    assert_eq!(device.events[0].payload["code"], "firmware_log");
    assert_eq!(
        device.events[0].payload["line"],
        "INFO heater runtime disabled by safety gate"
    );
}

#[test]
fn serial_monitor_persistence_fault_emits_structured_event_without_raw_data() {
    let state = AppState::test();
    let mut serial_device = DeviceRecord::mock("serial-known", DeviceTransport::NativeSerial);
    serial_device.port_path = Some("/dev/cu.usbmodem-test".to_string());
    {
        let mut inner = state.lock().unwrap();
        inner
            .devices
            .insert(serial_device.id.clone(), serial_device);
    }

    emit_serial_log_line(
            &state.inner,
            &state.events,
            "serial-known",
            b"PERSISTENCE_COMMIT_ATTEMPT_FAILED code=eeprom_write_failed phase=write attempt=2 sequence=3221 slot=A",
        );

    let inner = state.lock().unwrap();
    let device = inner.devices.get("serial-known").unwrap();
    assert_eq!(device.events.len(), 2);
    assert_eq!(device.events[0].kind, "serial");
    assert_eq!(device.events[1].kind, "persistence_fault");
    assert_eq!(device.events[1].payload["code"], "eeprom_write_failed");
    assert_eq!(device.events[1].payload["phase"], "write");
    assert_eq!(device.events[1].payload["attempt"], 2);
    assert_eq!(device.events[1].payload["sequence"], 3221);
    assert_eq!(device.events[1].payload["slot"], "A");
    assert!(device.events[1].payload.get("line").is_none());
}

#[test]
fn serial_open_failed_preserves_missing_authorized_port_diagnostic() {
    let state = AppState::test();
    let device = missing_serial_device_record("/dev/cu.usbmodem-test", &[]);
    {
        let mut inner = state.lock().unwrap();
        inner.devices.insert(device.id.clone(), device);
    }

    let error = HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_open_failed",
        "Failed to open serial port: No such file or directory",
        true,
    );

    record_serial_bridge_error(&state, "serial-_dev_cu.usbmodem-test", "identity", &error);

    let inner = state.lock().unwrap();
    let device = inner.devices.get("serial-_dev_cu.usbmodem-test").unwrap();
    assert_eq!(device.connection, ConnectionState::Error);
    assert_eq!(device.network.state, NetworkState::Error);
    assert!(device.network.last_error.as_deref().is_some_and(|message| {
        message.starts_with("Authorized serial port /dev/cu.usbmodem-test is missing.")
    }));
    assert_eq!(
        device.events.back().unwrap().message,
        "native serial RPC failed"
    );
}

#[test]
fn artifact_verify_checks_hash_and_size() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("firmware.bin");
    fs::write(&file_path, b"flux-purr").unwrap();
    let digest = format!("sha256:{:x}", Sha256::digest(b"flux-purr"));
    let artifact = FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "debug".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "app".to_string(),
            path: "firmware.bin".to_string(),
            sha256: digest.clone(),
            size: 9,
            flash_address: Some(0x10000),
        }],
    };

    let result = verify_artifact(&artifact, Some(dir.path())).unwrap();
    assert!(result.verified);
    assert_eq!(result.files[0].sha256, digest);
}

#[test]
fn artifact_verify_reports_hash_mismatch() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("firmware.bin");
    fs::write(&file_path, b"flux-purr").unwrap();
    let artifact = FirmwareArtifact {
        artifact_id: "bad-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "debug".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "app".to_string(),
            path: "firmware.bin".to_string(),
            sha256: "sha256:bad".to_string(),
            size: 9,
            flash_address: Some(0x10000),
        }],
    };

    let result = verify_artifact(&artifact, Some(dir.path())).unwrap();
    assert!(!result.verified);
    assert!(!result.files[0].ok);
}

#[test]
fn artifact_verify_rejects_paths_outside_artifact_root() {
    let dir = tempdir().unwrap();
    let artifact = FirmwareArtifact {
        artifact_id: "escaped-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "debug".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "app".to_string(),
            path: "../firmware.bin".to_string(),
            sha256: "sha256:bad".to_string(),
            size: 9,
            flash_address: Some(0x10000),
        }],
    };
    let parent_escape = verify_artifact(&artifact, Some(dir.path())).unwrap_err();
    assert_eq!(parent_escape.kind(), io::ErrorKind::PermissionDenied);

    let mut absolute_artifact = artifact;
    absolute_artifact.files[0].path = "/etc/hosts".to_string();
    let absolute_escape = verify_artifact(&absolute_artifact, Some(dir.path())).unwrap_err();
    assert_eq!(absolute_escape.kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn artifact_verify_rejects_empty_file_list() {
    let artifact = FirmwareArtifact {
        artifact_id: "empty-artifact".to_string(),
        name: "Empty".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "debug".to_string(),
        features: Vec::new(),
        protocol: "flux-purr.usb.v1".to_string(),
        files: Vec::new(),
    };

    let result = verify_artifact(&artifact, None).unwrap();
    assert!(!result.verified);
}

#[test]
fn artifact_catalog_uses_only_the_canonical_firmware_target() {
    let dir = tempdir().unwrap();
    let canonical_path = dir
        .path()
        .join("firmware/target/xtensa-esp32s3-none-elf/release");
    fs::create_dir_all(&canonical_path).unwrap();
    fs::write(canonical_path.join("flux-purr"), b"current-firmware-image").unwrap();

    let stale_path = dir.path().join("target/xtensa-esp32s3-none-elf/release");
    fs::create_dir_all(&stale_path).unwrap();
    fs::write(stale_path.join("flux-purr"), b"stale-firmware-image").unwrap();

    let artifacts = discover_firmware_artifacts(Some(dir.path())).unwrap();

    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].artifact_id, "local-esp32s3-release");
    assert_eq!(artifacts[0].target_chip, "esp32s3");
    assert_eq!(
        artifacts[0].profile,
        "release + web_serial + net_http + buzzer-test"
    );
    assert_eq!(
        artifacts[0].features,
        ["web_serial", "net_http", "buzzer-test"]
    );
    assert_eq!(artifacts[0].files[0].kind, "elf");
    assert_eq!(
        artifacts[0].files[0].path,
        "firmware/target/xtensa-esp32s3-none-elf/release/flux-purr"
    );
    assert_eq!(artifacts[0].files[0].size, 22);
    assert_eq!(artifacts[0].files[0].flash_address, None);
    assert!(artifacts[0].files[0].sha256.starts_with("sha256:"));
}

#[test]
fn artifact_catalog_exposes_observer_firmware_separately() {
    let dir = tempdir().unwrap();
    let observer_path = dir
        .path()
        .join("firmware/target/buzzer-observe/xtensa-esp32s3-none-elf/release");
    fs::create_dir_all(&observer_path).unwrap();
    fs::write(observer_path.join("flux-purr"), b"observer-firmware-image").unwrap();

    let artifacts = discover_firmware_artifacts(Some(dir.path())).unwrap();

    assert_eq!(artifacts.len(), 1);
    assert_eq!(
        artifacts[0].artifact_id,
        "local-esp32s3-release-buzzer-observe"
    );
    assert_eq!(
        artifacts[0].features,
        ["web_serial", "net_http", "buzzer-test", "buzzer-observe"]
    );
    assert_eq!(
        artifacts[0].files[0].path,
        "firmware/target/buzzer-observe/xtensa-esp32s3-none-elf/release/flux-purr"
    );
}

#[test]
fn usb_runtime_wire_serializes_thermal_profile_mode() {
    let json = encode_usb_runtime_mode_for_test(&"100w".to_string());

    assert!(json.contains(r#""thermalProfileMode":"100w""#));
}

#[test]
fn usb_runtime_wire_serializes_fault_attention_acknowledgement() {
    let json = serde_json::to_value(UsbRuntimeConfigWire {
        frame_type: "runtime_config",
        request_id: "attention-test",
        target_temp_c: Some(140),
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: Some(true),
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: Some(false),
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        fault_attention_acknowledged: Some(true),
        calibration: None,
        thermal_profile_mode: None,
        thermal_control_profile: None,
    })
    .unwrap();

    assert_eq!(json["faultAttentionAcknowledged"], true);
}

#[test]
fn usb_buzzer_test_wire_only_serializes_fixed_cue_or_scenario_requests() {
    let wire = UsbBuzzerTestWire {
        frame_type: "buzzer_test",
        request_id: "buzzer-1",
        op: BuzzerTestOp::Run,
        buzzer_cue: None,
        buzzer_scenario: Some(BuzzerTestScenario::ActiveCoolingRetrigger),
        repeat: false,
    };
    let json = serde_json::to_value(wire).unwrap();

    assert_eq!(json["type"], "buzzer_test");
    assert_eq!(json["op"], "run");
    assert_eq!(json["buzzerScenario"], "active_cooling_retrigger");
    assert!(json.get("buzzerCue").is_none());
    assert!(json.get("frequencyHz").is_none());
    assert!(json.get("dutyPercent").is_none());
}

#[test]
fn buzzer_test_request_validation_accepts_only_the_operation_shape() {
    let valid = BuzzerTestRequest {
        lease_id: "lease-1".to_string(),
        op: BuzzerTestOp::Trigger,
        cue: Some(BuzzerTestCue::UiInput),
        scenario: None,
        repeat: false,
    };
    assert!(validate_buzzer_test_request(&valid).is_ok());

    let invalid = BuzzerTestRequest {
        scenario: Some(BuzzerTestScenario::FeedbackCoalesce),
        ..valid
    };
    let error = validate_buzzer_test_request(&invalid).unwrap_err();
    assert_eq!(error.error.code, "invalid_buzzer_test_command");
}

#[tokio::test]
async fn buzzer_test_rejects_firmware_without_the_development_capability() {
    let state = AppState::test();
    let device_id = "native-buzzer-test-test";
    {
        let mut state_lock = state.lock().unwrap();
        state_lock.devices.insert(
            device_id.to_string(),
            DeviceRecord::native_serial_placeholder(
                device_id,
                "Native buzzer test target".to_string(),
                "/dev/null".to_string(),
            ),
        );
    }
    let lease = state.lease_device(device_id).unwrap();

    let error = configure_buzzer_test(
        State(state),
        AxumPath(device_id.to_string()),
        Json(BuzzerTestRequest {
            lease_id: lease.lease_id,
            op: BuzzerTestOp::Status,
            cue: None,
            scenario: None,
            repeat: false,
        }),
    )
    .await
    .unwrap_err();

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.error.code, "buzzer_test_unavailable");
}

#[test]
fn real_flash_args_flash_elf_and_hard_reset() {
    let artifact = FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "elf".to_string(),
            path: "firmware.elf".to_string(),
            sha256: "sha256:test".to_string(),
            size: 9,
            flash_address: None,
        }],
    };

    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("firmware")).unwrap();
    std::fs::write(
        dir.path().join("firmware/partitions.csv"),
        "legacy_config,data,0x06,0x210000,0x2000",
    )
    .unwrap();
    let commands = build_espflash_args_with_reset_mode(
        &artifact,
        Some(dir.path()),
        "/dev/cu.usbmodem21221401",
        "usb-reset",
    )
    .unwrap();
    assert_eq!(commands.len(), 1);
    let args = &commands[0];

    assert_eq!(args[0], "flash");
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--port", "/dev/cu.usbmodem21221401"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--before", "usb-reset"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--after", "hard-reset"])
    );
    assert!(args.iter().any(|argument| argument == "--no-stub"));
    assert!(!args.contains(&"-S".to_string()));
    assert!(args.iter().any(|arg| arg.ends_with("firmware.elf")));
    assert!(args.windows(2).any(|pair| {
        pair[0] == "--partition-table" && pair[1].ends_with("firmware/partitions.csv")
    }));
    assert!(!args.contains(&"65536".to_string()));
}

#[test]
fn real_flash_args_write_raw_app_bin_with_partition_table_and_reset() {
    let artifact = FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "app".to_string(),
            path: "firmware.bin".to_string(),
            sha256: "sha256:test".to_string(),
            size: 9,
            flash_address: Some(DEFAULT_APP_FLASH_ADDRESS),
        }],
    };

    let dir = tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("firmware")).unwrap();
    std::fs::write(
        dir.path().join("firmware/partitions.csv"),
        "legacy_config,data,0x06,0x210000,0x2000",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("firmware/partitions.bin"),
        b"partition-table",
    )
    .unwrap();
    let commands = build_espflash_args_with_reset_mode(
        &artifact,
        Some(dir.path()),
        "/dev/cu.usbmodem21221401",
        "usb-reset",
    )
    .unwrap();

    assert_eq!(commands.len(), 3);
    assert_eq!(commands[0][0], "write-bin");
    assert!(commands[0].windows(2).any(|pair| {
        pair == [
            DEFAULT_PARTITION_TABLE_FLASH_ADDRESS.to_string(),
            dir.path()
                .join("firmware/partitions.bin")
                .to_string_lossy()
                .into_owned(),
        ]
    }));
    assert_eq!(commands[1][0], "write-bin");
    assert!(commands[1].windows(2).any(|pair| {
        pair == [
            DEFAULT_APP_FLASH_ADDRESS.to_string(),
            dir.path()
                .join("firmware.bin")
                .to_string_lossy()
                .into_owned(),
        ]
    }));
    assert_eq!(commands[2][0], "reset");
    assert!(
        commands[2]
            .windows(2)
            .any(|pair| pair == ["--before", "usb-reset"])
    );
}

#[test]
fn non_native_serial_ports_keep_the_default_reset_mode() {
    let artifact = FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec![],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![],
    };
    assert_eq!(
        espflash_reset_modes(&artifact, "/dev/cu.usbserial-1410"),
        ["default-reset"]
    );
}

#[test]
fn usbmodem_flash_retries_usb_reset_before_default_reset_without_manual_boot_mode() {
    let artifact = FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec![],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![],
    };

    assert_eq!(
        espflash_reset_modes(&artifact, "/dev/cu.usbmodem2111401"),
        ["usb-reset", "usb-reset", "default-reset"]
    );
}

#[test]
fn broken_pipe_is_an_espflash_connection_failure() {
    assert!(espflash_connection_failure_text(
        "IO error while using serial port: Broken pipe"
    ));
    assert!(espflash_connection_failure_text(
        "Error while connecting to device: No such file or directory (os error 2)"
    ));
    assert!(!espflash_connection_failure_text(
        "Image verification failed after flash"
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn usbmodem_connection_failure_retries_usb_reset_before_default_reset() {
    let dir = tempdir().unwrap();
    let program = dir.path().join("retrying-espflash.sh");
    let attempts = dir.path().join("attempts.log");
    std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$1\" >> \"{}\"\nattempts=$(wc -l < \"{}\")\nif [ \"$1\" = \"usb-reset\" ] && [ \"$attempts\" -eq 2 ]; then\n  exit 0\nfi\nprintf '%s\\n' 'Broken pipe' >&2\nexit 1\n",
                attempts.display(),
                attempts.display(),
            ),
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).unwrap();

    run_espflash_with_reset_fallback_with_program(
        &program,
        &test_flash_artifact(),
        "/dev/cu.usbmodem2111401",
        |before_reset| Ok(vec![vec![before_reset.to_string()]]),
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(attempts).unwrap(),
        "usb-reset\nusb-reset\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bundle_flash_retries_a_transient_connection_failure() {
    let dir = tempdir().unwrap();
    let program = dir.path().join("retrying-bundle-espflash.sh");
    let attempts = dir.path().join("attempts.log");
    std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"{}\"\nattempts=$(wc -l < \"{}\")\nif [ \"$attempts\" -eq 1 ]; then\n  printf '%s\\n' 'Broken pipe' >&2\n  exit 1\nfi\n",
                attempts.display(),
                attempts.display(),
            ),
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).unwrap();

    require_bundle_espflash_success(
        &program,
        &[
            "write-bin".to_string(),
            "--before".to_string(),
            "no-reset".to_string(),
        ],
        "/dev/cu.usbmodem2111401",
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(attempts).unwrap(),
        "write-bin --before no-reset\nwrite-bin --before usb-reset\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn bundle_recovery_retries_initial_usb_reset_before_default_reset() {
    let dir = tempdir().unwrap();
    let program = dir.path().join("retrying-recovery-espflash.sh");
    let attempts = dir.path().join("attempts.log");
    std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nset -eu\nprintf '%s\\n' \"$*\" >> \"{}\"\ncase \"$*\" in\n  *'--before default-reset'*) exit 0 ;;\nesac\nprintf '%s\\n' 'Broken pipe' >&2\nexit 1\n",
                attempts.display(),
            ),
        )
        .unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).unwrap();

    require_bundle_espflash_success(
        &program,
        &[
            "erase-flash".to_string(),
            "--before".to_string(),
            "usb-reset".to_string(),
        ],
        "/dev/cu.usbmodem2111401",
    )
    .await
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(attempts).unwrap(),
        "erase-flash --before usb-reset\nerase-flash --before usb-reset\nerase-flash --before default-reset\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn espflash_subprocess_timeout_is_reported_without_hanging_the_request() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    let program = dir.path().join("stuck-espflash");
    std::fs::write(&program, "#!/bin/sh\nsleep 5\n").unwrap();
    let mut permissions = std::fs::metadata(&program).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&program, permissions).unwrap();

    let started = Instant::now();
    let error = run_espflash_command_with_timeout(
        &program,
        &["read-flash".to_string()],
        Duration::from_millis(50),
    )
    .await
    .unwrap_err();

    assert_eq!(error.error.code, "flash_tool_timeout");
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn flash_serial_lock_wait_is_bounded() {
    let serial_rpc = Arc::new(tokio::sync::Mutex::new(()));
    let held = serial_rpc.clone().lock_owned().await;

    let started = Instant::now();
    let error = acquire_serial_rpc_with_timeout(serial_rpc, Duration::from_millis(25))
        .await
        .unwrap_err();

    assert_eq!(error.error.code, "serial_lock_timeout");
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(held);
}

#[test]
fn transient_usb_jtag_connection_errors_are_retryable() {
    assert!(espflash_connection_failure_text(
        "Error while connecting to device: No such device or address"
    ));
}

#[cfg(unix)]
#[test]
fn flash_end_error_is_recoverable_only_for_a_complete_flash_command() {
    let flash_args = vec!["flash".to_string()];
    let write_args = vec!["write-bin".to_string()];
    let output = Output {
        status: ExitStatus::from_raw(1 << 8),
        stdout: Vec::new(),
        stderr: b"Error: Error while running FlashEnd command".to_vec(),
    };

    assert!(espflash_flash_end_requires_reset(&flash_args, &output));
    assert!(!espflash_flash_end_requires_reset(&write_args, &output));
}

#[test]
fn flash_end_recovery_reset_is_explicit_and_uses_the_authorized_usb_port() {
    let artifact = FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec![],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![],
    };

    let args =
        build_espflash_reset_args(&artifact, "/dev/cu.usbmodem2111401", "usb-reset").unwrap();

    assert_eq!(args[0], "reset");
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--port", "/dev/cu.usbmodem2111401"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--before", "usb-reset"])
    );
    assert!(
        args.windows(2)
            .any(|pair| pair == ["--after", "hard-reset"])
    );
}

#[test]
fn bundle_retry_replaces_only_the_recoverable_no_reset_mode() {
    let command = vec![
        "write-bin".to_string(),
        "--before".to_string(),
        "no-reset".to_string(),
        "--after".to_string(),
        "no-reset".to_string(),
    ];

    assert_eq!(
        replace_espflash_before_reset(&command, "usb-reset"),
        Some(vec![
            "write-bin".to_string(),
            "--before".to_string(),
            "usb-reset".to_string(),
            "--after".to_string(),
            "no-reset".to_string(),
        ])
    );
}

#[tokio::test]
async fn runtime_endpoint_requires_valid_lease() {
    let state = AppState::test();
    let error = configure_runtime(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(RuntimeConfigRequest {
            lease_id: "missing-lease".to_string(),
            target_temp_c: Some(230),
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: None,
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: None,
        }),
    )
    .await
    .unwrap_err();

    assert_eq!(error.status, StatusCode::FORBIDDEN);
    assert_eq!(error.error.code, "lease_expired");
}

#[tokio::test]
async fn native_serial_reads_and_pairing_window_operations_require_an_active_lease() {
    let state = AppState::test();
    {
        let mut state_lock = state.lock().unwrap();
        state_lock.devices.insert(
            "native-test".to_string(),
            DeviceRecord::native_serial_placeholder(
                "native-test",
                "Native test target".to_string(),
                "/dev/null".to_string(),
            ),
        );
    }

    let pairing_error = get_lan_pairing_code(
        State(state.clone()),
        AxumPath("native-test".to_string()),
        Query(LeaseQuery { lease_id: None }),
    )
    .await
    .unwrap_err();
    assert_eq!(pairing_error.status, StatusCode::FORBIDDEN);
    assert_eq!(pairing_error.error.code, "lease_required");

    let pairing_window_error = open_lan_pairing_window(
        State(state.clone()),
        AxumPath("native-test".to_string()),
        Query(LeaseQuery { lease_id: None }),
    )
    .await
    .unwrap_err();
    assert_eq!(pairing_window_error.status, StatusCode::FORBIDDEN);
    assert_eq!(pairing_window_error.error.code, "lease_required");

    let status_error = device_status(
        State(state),
        AxumPath("native-test".to_string()),
        Query(LeaseQuery { lease_id: None }),
    )
    .await
    .unwrap_err();
    assert_eq!(status_error.status, StatusCode::FORBIDDEN);
    assert_eq!(status_error.error.code, "lease_required");
}

fn test_thermal_control_profile_point(target_temp_c: i16) -> ThermalControlProfilePoint {
    ThermalControlProfilePoint {
        target_temp_c,
        brake_distance_centi_c: 1_000,
        warmup_power_permille: 1_000,
        warmup_reenter_centi_c: 400,
        approach_power_permille: 500,
        approach_floor_power_permille: 300,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 300,
        hold_reheat_power_permille: 350,
        hold_entry_centi_c: 150,
        hold_exit_centi_c: 100,
        hold_on_centi_c: 20,
        hold_off_centi_c: 100,
        overshoot_cutoff_centi_c: 200,
        hold_kp_permille_per_c: 20,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 2,
        approach_lead_ticks: 2,
        hold_lead_ticks: 1,
    }
}

fn thermal_profile_runtime_request(
    lease_id: String,
    op: ThermalControlProfileOp,
    profile: Option<ThermalControlProfilePackage>,
) -> RuntimeConfigRequest {
    RuntimeConfigRequest {
        lease_id,
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        calibration: None,
        thermal_profile_mode: None,
        fault_attention_acknowledged: None,
        thermal_control_profile: Some(ThermalControlProfileRequest {
            op,
            bank: None,
            profile,
        }),
    }
}

fn thermal_preview_profile() -> ThermalControlProfilePackage {
    ThermalControlProfilePackage {
        settings: None,
        points: vec![
            Some(ThermalControlProfilePoint {
                target_temp_c: 100,
                brake_distance_centi_c: 700,
                warmup_power_permille: 320,
                warmup_reenter_centi_c: 0,
                approach_power_permille: 320,
                approach_floor_power_permille: 220,
                approach_damping_exponent_permille: 1_000,
                approach_tail_window_centi_c: 0,
                hold_power_permille: 220,
                hold_reheat_power_permille: 0,
                hold_entry_centi_c: 0,
                hold_exit_centi_c: 0,
                hold_on_centi_c: 0,
                hold_off_centi_c: 0,
                overshoot_cutoff_centi_c: 0,
                hold_kp_permille_per_c: 0,
                hold_ki_permille_per_c_tick: 0,
                hold_blend_ticks: 0,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
            }),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ],
    }
}

fn mock_runtime_request(
    lease_id: String,
    target_temp_c: Option<i16>,
    active_cooling_enabled: Option<bool>,
    heater_enabled: Option<bool>,
    manual_pps_enabled: Option<bool>,
    manual_pps_mv: Option<u16>,
    manual_pps_ma: Option<u16>,
) -> RuntimeConfigRequest {
    RuntimeConfigRequest {
        lease_id,
        target_temp_c,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled,
        manual_pps_enabled,
        manual_pps_mv,
        manual_pps_ma,
        calibration: None,
        thermal_profile_mode: None,
        fault_attention_acknowledged: None,
        thermal_control_profile: None,
    }
}

fn assert_wifi_runtime_events(state: &AppState) {
    let inner = state.lock().unwrap();
    let device = inner.devices.get("mock-fp-lab-01").unwrap();
    let wifi_event = device
        .events
        .iter()
        .find(|event| event.kind == "wifi" && event.message == "wifi config accepted")
        .unwrap();
    assert_eq!(wifi_event.payload["ssid"], "FluxPurr-Lab");
    assert_eq!(wifi_event.payload["passwordPresent"], true);
    assert!(
        !serde_json::to_string(&wifi_event.payload)
            .unwrap()
            .contains("secret-pass")
    );
    let runtime_event = device
        .events
        .iter()
        .find(|event| event.kind == "runtime" && event.message == "runtime config applied")
        .unwrap();
    assert_eq!(runtime_event.payload["status"]["targetTempC"], 231);
    assert_eq!(
        runtime_event.payload["status"]["activeCoolingEnabled"],
        false
    );
    assert_eq!(runtime_event.payload["status"]["heaterEnabled"], false);
}

fn assert_flash_blocked_event(state: &AppState) {
    let inner = state.lock().unwrap();
    let device = inner.devices.get("serial-test").unwrap();
    assert!(device.events.iter().any(|event| {
        event.kind == "flash"
            && event.message == "real flash blocked"
            && event.payload["code"] == "real_flash_disabled"
    }));
}

#[tokio::test]
async fn runtime_endpoint_previews_and_clears_thermal_control_profile() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let preview = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(thermal_profile_runtime_request(
            lease.lease_id.clone(),
            ThermalControlProfileOp::Preview,
            Some(thermal_preview_profile()),
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(preview.thermal_control_profile_preview);
    assert!(preview.thermal_control.profile_active);
    assert_eq!(preview.thermal_control.profile_source, "preview");

    let clear_saved = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(thermal_profile_runtime_request(
            lease.lease_id.clone(),
            ThermalControlProfileOp::ClearSaved,
            None,
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(clear_saved.thermal_control_profile_preview);
    assert!(clear_saved.thermal_control.profile_active);
    assert_eq!(clear_saved.thermal_control.profile_source, "preview");

    let clear = configure_runtime(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(thermal_profile_runtime_request(
            lease.lease_id,
            ThermalControlProfileOp::ClearPreview,
            None,
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(!clear.thermal_control_profile_preview);
    assert_eq!(clear.thermal_control.profile_source, "default");
    assert!(!clear.thermal_control.profile_active);
}

#[tokio::test]
async fn runtime_endpoint_saves_and_clears_saved_thermal_control_profile() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let points = (0..FRONT_PANEL_PRESET_COUNT)
        .map(|index| {
            Some(test_thermal_control_profile_point(
                60 + i16::try_from(index).unwrap() * 20,
            ))
        })
        .collect();
    let save = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(RuntimeConfigRequest {
            lease_id: lease.lease_id.clone(),
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: None,
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: Some(ThermalControlProfileRequest {
                op: ThermalControlProfileOp::Save,
                bank: None,
                profile: Some(ThermalControlProfilePackage {
                    settings: None,
                    points,
                }),
            }),
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(!save.thermal_control_profile_preview);
    assert!(save.thermal_control.profile_active);
    assert_eq!(save.thermal_control.profile_source, "saved");

    let clear_saved = configure_runtime(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(RuntimeConfigRequest {
            lease_id: lease.lease_id,
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: None,
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: Some(ThermalControlProfileRequest {
                op: ThermalControlProfileOp::ClearSaved,
                bank: None,
                profile: None,
            }),
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(!clear_saved.thermal_control_profile_preview);
    assert_eq!(clear_saved.thermal_control.profile_source, "default");
    assert!(!clear_saved.thermal_control.profile_active);
}

#[test]
fn thermal_profile_clear_preview_rejects_profile_payload() {
    let error = validate_thermal_control_profile_request(&ThermalControlProfileRequest {
        op: ThermalControlProfileOp::ClearPreview,
        bank: None,
        profile: Some(ThermalControlProfilePackage {
            settings: None,
            points: vec![None; FRONT_PANEL_PRESET_COUNT],
        }),
    })
    .unwrap_err();

    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.error.code, "invalid_thermal_profile");
}

#[test]
fn thermal_profile_rejects_out_of_range_warmup_reentry() {
    let mut point = test_thermal_control_profile_point(140);
    point.warmup_reenter_centi_c = 5_001;
    let mut points = vec![None; FRONT_PANEL_PRESET_COUNT];
    points[0] = Some(point);

    let error = validate_thermal_control_profile_request(&ThermalControlProfileRequest {
        op: ThermalControlProfileOp::Preview,
        bank: None,
        profile: Some(ThermalControlProfilePackage {
            settings: None,
            points,
        }),
    })
    .unwrap_err();

    assert_eq!(error.error.code, "invalid_thermal_profile");
}

#[test]
fn runtime_config_rejects_unknown_thermal_profile_enums() {
    let mut payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        fault_attention_acknowledged: None,
        calibration: None,
        thermal_profile_mode: Some("turbo".to_string()),
        thermal_control_profile: None,
    };
    assert_eq!(
        validate_runtime_config(&payload).unwrap_err().error.code,
        "invalid_thermal_profile_mode"
    );

    payload.thermal_profile_mode = None;
    payload.thermal_control_profile = Some(ThermalControlProfileRequest {
        op: ThermalControlProfileOp::ClearSaved,
        bank: Some("pps9a".to_string()),
        profile: None,
    });
    assert_eq!(
        validate_runtime_config(&payload).unwrap_err().error.code,
        "invalid_thermal_profile_bank"
    );
}

#[test]
fn runtime_config_validates_multi_level_fan_modes_and_legacy_conflicts() {
    let mut payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: Some("fast".to_string()),
        heating_fan_guard_mode: Some("high".to_string()),
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        fault_attention_acknowledged: None,
        calibration: None,
        thermal_profile_mode: None,
        thermal_control_profile: None,
    };
    validate_runtime_config(&payload).expect("fan modes should validate");

    payload.heating_fan_guard_mode = Some("turbo".to_string());
    assert_eq!(
        validate_runtime_config(&payload).unwrap_err().error.code,
        "invalid_heating_fan_guard_mode"
    );

    payload.heating_fan_guard_mode = Some("high".to_string());
    payload.active_cooling_enabled = Some(false);
    assert_eq!(
        validate_runtime_config(&payload).unwrap_err().error.code,
        "fan_policy_conflict"
    );
}

#[tokio::test]
async fn mock_persistence_to_inactive_bank_does_not_switch_the_resolved_bank() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let status = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(RuntimeConfigRequest {
            lease_id: lease.lease_id,
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            fault_attention_acknowledged: None,
            calibration: None,
            thermal_profile_mode: None,
            thermal_control_profile: Some(ThermalControlProfileRequest {
                op: ThermalControlProfileOp::Save,
                bank: Some("pps5a".to_string()),
                profile: Some(ThermalControlProfilePackage {
                    settings: None,
                    points: vec![None; FRONT_PANEL_PRESET_COUNT],
                }),
            }),
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(status.thermal_profile_resolved_bank, "pps3a");
    assert_eq!(status.thermal_control.profile_source, "default");
    assert!(
        state
            .lock()
            .unwrap()
            .devices
            .get("mock-fp-lab-01")
            .unwrap()
            .saved_thermal_control_profile_pps5a
            .is_some()
    );
}

#[test]
fn thermal_profile_preview_accepts_the_ch224q_5v_floor() {
    let result = validate_thermal_control_profile_request(&ThermalControlProfileRequest {
        op: ThermalControlProfileOp::Preview,
        bank: None,
        profile: Some(ThermalControlProfilePackage {
            settings: Some(ThermalControlProfileSettings {
                temp_filter_alpha_permille: 700,
                warmup_reenter_centi_c: 400,
                hold_entry_centi_c: 90,
                hold_exit_centi_c: 200,
                hold_on_centi_c: 30,
                hold_off_centi_c: 5,
                overshoot_cutoff_centi_c: 25,
                approach_max_ticks: 5,
                approach_min_power_ratio_permille: 0,
                hold_kp_permille_per_c: 120,
                hold_ki_permille_per_c_tick: 12,
                hold_blend_ticks: 12,
                hold_reheat_power_permille: 0,
                approach_lead_ticks: 0,
                hold_lead_ticks: 0,
                auto_adjustable_working_floor_mv: PPS_HARDWARE_MIN_MV,
                heater_current_reserve_ma: 200,
            }),
            points: vec![None; FRONT_PANEL_PRESET_COUNT],
        }),
    });

    assert!(result.is_ok());
}

#[tokio::test]
async fn daemon_local_device_mutations_require_valid_lease() {
    let state = AppState::test();
    let missing_lease = bind_device(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(LeaseQuery {
            lease_id: Some("missing-lease".to_string()),
        }),
        Json(BindRequest {
            alias: Some("Bench Alias".to_string()),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(missing_lease.status, StatusCode::FORBIDDEN);
    assert_eq!(missing_lease.error.code, "lease_expired");

    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let bound = bind_device(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(LeaseQuery {
            lease_id: Some(lease.lease_id.clone()),
        }),
        Json(BindRequest {
            alias: Some("Bench Alias".to_string()),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(bound.display_name, "Bench Alias");

    let connected = connect_device(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(LeaseQuery {
            lease_id: Some(lease.lease_id.clone()),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(connected.connection, ConnectionState::Connected);

    let disconnected = disconnect_device(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(LeaseQuery {
            lease_id: Some(lease.lease_id),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(disconnected.connection, ConnectionState::Disconnected);
}

#[tokio::test]
async fn wifi_and_runtime_successes_record_safe_events() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();

    let _ = configure_wifi(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(WifiConfigRequest {
            lease_id: lease.lease_id.clone(),
            op: WifiConfigOp::Set,
            ssid: Some("FluxPurr-Lab".to_string()),
            password: Some("secret-pass".to_string()),
            static_ipv4: None,
            telemetry_interval_ms: Some(500),
        }),
    )
    .await
    .unwrap();

    let _ = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(mock_runtime_request(
            lease.lease_id.clone(),
            Some(231),
            Some(false),
            Some(false),
            None,
            None,
            None,
        )),
    )
    .await
    .unwrap();

    assert_wifi_runtime_events(&state);

    let invalid_manual = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(mock_runtime_request(
            lease.lease_id.clone(),
            Some(199),
            None,
            None,
            Some(true),
            None,
            None,
        )),
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_manual.error.code, "invalid_manual_pps");
    {
        let inner = state.lock().unwrap();
        let device = inner.devices.get("mock-fp-lab-01").unwrap();
        assert_eq!(device.status.target_temp_c, 231);
        assert!(!device.status.manual_pps_enabled);
    }

    let manual_status = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(mock_runtime_request(
            lease.lease_id.clone(),
            None,
            None,
            None,
            Some(true),
            Some(10_400),
            Some(2_500),
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(manual_status.manual_pps_enabled);
    assert_eq!(manual_status.manual_pps_mv, Some(10_400));
    assert_eq!(manual_status.manual_pps_ma, Some(2_500));
    assert_eq!(manual_status.pd_contract_mv, 10_400);

    let cleared_status = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(mock_runtime_request(
            lease.lease_id,
            None,
            None,
            None,
            Some(false),
            None,
            None,
        )),
    )
    .await
    .unwrap()
    .0;
    assert!(!cleared_status.manual_pps_enabled);
    assert_eq!(cleared_status.manual_pps_mv, None);
    assert_eq!(cleared_status.manual_pps_ma, None);
    assert_eq!(cleared_status.pd_contract_mv, DEFAULT_PD_REQUEST_MV);
    assert_eq!(cleared_status.voltage_mv, u32::from(DEFAULT_PD_REQUEST_MV));
}

#[tokio::test]
async fn calibration_runtime_uses_readback_current_and_ignores_stale_calibration_current() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    {
        let mut inner = state.lock().unwrap();
        let device = inner.devices.get_mut("mock-fp-lab-01").unwrap();
        device.status.current_ma = 1_350;
        device.status.manual_pps_ma = None;
        device.status.pps_capability_max_ma = Some(3_000);
        device.status.calibration.pps_ma = Some(2_500);
    }

    let status = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(RuntimeConfigRequest {
            lease_id: lease.lease_id,
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: Some(CalibrationControlRequest {
                mode: Some(CalibrationMode::VinAdc),
                pps_enabled: Some(true),
                pps_mv: Some(12_000),
                heater_enabled: Some(false),
                target_adc_mv: None,
            }),
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: None,
        }),
    )
    .await
    .unwrap()
    .0;

    assert!(status.calibration.pps_enabled);
    assert_eq!(status.calibration.pps_mv, Some(12_000));
    assert_eq!(status.calibration.pps_ma, Some(1_350));
    assert_eq!(status.manual_pps_ma, Some(1_350));
}

#[tokio::test]
async fn calibration_runtime_falls_back_to_capability_current_when_readback_is_missing() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    {
        let mut inner = state.lock().unwrap();
        let device = inner.devices.get_mut("mock-fp-lab-01").unwrap();
        device.status.current_ma = 0;
        device.status.manual_pps_ma = None;
        device.status.pps_capability_max_ma = Some(3_000);
        device.status.calibration.pps_ma = Some(2_500);
    }

    let status = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(RuntimeConfigRequest {
            lease_id: lease.lease_id,
            target_temp_c: None,
            selected_preset_slot: None,
            presets_c: None,
            active_cooling_enabled: None,
            post_heat_cooling_mode: None,
            heating_fan_guard_mode: None,
            heater_enabled: None,
            manual_pps_enabled: None,
            manual_pps_mv: None,
            manual_pps_ma: None,
            calibration: Some(CalibrationControlRequest {
                mode: Some(CalibrationMode::VinAdc),
                pps_enabled: Some(true),
                pps_mv: Some(12_000),
                heater_enabled: Some(false),
                target_adc_mv: None,
            }),
            thermal_profile_mode: None,
            fault_attention_acknowledged: None,
            thermal_control_profile: None,
        }),
    )
    .await
    .unwrap()
    .0;

    assert!(status.calibration.pps_enabled);
    assert_eq!(status.calibration.pps_ma, Some(3_000));
    assert_eq!(status.manual_pps_ma, Some(3_000));
}

#[tokio::test]
async fn thermal_plant_mock_job_requires_20v_three_amp_capability() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let started = configure_calibration_job(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id.clone(),
            op: CalibrationJobOp::Start,
            kind: Some(CalibrationJobKind::ThermalPlantAuto),
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(started.status, CalibrationJobStatus::Running);
    assert_eq!(started.kind, Some(CalibrationJobKind::ThermalPlantAuto));
    assert_eq!(started.next_request_mv, Some(21_000));
    assert_eq!(
        state
            .lock()
            .unwrap()
            .devices
            .get("mock-fp-lab-01")
            .unwrap()
            .status
            .calibration
            .mode,
        CalibrationMode::ThermalPlant
    );

    let (
        heater_enabled,
        heater_output_percent,
        heater_physical_output_percent,
        manual_pps_mv,
        calibration_pps_mv,
    ) = {
        let state_lock = state.lock().unwrap();
        let status = &state_lock.devices.get("mock-fp-lab-01").unwrap().status;
        (
            status.heater_enabled,
            status.heater_output_percent,
            status.heater_physical_output_percent,
            status.manual_pps_mv,
            status.calibration.pps_mv,
        )
    };
    assert!(!heater_enabled);
    assert_eq!(heater_output_percent, 0);
    assert_eq!(heater_physical_output_percent, 0);
    assert_eq!(manual_pps_mv, Some(21_000));
    assert_eq!(calibration_pps_mv, Some(21_000));

    let _ = configure_calibration_job(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id.clone(),
            op: CalibrationJobOp::Cancel,
            kind: None,
        }),
    )
    .await
    .unwrap();

    {
        let mut state_lock = state.lock().unwrap();
        state_lock
            .devices
            .get_mut("mock-fp-lab-01")
            .unwrap()
            .status
            .pps_capability_max_ma = Some(2_999);
        state_lock
            .devices
            .get_mut("mock-fp-lab-01")
            .unwrap()
            .mock_pps_apdos[0]
            .max_ma = 2_999;
    }
    let error = configure_calibration_job(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id,
            op: CalibrationJobOp::Start,
            kind: Some(CalibrationJobKind::ThermalPlantAuto),
        }),
    )
    .await
    .unwrap_err();

    assert_eq!(error.error.code, "thermal_plant_source_unsupported");
}

#[tokio::test]
async fn thermal_plant_mock_job_locks_runtime_overrides_and_state_transitions() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();

    let _ = configure_calibration_job(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id.clone(),
            op: CalibrationJobOp::Start,
            kind: Some(CalibrationJobKind::ThermalPlantAuto),
        }),
    )
    .await
    .unwrap();

    let repeated_start = configure_calibration_job(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id.clone(),
            op: CalibrationJobOp::Start,
            kind: Some(CalibrationJobKind::ThermalPlantAuto),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(repeated_start.error.code, "heater_disarm_pending");

    let manual_override = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(mock_runtime_request(
            lease.lease_id.clone(),
            None,
            None,
            None,
            Some(true),
            Some(20_000),
            Some(3_000),
        )),
    )
    .await
    .unwrap_err();
    assert_eq!(manual_override.error.code, "manual_pps_calibration_busy");

    let heater_override = configure_runtime(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(mock_runtime_request(
            lease.lease_id.clone(),
            None,
            None,
            Some(true),
            None,
            None,
            None,
        )),
    )
    .await
    .unwrap_err();
    assert_eq!(heater_override.error.code, "manual_pps_calibration_busy");

    let canceled = configure_calibration_job(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id.clone(),
            op: CalibrationJobOp::Cancel,
            kind: None,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(canceled.status, CalibrationJobStatus::Canceled);
    assert_eq!(
        state
            .lock()
            .unwrap()
            .devices
            .get("mock-fp-lab-01")
            .unwrap()
            .status
            .calibration
            .mode,
        CalibrationMode::Off
    );

    let idle_cancel = configure_calibration_job(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id,
            op: CalibrationJobOp::Cancel,
            kind: None,
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(idle_cancel.status, CalibrationJobStatus::Canceled);
}

#[tokio::test]
async fn thermal_plant_mock_job_requires_one_matching_apdo() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    {
        let mut state_lock = state.lock().unwrap();
        let device = state_lock.devices.get_mut("mock-fp-lab-01").unwrap();
        device.status.pps_capability_min_mv = Some(5_000);
        device.status.pps_capability_max_mv = Some(21_000);
        device.status.pps_capability_max_ma = Some(5_000);
        device.mock_pps_apdos = vec![
            MockPpsApdo {
                min_mv: 5_000,
                max_mv: 11_000,
                max_ma: 5_000,
            },
            MockPpsApdo {
                min_mv: 20_000,
                max_mv: 21_000,
                max_ma: 1_000,
            },
        ];
    }

    let error = configure_calibration_job(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id,
            op: CalibrationJobOp::Start,
            kind: Some(CalibrationJobKind::ThermalPlantAuto),
        }),
    )
    .await
    .unwrap_err();

    assert_eq!(error.error.code, "thermal_plant_source_unsupported");
}

#[tokio::test]
async fn thermal_plant_mock_job_accepts_an_apdo_whose_range_starts_at_20v() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    {
        let mut state_lock = state.lock().unwrap();
        let device = state_lock.devices.get_mut("mock-fp-lab-01").unwrap();
        device.status.pps_capability_min_mv = Some(20_000);
        device.status.pps_capability_max_mv = Some(21_000);
        device.status.pps_capability_max_ma = Some(3_000);
        device.mock_pps_apdos = vec![MockPpsApdo {
            min_mv: 20_000,
            max_mv: 21_000,
            max_ma: 3_000,
        }];
    }

    let started = configure_calibration_job(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Json(CalibrationJobRequest {
            lease_id: lease.lease_id,
            op: CalibrationJobOp::Start,
            kind: Some(CalibrationJobKind::ThermalPlantAuto),
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(started.status, CalibrationJobStatus::Running);
    assert_eq!(started.next_request_mv, Some(21_000));
}

#[tokio::test]
async fn thermal_plant_run_reader_pages_cooling_trace_without_raw_adc() {
    let state = AppState::test();
    let lease = state.lease_device("mock-fp-lab-01").unwrap();

    let first = device_thermal_plant_run(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(ThermalPlantRunQuery {
            lease_id: Some(lease.lease_id.clone()),
            after_sample: Some(0),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(first.trace_page.points.len(), 14);
    assert_eq!(first.trace_page.next_sample, None);
    assert!(
        first
            .trace_page
            .points
            .iter()
            .any(|point| point.phase == ThermalPlantRunPhase::Cooling)
    );
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains("rawAdc"));
    assert!(serialized.len() < 8 * 1024);

    let tail = device_thermal_plant_run(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(ThermalPlantRunQuery {
            lease_id: Some(lease.lease_id),
            after_sample: Some(8),
        }),
    )
    .await
    .unwrap()
    .0;
    assert_eq!(tail.trace_page.start_sample, 8);
    assert_eq!(tail.trace_page.points.len(), 6);
    assert_eq!(tail.trace_page.points[0].sample_index, 8);
}

#[test]
fn calibration_slot_fit_normalizes_invalid_coefficients() {
    let mut device = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock);
    device.calibration.rtd_adc.slots.a = CalibrationSlotFit {
        gain: 0.0,
        offset_mv: f32::NAN,
    };
    device.calibration.rtd_adc.sanitize_slot_fits();

    assert_eq!(device.calibration.rtd_adc.slots.a.gain, 1.0);
    assert_eq!(device.calibration.rtd_adc.slots.a.offset_mv, 0.0);
}

#[test]
fn manual_calibration_cannot_select_the_thermal_plant_runtime_state() {
    let status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
    let error = validate_calibration_request_against_status(
        &CalibrationControlRequest {
            mode: Some(CalibrationMode::ThermalPlant),
            pps_enabled: None,
            pps_mv: None,
            heater_enabled: None,
            target_adc_mv: None,
        },
        &status,
        &status.calibration,
    )
    .unwrap_err();

    assert_eq!(error.error.code, "thermal_plant_managed_by_job");
}

#[tokio::test]
async fn real_flash_requires_dry_run_confirmation_and_allow_flag() {
    let dir = tempdir().unwrap();
    let artifact = test_artifact_with_file(dir.path(), "firmware.bin", b"firmware-image");
    let state = AppState::new(AppConfig {
        artifact_root: Some(dir.path().to_path_buf()),
        ..AppConfig::default()
    });
    let mut native = DeviceRecord::mock("serial-test", DeviceTransport::NativeSerial);
    native.port_path = Some("/dev/cu.usbmodem21221401".to_string());
    {
        let mut inner = state.lock().unwrap();
        inner.devices.insert(native.id.clone(), native);
    }
    let lease = state.lease_device("serial-test").unwrap();

    let without_dry_run = flash_device(
        State(state.clone()),
        AxumPath("serial-test".to_string()),
        Json(FlashRequest {
            lease_id: lease.lease_id.clone(),
            artifact: artifact.clone(),
            dry_run: false,
            confirm: None,
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(without_dry_run.status, StatusCode::FORBIDDEN);
    assert_eq!(without_dry_run.error.code, "dry_run_required");

    let dry_run = flash_device(
        State(state.clone()),
        AxumPath("serial-test".to_string()),
        Json(FlashRequest {
            lease_id: lease.lease_id.clone(),
            artifact: artifact.clone(),
            dry_run: true,
            confirm: None,
        }),
    )
    .await
    .unwrap()
    .0;
    assert!(dry_run.dry_run);
    assert_eq!(dry_run.status, "passed");
    {
        let inner = state.lock().unwrap();
        let device = inner.devices.get("serial-test").unwrap();
        assert_eq!(
            device.selected_artifact_id.as_deref(),
            Some("test-artifact")
        );
        assert!(device.events.iter().any(|event| {
            event.kind == "flash"
                && event.message == "artifact dry-run passed"
                && event.payload["artifactId"] == "test-artifact"
        }));
    }

    let changed_artifact =
        test_artifact_with_file(dir.path(), "firmware-v2.bin", b"firmware-image-v2");
    let changed_without_dry_run = flash_device(
        State(state.clone()),
        AxumPath("serial-test".to_string()),
        Json(FlashRequest {
            lease_id: lease.lease_id.clone(),
            artifact: changed_artifact,
            dry_run: false,
            confirm: Some("FLASH".to_string()),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(changed_without_dry_run.status, StatusCode::FORBIDDEN);
    assert_eq!(changed_without_dry_run.error.code, "dry_run_required");

    let without_confirm = flash_device(
        State(state.clone()),
        AxumPath("serial-test".to_string()),
        Json(FlashRequest {
            lease_id: lease.lease_id.clone(),
            artifact: artifact.clone(),
            dry_run: false,
            confirm: None,
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(without_confirm.status, StatusCode::FORBIDDEN);
    assert_eq!(without_confirm.error.code, "confirmation_required");

    let flash_disabled = flash_device(
        State(state.clone()),
        AxumPath("serial-test".to_string()),
        Json(FlashRequest {
            lease_id: lease.lease_id,
            artifact,
            dry_run: false,
            confirm: Some("FLASH".to_string()),
        }),
    )
    .await
    .unwrap_err();
    assert_eq!(flash_disabled.status, StatusCode::FORBIDDEN);
    assert_eq!(flash_disabled.error.code, "real_flash_disabled");
    assert_flash_blocked_event(&state);
}

#[test]
fn wifi_response_redacts_password_shape() {
    let request = WifiConfigRequest {
        lease_id: "lease-1".to_string(),
        op: WifiConfigOp::Set,
        ssid: Some("FluxPurr-Lab".to_string()),
        password: Some("secret-pass".to_string()),
        static_ipv4: Some(Some(WifiStaticIpv4Request {
            address: [192, 168, 31, 42],
            prefix_len: 24,
            gateway: [192, 168, 31, 1],
            dns: [1, 1, 1, 1],
        })),
        telemetry_interval_ms: Some(500),
    };
    let value = json!({
        "wifi": {
            "op": request.op,
            "ssid": request.ssid,
            "password": request.password.as_ref().map(|_| "<redacted>")
        }
    });
    assert!(value.to_string().contains("<redacted>"));
    assert!(!value.to_string().contains("secret-pass"));
}

#[test]
fn wifi_receipt_rejects_unversioned_and_malformed_network_snapshots() {
    let unversioned = extract_wifi_config_network(
        json!({
            "wifi": {
                "network": {
                    "state": "saving",
                    "ssid": null,
                    "wifiPasswordLength": 0,
                    "ip": null,
                    "gateway": null,
                    "dns": [],
                    "wifiRssi": null,
                    "lastError": null
                }
            }
        }),
        false,
    )
    .unwrap_err();
    assert_eq!(unversioned.error.code, "invalid_wifi_receipt");

    let malformed =
        extract_wifi_config_network(json!({ "wifi": { "network": 42 } }), false).unwrap_err();
    assert_eq!(malformed.error.code, "usb_payload_decode_failed");

    let non_public = extract_wifi_config_network(
        json!({
            "wifi": {
                "network": {
                    "state": "saving",
                    "configurationGeneration": 1,
                    "transitionSequence": 1,
                    "ssid": null,
                    "wifiPasswordLength": 0,
                    "ip": null,
                    "gateway": null,
                    "dns": [],
                    "wifiRssi": null,
                    "lastError": null
                }
            }
        }),
        false,
    )
    .unwrap_err();
    assert_eq!(non_public.error.code, "invalid_wifi_receipt");
    assert_eq!(
        non_public.error.message,
        "The device returned a non-public WiFi state."
    );
}

#[test]
fn wifi_cancel_receipt_accepts_only_the_confirmed_idle_snapshot() {
    let receipt = extract_wifi_config_network(
        json!({
            "wifi": {
                "network": {
                    "state": "idle",
                    "configurationGeneration": 4,
                    "transitionSequence": 12,
                    "ssid": "FluxPurr-Lab",
                    "wifiPasswordLength": 11,
                    "ip": null,
                    "gateway": null,
                    "dns": [],
                    "wifiRssi": null,
                    "lastError": null
                }
            }
        }),
        true,
    )
    .unwrap();

    assert_eq!(receipt.state, NetworkState::Idle);
    assert_eq!(receipt.ssid.as_deref(), Some("FluxPurr-Lab"));
}

#[test]
fn wifi_static_ipv4_preserves_absent_and_forwards_explicit_dhcp_clear() {
    let absent: WifiConfigRequest = serde_json::from_value(json!({
        "leaseId": "lease-1",
        "op": "set",
        "autoReconnect": false
    }))
    .unwrap();
    let clear: WifiConfigRequest = serde_json::from_value(json!({
        "leaseId": "lease-1",
        "op": "set",
        "staticIpv4": null
    }))
    .unwrap();

    assert!(absent.static_ipv4.is_none());
    assert!(matches!(clear.static_ipv4, Some(None)));

    let clear_wire = serde_json::to_value(UsbWifiConfigWire {
        frame_type: "wifi_config",
        request_id: "wifi-clear",
        op: "set",
        ssid: None,
        password: None,
        static_ipv4: clear.static_ipv4,
        telemetry_interval_ms: None,
    })
    .unwrap();
    let absent_wire = serde_json::to_value(UsbWifiConfigWire {
        frame_type: "wifi_config",
        request_id: "wifi-preserve",
        op: "set",
        ssid: None,
        password: None,
        static_ipv4: absent.static_ipv4,
        telemetry_interval_ms: None,
    })
    .unwrap();

    assert!(clear_wire["staticIpv4"].is_null());
    assert!(absent_wire.get("staticIpv4").is_none());
}

#[test]
fn static_ipv4_validation_rejects_non_unicast_values() {
    let valid = WifiStaticIpv4Request {
        address: [192, 168, 31, 42],
        prefix_len: 24,
        gateway: [192, 168, 31, 1],
        dns: [1, 1, 1, 1],
    };
    assert!(static_ipv4_request_is_valid(valid));
    assert!(!static_ipv4_request_is_valid(WifiStaticIpv4Request {
        address: [224, 0, 0, 1],
        ..valid
    }));
    assert!(!static_ipv4_request_is_valid(WifiStaticIpv4Request {
        dns: [0, 0, 0, 0],
        ..valid
    }));
}

#[test]
fn usb_response_decoder_ignores_logs_and_selects_matching_request() {
    assert!(
        decode_usb_response_line(b"INFO firmware booted", "req-1")
            .unwrap()
            .is_none()
    );
    assert!(
            decode_usb_response_line(
                br#"{"type":"response","requestId":"other","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#,
                "req-1"
            )
            .unwrap()
            .is_none()
        );

    let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"req-1","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#,
            "req-1",
        )
        .unwrap()
        .unwrap();

    let network = extract_usb_payload::<NetworkSummary>(payload, "network").unwrap();
    assert_eq!(network.state, NetworkState::Disabled);
}

#[test]
fn usb_response_decoder_extracts_a_matching_frame_appended_to_a_boot_log() {
    let payload = decode_usb_response_line(
            br#"I (181) esp_image: segment 1: paddr=00061018 vaddr=3fc91988 size{"type":"response","requestId":"req-1","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#,
            "req-1",
        )
        .unwrap()
        .unwrap();

    let network = extract_usb_payload::<NetworkSummary>(payload, "network").unwrap();
    assert_eq!(network.state, NetworkState::Disabled);
}

#[test]
fn serial_line_reader_discards_an_overlong_frame_before_the_next_response() {
    let mut line = Vec::new();
    let mut discarding = false;
    for _ in 0..SERIAL_LINE_CONTENT_LIMIT {
        assert!(!serial_line_finished(&mut line, &mut discarding, b'x'));
    }
    assert_eq!(line.len(), SERIAL_LINE_CONTENT_LIMIT);
    assert!(serial_line_finished(&mut line, &mut discarding, b'\n'));
    assert!(!discarding);
    assert_eq!(line.len(), SERIAL_LINE_CONTENT_LIMIT);

    line.clear();
    for _ in 0..=SERIAL_LINE_CONTENT_LIMIT {
        assert!(!serial_line_finished(&mut line, &mut discarding, b'x'));
    }
    assert!(discarding);
    assert!(!serial_line_finished(&mut line, &mut discarding, b'\n'));
    assert!(!discarding);
    assert!(line.is_empty());

    let response = br#"{"type":"response","requestId":"req-1","ok":true,"result":{"network":{"state":"disabled","dns":[]}}}"#;
    for byte in response {
        assert!(!serial_line_finished(&mut line, &mut discarding, *byte));
    }
    assert!(serial_line_finished(&mut line, &mut discarding, b'\n'));
    assert!(decode_usb_response_line(&line, "req-1").unwrap().is_some());
}

#[test]
fn usb_reset_markers_are_observed_without_reopening_the_session() {
    assert!(serial_line_is_usb_reset_marker(
        b"reset_reason=core_usb_uart"
    ));
    assert!(serial_line_is_usb_reset_marker(
        b" reset_reason=core_usb_jtag\r"
    ));
    assert!(!serial_line_is_usb_reset_marker(
        b"reset_reason=core_software"
    ));
    assert!(!serial_line_is_usb_reset_marker(
        b"boot_stage=lan_heap_ready"
    ));
    assert!(!serial_line_is_usb_reset_marker(
        br#"{\"type\":\"response\",\"requestId\":\"req-1\",\"ok\":true}"#
    ));
}

#[test]
fn post_flash_boot_requires_runtime_ready() {
    let mut observation = BootObservation::default();

    assert!(
        !observation
            .observe_line("reset_reason=core_software")
            .unwrap()
    );
    assert!(
        !observation
            .observe_line("boot_stage=adc_init_complete")
            .unwrap()
    );
    assert!(observation.observe_line(RUNTIME_READY_BOOT_STAGE).unwrap());
    assert_eq!(observation.reset_count, 1);
    assert_eq!(
        observation.last_stage.as_deref(),
        Some(RUNTIME_READY_BOOT_STAGE)
    );
}

#[test]
fn post_flash_boot_rejects_a_stale_runtime_ready_marker() {
    let mut observation = BootObservation::default();

    assert!(!observation.observe_line(RUNTIME_READY_BOOT_STAGE).unwrap());
    assert!(observation.last_stage.is_none());
    assert!(
        !observation
            .observe_line("boot_stage=display_init_complete")
            .unwrap()
    );
    assert!(observation.observe_line(RUNTIME_READY_BOOT_STAGE).unwrap());
}

#[test]
fn post_flash_boot_rejects_reboot_loops_and_panics() {
    let mut reboot = BootObservation::default();
    reboot.observe_line("reset_reason=core_software").unwrap();
    let error = reboot
        .observe_line("reset_reason=core_software")
        .unwrap_err();
    assert_eq!(error.error.code, "firmware_reboot_loop");

    let error = BootObservation::default()
        .observe_line("Guru Meditation Error: Core 0 panic'ed")
        .unwrap_err();
    assert_eq!(error.error.code, "firmware_boot_failed");
}

#[test]
fn usb_response_decoder_extracts_runtime_config_status_payload() {
    let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"runtime-1","ok":true,"result":{"status":{"mode":"sampling","uptimeSeconds":12,"currentTempC":194.0,"targetTempC":240,"heaterEnabled":true,"heaterOutputPercent":25,"activeCoolingEnabled":false,"fanDisplayState":"AUTO","fanEnabled":true,"fanPwmPermille":500,"voltageMv":20000,"currentMa":850,"boardTempCenti":1940,"pdRequestMv":20000,"pdContractMv":20000,"pdState":"ready","frontpanelKey":null,"network":{"state":"idle","dns":[],"wifiRssi":null}}}}"#,
            "runtime-1",
        )
        .unwrap()
        .unwrap();

    let status = extract_usb_payload::<ControlPlaneStatus>(payload, "status").unwrap();

    assert_eq!(status.target_temp_c, 240);
    assert!(status.heater_enabled);
    assert!(!status.active_cooling_enabled);
    assert!(status.adc_diagnostics.is_none());
}

#[test]
fn usb_response_decoder_extracts_buzzer_test_payload() {
    let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"buzzer-1","ok":true,"result":{"buzzer_test":{"state":"complete","scenario":"feedback_replace","activeCue":"heater_on","trace":[{"elapsedMs":30,"decision":{"source":"buzzer_test","cue":"heater_on","disposition":"replaced"}}],"outputTrace":[{"elapsedMs":90,"requestedFrequencyHz":1680,"appliedFrequencyHz":1739,"dutyPercent":50,"generation":2,"timerPrescaler":22,"timerPeriodTicks":999}]}}}"#,
            "buzzer-1",
        )
        .unwrap()
        .unwrap();

    let status = extract_usb_payload::<BuzzerTestStatus>(payload, "buzzer_test").unwrap();
    assert_eq!(status.state, BuzzerTestSessionState::Complete);
    assert_eq!(status.scenario, Some(BuzzerTestScenario::FeedbackReplace));
    assert_eq!(status.active_cue.as_deref(), Some("heater_on"));
    assert_eq!(status.trace[0].decision.disposition, "replaced");
    assert_eq!(status.output_trace[0].applied_frequency_hz, 1_739);
}

#[test]
fn usb_response_decoder_preserves_adc_diagnostics() {
    let payload = decode_usb_response_line(
            br#"{"type":"response","requestId":"status-adc","ok":true,"result":{"status":{"mode":"sampling","uptimeSeconds":12,"currentTempC":31.5,"targetTempC":240,"heaterEnabled":false,"heaterOutputPercent":0,"activeCoolingEnabled":false,"fanDisplayState":"OFF","fanEnabled":false,"fanPwmPermille":0,"voltageMv":20000,"currentMa":0,"boardTempCenti":3150,"adcDiagnostics":{"calibrationSource":"efuse","efuseVersion":1,"attenuationDb":6,"initCode":1850,"referenceCode":1600,"referenceMv":850,"rtdRawCodeMean":2100,"rtdRawCodeMin":2098,"rtdRawCodeMax":2102,"rtdRawCodeSpread":4,"vinRawCodeMean":1800},"pdRequestMv":20000,"pdContractMv":20000,"pdState":"ready","frontpanelKey":null,"network":{"state":"idle","dns":[],"wifiRssi":null}}}}"#,
            "status-adc",
        )
        .unwrap()
        .unwrap();

    let status = extract_usb_payload::<ControlPlaneStatus>(payload, "status").unwrap();
    let diagnostics = status.adc_diagnostics.expect("ADC diagnostics present");

    assert_eq!(diagnostics.calibration_source, "efuse");
    assert_eq!(diagnostics.rtd_raw_code_spread, 4);
}

#[test]
fn usb_response_decoder_maps_firmware_errors() {
    let error = decode_usb_response_line(
            br#"{"type":"response","requestId":"req-1","ok":false,"error":{"code":"bad_op","message":"Bad op","retryable":false}}"#,
            "req-1",
        )
        .unwrap_err();

    assert_eq!(error.status, StatusCode::BAD_GATEWAY);
    assert_eq!(error.error.code, "bad_op");
    assert!(!error.error.retryable);
}

#[test]
fn usb_response_decoder_ignores_requestless_firmware_frame_errors() {
    assert!(decode_usb_response_line(
            br#"{"type":"error","requestId":null,"error":{"code":"malformed_json","message":"Malformed USB JSONL frame.","retryable":false}}"#,
            "req-1",
        )
        .unwrap()
        .is_none());
}

#[test]
fn usb_response_decoder_marks_startup_busy_retryable() {
    let error = decode_usb_response_line(
            br#"{"type":"response","requestId":"req-1","ok":false,"error":{"code":"startup_busy","message":"Runtime status is not available until hardware initialization completes.","retryable":true}}"#,
            "req-1",
        )
        .unwrap_err();

    assert_eq!(error.status, StatusCode::BAD_GATEWAY);
    assert!(is_retryable_startup_busy(&error));
}

#[test]
fn runtime_config_matcher_accepts_matching_calibration_status() {
    let payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: Some(45),
        selected_preset_slot: Some(2),
        presets_c: Some(vec![
            Some(50),
            Some(100),
            Some(150),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ]),
        active_cooling_enabled: Some(false),
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        calibration: Some(CalibrationControlRequest {
            mode: Some(CalibrationMode::RtdAdc),
            pps_enabled: Some(true),
            pps_mv: Some(12_000),
            heater_enabled: Some(true),
            target_adc_mv: Some(930),
        }),
        thermal_profile_mode: None,
        fault_attention_acknowledged: None,
        thermal_control_profile: None,
    };
    let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
    status.target_temp_c = 45;
    status.selected_preset_slot = Some(2);
    status.presets_c = payload.presets_c.clone();
    status.active_cooling_enabled = false;
    status.calibration.mode = CalibrationMode::RtdAdc;
    status.calibration.pps_enabled = true;
    status.calibration.pps_mv = Some(12_000);
    status.calibration.heater_enabled = true;
    status.calibration.target_adc_mv = Some(930);

    assert!(runtime_config_matches_status(&payload, &status));
}

#[test]
fn runtime_config_matcher_rejects_mismatched_calibration_status() {
    let payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        calibration: Some(CalibrationControlRequest {
            mode: Some(CalibrationMode::VinAdc),
            pps_enabled: Some(true),
            pps_mv: Some(16_000),
            heater_enabled: Some(false),
            target_adc_mv: None,
        }),
        thermal_profile_mode: None,
        fault_attention_acknowledged: None,
        thermal_control_profile: None,
    };
    let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
    status.calibration.mode = CalibrationMode::VinAdc;
    status.calibration.pps_enabled = true;
    status.calibration.pps_mv = Some(12_000);
    status.calibration.heater_enabled = false;

    assert!(!runtime_config_matches_status(&payload, &status));
}

#[test]
fn runtime_config_matcher_requires_attention_to_be_cleared_after_acknowledgement() {
    let payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        fault_attention_acknowledged: Some(true),
        calibration: None,
        thermal_profile_mode: None,
        thermal_control_profile: None,
    };
    let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
    status.fault_attention_pending = true;
    assert!(!runtime_config_matches_status(&payload, &status));
    status.fault_attention_pending = false;
    assert!(runtime_config_matches_status(&payload, &status));
}

#[test]
fn runtime_config_matcher_reconciles_preview_from_status_flag() {
    let mut points = vec![None; FRONT_PANEL_PRESET_COUNT];
    points[0] = Some(ThermalControlProfilePoint {
        target_temp_c: 120,
        brake_distance_centi_c: 700,
        warmup_power_permille: 320,
        warmup_reenter_centi_c: 0,
        approach_power_permille: 320,
        approach_floor_power_permille: 220,
        approach_damping_exponent_permille: 1_000,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 220,
        hold_reheat_power_permille: 0,
        hold_entry_centi_c: 0,
        hold_exit_centi_c: 0,
        hold_on_centi_c: 0,
        hold_off_centi_c: 0,
        overshoot_cutoff_centi_c: 0,
        hold_kp_permille_per_c: 0,
        hold_ki_permille_per_c_tick: 0,
        hold_blend_ticks: 0,
        approach_lead_ticks: 0,
        hold_lead_ticks: 0,
    });
    let payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        calibration: None,
        thermal_profile_mode: None,
        fault_attention_acknowledged: None,
        thermal_control_profile: Some(ThermalControlProfileRequest {
            op: ThermalControlProfileOp::Preview,
            bank: None,
            profile: Some(ThermalControlProfilePackage {
                settings: None,
                points,
            }),
        }),
    };
    let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
    status.thermal_control_profile_preview = true;
    status.thermal_control = mock_thermal_runtime(
        status.target_temp_c,
        payload
            .thermal_control_profile
            .as_ref()
            .and_then(|profile| profile.profile.as_ref()),
        true,
    );

    assert!(runtime_config_matches_status(&payload, &status));
}

#[test]
fn runtime_config_matcher_reconciles_saved_profile_runtime() {
    let mut points = vec![None; FRONT_PANEL_PRESET_COUNT];
    points[0] = Some(ThermalControlProfilePoint {
        target_temp_c: 220,
        brake_distance_centi_c: 520,
        warmup_power_permille: 1_000,
        warmup_reenter_centi_c: 0,
        approach_power_permille: 760,
        approach_floor_power_permille: 600,
        approach_damping_exponent_permille: 550,
        approach_tail_window_centi_c: 0,
        hold_power_permille: 620,
        hold_reheat_power_permille: 700,
        hold_entry_centi_c: 8,
        hold_exit_centi_c: 50,
        hold_on_centi_c: 14,
        hold_off_centi_c: 240,
        overshoot_cutoff_centi_c: 320,
        hold_kp_permille_per_c: 22,
        hold_ki_permille_per_c_tick: 1,
        hold_blend_ticks: 2,
        approach_lead_ticks: 2,
        hold_lead_ticks: 0,
    });
    let payload = RuntimeConfigRequest {
        lease_id: "lease-1".to_string(),
        target_temp_c: None,
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        calibration: None,
        thermal_profile_mode: None,
        fault_attention_acknowledged: None,
        thermal_control_profile: Some(ThermalControlProfileRequest {
            op: ThermalControlProfileOp::Save,
            bank: None,
            profile: Some(ThermalControlProfilePackage {
                settings: None,
                points,
            }),
        }),
    };
    let mut status = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock).status;
    status.thermal_control = mock_thermal_runtime(
        status.target_temp_c,
        payload
            .thermal_control_profile
            .as_ref()
            .and_then(|profile| profile.profile.as_ref()),
        false,
    );

    assert!(runtime_config_matches_status(&payload, &status));
}

#[test]
fn rtd_capture_expected_mv_uses_target_adc_before_temperature_curve() {
    let payload = CalibrationConfigRequest {
        lease_id: "lease-1".to_string(),
        op: CalibrationConfigOp::Capture,
        channel: Some(CalibrationChannel::RtdAdc),
        reference_temp_c: Some(49.0),
        reference_vin_mv: None,
        target_adc_mv: Some(1_000),
        observed_mv: None,
        expected_mv: None,
        sample_index: None,
        state: None,
        slot: None,
        fit: None,
    };

    assert_eq!(
        expected_calibration_adc_mv(&payload, CalibrationChannel::RtdAdc),
        Some(1_000)
    );
}

#[test]
fn rtd_capture_expected_mv_requires_target_adc_without_explicit_expected() {
    let payload = CalibrationConfigRequest {
        lease_id: "lease-1".to_string(),
        op: CalibrationConfigOp::Capture,
        channel: Some(CalibrationChannel::RtdAdc),
        reference_temp_c: Some(49.0),
        reference_vin_mv: None,
        target_adc_mv: None,
        observed_mv: None,
        expected_mv: None,
        sample_index: None,
        state: None,
        slot: None,
        fit: None,
    };

    assert_eq!(
        expected_calibration_adc_mv(&payload, CalibrationChannel::RtdAdc),
        None
    );
}

#[test]
fn backfills_live_rtd_capture_metadata_for_legacy_firmware_response() {
    let mut calibration = CalibrationState::default();
    calibration.rtd_adc.samples[0] = Some(CalibrationSample {
        observed_mv: 1_001,
        expected_mv: 970,
        reference_temp_c: None,
        target_adc_mv: None,
        reference_vin_mv: None,
    });
    let payload = CalibrationConfigRequest {
        lease_id: "lease-1".to_string(),
        op: CalibrationConfigOp::Capture,
        channel: Some(CalibrationChannel::RtdAdc),
        reference_temp_c: Some(49.0),
        reference_vin_mv: None,
        target_adc_mv: Some(1_000),
        observed_mv: None,
        expected_mv: Some(1_000),
        sample_index: None,
        state: None,
        slot: None,
        fit: None,
    };

    backfill_live_calibration_capture(&mut calibration, &payload);

    let sample = calibration.rtd_adc.samples[0].expect("sample should exist");
    assert_eq!(sample.observed_mv, 1_001);
    assert_eq!(sample.expected_mv, 1_000);
    assert_eq!(sample.reference_temp_c, Some(49.0));
    assert_eq!(sample.target_adc_mv, Some(1_000));
}

#[test]
fn merges_live_rtd_sample_metadata_on_refresh() {
    let mut previous = CalibrationState::default();
    previous.rtd_adc.samples[0] = Some(CalibrationSample {
        observed_mv: 1_001,
        expected_mv: 1_000,
        reference_temp_c: Some(49.0),
        target_adc_mv: Some(1_000),
        reference_vin_mv: None,
    });
    let mut refreshed = CalibrationState::default();
    refreshed.rtd_adc.samples[0] = Some(CalibrationSample {
        observed_mv: 1_001,
        expected_mv: 1_000,
        reference_temp_c: None,
        target_adc_mv: None,
        reference_vin_mv: None,
    });

    merge_live_calibration_metadata(&mut refreshed, &previous);

    let sample = refreshed.rtd_adc.samples[0].expect("sample should exist");
    assert_eq!(sample.reference_temp_c, Some(49.0));
    assert_eq!(sample.target_adc_mv, Some(1_000));
}

#[test]
fn incomplete_live_rtd_samples_are_not_web_facing_samples() {
    let mut calibration = CalibrationState::default();
    calibration.rtd_adc.samples[0] = Some(CalibrationSample {
        observed_mv: 1_001,
        expected_mv: 970,
        reference_temp_c: None,
        target_adc_mv: None,
        reference_vin_mv: None,
    });

    calibration.refresh_fits();

    assert!(calibration.rtd_adc.samples.iter().all(Option::is_none));
    assert_eq!(calibration.rtd_adc.fitted_fit.sample_count, 0);
    assert_eq!(calibration.rtd_adc.fitted_fit.gain, 1.0);
    assert_eq!(calibration.rtd_adc.fitted_fit.offset_mv, 0.0);
}

#[test]
fn serial_requests_only_retry_after_an_observed_runtime_ready_marker() {
    let now = Instant::now();
    let deadline = now + Duration::from_millis(100);

    assert!(!should_retry_request_after_runtime_ready(
        true,
        b"boot_stage=display_ready",
        now,
        deadline
    ));
    assert!(!should_retry_request_after_runtime_ready(
        false,
        RUNTIME_READY_BOOT_STAGE.as_bytes(),
        now,
        deadline
    ));
    assert!(!should_retry_request_after_runtime_ready(
        true,
        RUNTIME_READY_BOOT_STAGE.as_bytes(),
        now,
        now
    ));
    assert!(should_retry_request_after_runtime_ready(
        true,
        RUNTIME_READY_BOOT_STAGE.as_bytes(),
        now,
        deadline
    ));
    assert_eq!(
        serial_rpc_timeout(SerialRetryPolicy::ReadOnly),
        SERIAL_READ_ONLY_RPC_TIMEOUT
    );
    assert_eq!(
        serial_rpc_timeout(SerialRetryPolicy::SingleShot),
        SERIAL_RPC_TIMEOUT
    );
}

#[test]
fn write_stage_recoverable_serial_http_errors_are_detected() {
    let broken_pipe = HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_io_failed",
        "Serial I/O failed: Broken pipe",
        true,
    );
    assert!(is_recoverable_write_http_error(&broken_pipe));

    let disappeared_port = HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_io_failed",
        "Serial I/O failed: No such file or directory",
        true,
    );
    assert!(is_recoverable_write_http_error(&disappeared_port));

    let permanent = HttpError::new(
        StatusCode::BAD_GATEWAY,
        "serial_io_failed",
        "Serial I/O failed: Permission denied",
        true,
    );
    assert!(!is_recoverable_write_http_error(&permanent));

    let other_code = HttpError::new(
        StatusCode::BAD_GATEWAY,
        "usb_payload_decode_failed",
        "USB response payload could not be decoded.",
        true,
    );
    assert!(!is_recoverable_write_http_error(&other_code));
}

#[test]
fn serial_request_line_limit_accepts_full_line_and_rejects_overflow() {
    assert!(validate_serial_request_len(&"x".repeat(SERIAL_LINE_LIMIT - 1)).is_ok());
    let error = validate_serial_request_len(&"x".repeat(SERIAL_LINE_LIMIT)).unwrap_err();
    assert_eq!(error.status, StatusCode::BAD_REQUEST);
    assert_eq!(error.error.code, "usb_request_too_large");
}

#[cfg(unix)]
#[test]
fn serial_lock_is_not_reentrant_until_previous_session_is_dropped() {
    let port_path = "/tmp/flux-purr-devd-test-port";
    let deadline = Instant::now() + Duration::from_millis(250);

    let first = SerialPortProcessLock::acquire(port_path, deadline).unwrap();

    let second = match SerialPortProcessLock::acquire(
        port_path,
        Instant::now() + Duration::from_millis(250),
    ) {
        Ok(_) => panic!("second serial lock should time out while first session is alive"),
        Err(error) => error,
    };
    assert_eq!(second.error.code, "serial_lock_timeout");

    drop(first);

    let reopened =
        SerialPortProcessLock::acquire(port_path, Instant::now() + Duration::from_millis(250));
    assert!(reopened.is_ok());
}

#[tokio::test]
async fn cancelled_request_keeps_serial_rpc_locked_until_worker_finishes() {
    let serial_rpc = Arc::new(tokio::sync::Mutex::new(()));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (finish_tx, finish_rx) = tokio::sync::oneshot::channel();
    let worker_lock = serial_rpc.clone();
    let request = tokio::spawn(async move {
        spawn_serial_worker(worker_lock, move || {
            started_tx.send(()).unwrap();
            finish_rx.blocking_recv().unwrap();
        })
        .await
    });

    tokio::time::timeout(Duration::from_secs(1), started_rx)
        .await
        .unwrap()
        .unwrap();
    request.abort();
    tokio::task::yield_now().await;
    assert!(serial_rpc.try_lock().is_err());

    let error =
        spawn_serial_worker_with_timeout(serial_rpc.clone(), Duration::from_millis(25), || ())
            .await
            .unwrap_err();
    assert_eq!(error.error.code, "serial_lock_timeout");

    finish_tx.send(()).unwrap();
    let _serial_rpc = tokio::time::timeout(Duration::from_secs(1), serial_rpc.lock())
        .await
        .unwrap();
}

fn test_artifact_with_file(root: &Path, relative_path: &str, bytes: &[u8]) -> FirmwareArtifact {
    let path = root.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, bytes).unwrap();

    FirmwareArtifact {
        artifact_id: "test-artifact".to_string(),
        name: "Test".to_string(),
        version: "fw/test".to_string(),
        git_sha: "abc".to_string(),
        build_id: "build".to_string(),
        target_chip: "esp32s3".to_string(),
        profile: "release".to_string(),
        features: vec!["web_serial".to_string()],
        protocol: "flux-purr.usb.v1".to_string(),
        files: vec![ArtifactFile {
            kind: "app".to_string(),
            path: relative_path.to_string(),
            sha256: format!("sha256:{:x}", Sha256::digest(bytes)),
            size: bytes.len() as u64,
            flash_address: Some(0x10000),
        }],
    }
}

#[test]
fn wifi_config_acceptance_replaces_stale_error_with_connecting_summary() {
    let current = NetworkSummary {
        state: NetworkState::Error,
        configuration_generation: 4,
        transition_sequence: 11,
        failure_code: Some(NetworkFailureCode::AssociationRejected),
        ssid: Some("Old-Network".to_string()),
        wifi_password_length: 8,
        ip: None,
        gateway: None,
        dns: Vec::new(),
        wifi_rssi: None,
        last_error: Some("WiFi association failed.".to_string()),
    };
    let payload = WifiConfigRequest {
        lease_id: "lease-1".to_string(),
        op: WifiConfigOp::Set,
        ssid: Some("FluxPurr-Lab".to_string()),
        password: Some("secret-pass".to_string()),
        static_ipv4: None,
        telemetry_interval_ms: Some(500),
    };

    let accepted = mock_network_after_wifi_config(&current, &payload);
    assert_eq!(accepted.state, NetworkState::Connecting);
    assert_eq!(accepted.configuration_generation, 5);
    assert_eq!(accepted.transition_sequence, 12);
    assert_eq!(accepted.ssid.as_deref(), Some("FluxPurr-Lab"));
    assert_eq!(accepted.wifi_password_length, 11);
    assert_eq!(accepted.last_error, None);
}

#[test]
fn wifi_cancel_receipt_preserves_credentials_and_reports_idle() {
    let current = NetworkSummary {
        state: NetworkState::Connecting,
        configuration_generation: 4,
        transition_sequence: 11,
        failure_code: None,
        ssid: Some("FluxPurr-Lab".to_string()),
        wifi_password_length: 11,
        ip: Some("192.168.31.42".to_string()),
        gateway: Some("192.168.31.1".to_string()),
        dns: vec!["1.1.1.1".to_string()],
        wifi_rssi: Some(-48),
        last_error: None,
    };
    let payload = WifiConfigRequest {
        lease_id: "lease-1".to_string(),
        op: WifiConfigOp::Cancel,
        ssid: None,
        password: None,
        static_ipv4: None,
        telemetry_interval_ms: None,
    };

    let cancelled = mock_network_after_wifi_config(&current, &payload);

    assert_eq!(cancelled.state, NetworkState::Idle);
    assert_eq!(
        cancelled.configuration_generation,
        current.configuration_generation
    );
    assert_eq!(
        cancelled.transition_sequence,
        current.transition_sequence + 1
    );
    assert_eq!(cancelled.ssid, current.ssid);
    assert_eq!(cancelled.wifi_password_length, current.wifi_password_length);
    assert_eq!(cancelled.ip, None);
    assert_eq!(cancelled.wifi_rssi, None);
}

#[test]
fn network_snapshot_accepts_the_first_receipt_after_a_device_reboot() {
    let mut current = NetworkSummary {
        state: NetworkState::Connected,
        configuration_generation: 9,
        transition_sequence: 48,
        failure_code: None,
        ssid: Some("FluxPurr-Lab".to_string()),
        wifi_password_length: 8,
        ip: Some("192.168.1.42".to_string()),
        gateway: None,
        dns: Vec::new(),
        wifi_rssi: Some(-42),
        last_error: None,
    };
    current.configuration_generation = 1;
    current.transition_sequence = 2;

    let previous = NetworkSummary {
        configuration_generation: 9,
        transition_sequence: 48,
        ..current.clone()
    };
    assert!(current.is_not_older_than(&previous));
}

#[test]
fn lan_bridge_payload_keeps_the_remote_lease_authoritative() {
    let payload = RuntimeConfigRequest {
        lease_id: "local-lease".to_string(),
        target_temp_c: Some(120),
        selected_preset_slot: None,
        presets_c: None,
        active_cooling_enabled: None,
        post_heat_cooling_mode: None,
        heating_fan_guard_mode: None,
        heater_enabled: None,
        manual_pps_enabled: None,
        manual_pps_mv: None,
        manual_pps_ma: None,
        fault_attention_acknowledged: None,
        calibration: None,
        thermal_profile_mode: None,
        thermal_control_profile: None,
    };

    let body = lan_bridge_payload(&payload).unwrap();
    assert_eq!(body["targetTempC"], 120);
    assert!(body.get("leaseId").is_none());
}

#[test]
fn lan_bridge_error_preserves_remote_conflict_status_and_code() {
    let error = lan_bridge_error(lan::LanClientError::RemoteApi {
        status: StatusCode::CONFLICT,
        code: "stale_write".to_string(),
        message: "The control state changed after this client last read it.".to_string(),
        retryable: false,
    });

    assert_eq!(error.status, StatusCode::CONFLICT);
    assert_eq!(error.error.code, "stale_write");
    assert!(!error.error.retryable);
}

#[test]
fn lan_bridge_error_preserves_remote_unauthorized_status_and_code() {
    let error = lan_bridge_error(lan::LanClientError::RemoteApi {
        status: StatusCode::UNAUTHORIZED,
        code: "unauthorized".to_string(),
        message: "The pairing token is not authorized for this device.".to_string(),
        retryable: false,
    });

    assert_eq!(error.status, StatusCode::UNAUTHORIZED);
    assert_eq!(error.error.code, "unauthorized");
    assert!(!error.error.retryable);
}

#[test]
fn eeprom_maintenance_validation_keeps_raw_chunks_bounded_only_by_transport() {
    let raw = EepromMaintenanceRequest {
        lease_id: "lease-1".to_string(),
        op: EepromMaintenanceOp::Write,
        offset: Some(8_160),
        length: None,
        bytes: Some(vec![0, 255, 17, 34]),
    };
    assert!(validate_eeprom_maintenance_request(&raw).is_ok());

    let out_of_range = EepromMaintenanceRequest {
        offset: Some(8_191),
        bytes: Some(vec![1, 2]),
        ..raw.clone()
    };
    assert_eq!(
        validate_eeprom_maintenance_request(&out_of_range)
            .unwrap_err()
            .error
            .code,
        "eeprom_range_invalid"
    );

    let erase_with_content = EepromMaintenanceRequest {
        op: EepromMaintenanceOp::Erase,
        offset: None,
        length: None,
        bytes: Some(vec![0xff]),
        ..raw
    };
    assert_eq!(
        validate_eeprom_maintenance_request(&erase_with_content)
            .unwrap_err()
            .error
            .code,
        "eeprom_erase_payload_invalid"
    );
}

#[test]
fn eeprom_maintenance_write_ack_omits_read_bytes() {
    let response = EepromMaintenanceResponse { bytes: None };
    let value = serde_json::to_value(response).unwrap();

    assert!(value.get("bytes").is_none());
}

#[test]
fn golden_wifi_fixture_has_monotonic_versioned_snapshots() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../fixtures/wifi-provisioning-v2.json")).unwrap();
    for trace in fixture["traces"].as_array().unwrap() {
        let mut previous = 0;
        for snapshot in trace["snapshots"].as_array().unwrap() {
            assert!(snapshot["configurationGeneration"].as_u64().unwrap() > 0);
            let sequence = snapshot["transitionSequence"].as_u64().unwrap();
            assert!(sequence > previous);
            previous = sequence;
        }
    }
}

fn seed_test_bundle(state: &AppState) -> String {
    let output = state.bundle_store.path().join("seed.fluxpurr-fw");
    let bundle = firmware_bundle::build_bundle(
        &output,
        firmware_bundle::BundleIdentity {
            version: "0.1.0".into(),
            source_sha: "e9754917ee23481dd30571fb7a78cb2c486b82a3".into(),
            build_id: "0123456789abcdef".into(),
            channel: firmware_bundle::BundleChannel::Local,
        },
        &vec![0x11; 0x4000],
        include_bytes!("../../../firmware/partitions.bin"),
        &vec![0x33; 0x4000],
    )
    .unwrap();
    let canonical = state.bundle_store.path().join(format!(
        "{}.fluxpurr-fw",
        bundle.bundle_sha256.trim_start_matches("sha256:")
    ));
    fs::rename(output, canonical).unwrap();
    bundle.bundle_sha256
}

#[test]
fn security_info_fails_closed_for_each_protected_state() {
    let safe = RomSecurityInfo {
        rom_mac: "00:11:22:33:44:55".into(),
        secure_boot_enabled: false,
        flash_encryption_enabled: false,
        secure_download_mode_enabled: false,
        response_known: true,
        chip_is_esp32s3: true,
        flash_size_bytes: 4 * 1024 * 1024,
        package_matches: true,
    };
    assert!(safe.validate_for_flash().is_ok());
    for blocked in [
        RomSecurityInfo {
            secure_boot_enabled: true,
            ..safe.clone()
        },
        RomSecurityInfo {
            flash_encryption_enabled: true,
            ..safe.clone()
        },
        RomSecurityInfo {
            secure_download_mode_enabled: true,
            ..safe.clone()
        },
        RomSecurityInfo {
            response_known: false,
            ..safe.clone()
        },
        RomSecurityInfo {
            chip_is_esp32s3: false,
            ..safe.clone()
        },
        RomSecurityInfo {
            flash_size_bytes: 8 * 1024 * 1024,
            ..safe.clone()
        },
        RomSecurityInfo {
            package_matches: false,
            ..safe.clone()
        },
    ] {
        assert!(blocked.validate_for_flash().is_err());
    }
}

#[tokio::test]
async fn recovery_preflight_allows_hot_or_foreign_mock_without_physical_confirmation() {
    let state = AppState::test();
    let artifact_id = seed_test_bundle(&state);
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let result = firmware_operation(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".into()),
        Json(FirmwareOperationRequest {
            lease_id: lease.lease_id,
            artifact_id,
            operation: FirmwareOperation::InstallRecovery,
            dry_run: true,
            approval_token: None,
            confirm: None,
            allow_downgrade: false,
        }),
    )
    .await
    .unwrap()
    .0;

    assert_eq!(result.outcome, "passed");
    assert!(result.approval_token.is_some());
    assert_eq!(result.stages, firmware_preflight_stages());
    assert!(!result.stages.contains(&"erase".to_string()));
    assert!(!result.stages.contains(&"write_segments".to_string()));
    assert_eq!(
        firmware_execution_stages(FirmwareOperation::InstallRecovery),
        vec![
            "authorization",
            "erase",
            "write_segments",
            "rom_md5",
            "reset",
            "runtime_reconnect",
            "runtime_verify",
        ]
    );

    let events = state
        .lock()
        .unwrap()
        .devices
        .get("mock-fp-lab-01")
        .unwrap()
        .events
        .iter()
        .filter(|event| {
            event.kind == "firmware_operation"
                && event.payload["operationId"] == result.operation_id
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 12);
    for (index, event) in events.iter().enumerate() {
        assert_eq!(event.payload["sequence"], (index + 1) as u64);
        assert_eq!(event.payload["phase"], "preflight");
        assert_eq!(event.payload["operation"], "install_recovery");
    }
    assert_eq!(
        events.first().unwrap().payload["event"],
        "operation_started"
    );
    assert_eq!(
        events.last().unwrap().payload["event"],
        "operation_completed"
    );
    assert_eq!(events.last().unwrap().payload["outcome"], "passed");
}

#[tokio::test]
async fn update_preflight_blocks_active_heater_and_high_temperature() {
    let state = AppState::test();
    let artifact_id = seed_test_bundle(&state);
    let lease = state.lease_device("mock-fp-lab-01").unwrap();
    let error = firmware_operation(
        State(state.clone()),
        AxumPath("mock-fp-lab-01".into()),
        Json(FirmwareOperationRequest {
            lease_id: lease.lease_id,
            artifact_id,
            operation: FirmwareOperation::Update,
            dry_run: true,
            approval_token: None,
            confirm: None,
            allow_downgrade: false,
        }),
    )
    .await
    .unwrap_err();

    assert_eq!(error.error.code, "update_temperature_gate");
    let events = state
        .lock()
        .unwrap()
        .devices
        .get("mock-fp-lab-01")
        .unwrap()
        .events
        .iter()
        .filter(|event| event.kind == "firmware_operation")
        .cloned()
        .collect::<Vec<_>>();
    let blocked_stage = events
        .iter()
        .rev()
        .find(|event| event.payload["event"] == "stage_failed")
        .expect("update temperature gate must report a failed preflight stage");
    assert_eq!(blocked_stage.payload["stage"], "transport");
    assert_eq!(blocked_stage.payload["code"], "update_temperature_gate");
    assert_eq!(
        events.last().unwrap().payload["event"],
        "operation_completed"
    );
    assert_eq!(events.last().unwrap().payload["outcome"], "blocked");
    assert!(
        events
            .iter()
            .all(|event| event.payload["stage"] != "rom_reset")
    );
    let state_lock = state.lock().unwrap();
    let status = &state_lock.devices.get("mock-fp-lab-01").unwrap().status;
    assert!(!status.heater_enabled);
    assert_eq!(status.heater_output_percent, 0);
}

#[test]
fn update_runtime_gate_uses_live_identity_and_thermal_facts() {
    let cached = DeviceRecord::mock("mock-fp-lab-01", DeviceTransport::Mock);
    let mut live_status = cached.status.clone();
    live_status.heater_enabled = true;
    live_status.current_temp_c = 31.0;

    let error =
        validate_update_runtime_facts(DeviceTransport::NativeSerial, "fw/v0.18.3", &live_status)
            .unwrap_err();
    assert_eq!(error.error.code, "update_temperature_gate");

    let error =
        validate_update_runtime_facts(DeviceTransport::NativeSerial, "unknown", &cached.status)
            .unwrap_err();
    assert_eq!(error.error.code, "update_identity_required");
}

#[test]
fn firmware_execution_progress_reports_ordered_authoritative_units() {
    let state = AppState::test();
    let mut progress = FirmwareOperationProgress::new(
        &state,
        "mock-fp-lab-01",
        FirmwareOperation::Update,
        "sha256:test",
        false,
    );
    let operation_id = progress.operation_id().to_string();

    progress.operation_started();
    progress.stage_started(
        "write_segments",
        json!({
            "completedUnits": 0,
            "totalUnits": 300,
            "unit": "bytes",
        }),
    );
    progress.stage_progress(
        "write_segments",
        json!({
            "completedUnits": 100,
            "totalUnits": 300,
            "unit": "bytes",
        }),
    );
    progress.stage_completed(
        "write_segments",
        json!({
            "completedUnits": 300,
            "totalUnits": 300,
            "unit": "bytes",
        }),
    );
    progress.operation_completed("verified");

    let events = state
        .lock()
        .unwrap()
        .devices
        .get("mock-fp-lab-01")
        .unwrap()
        .events
        .iter()
        .filter(|event| {
            event.kind == "firmware_operation" && event.payload["operationId"] == operation_id
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 5);
    assert_eq!(events[2].payload["event"], "stage_progress");
    assert_eq!(events[2].payload["completedUnits"], 100);
    assert_eq!(events[2].payload["totalUnits"], 300);
    assert_eq!(events[2].payload["unit"], "bytes");
    assert!(
        events
            .windows(2)
            .all(|window| window[0].payload["sequence"].as_u64()
                < window[1].payload["sequence"].as_u64())
    );
}

#[test]
fn rom_md5_keeps_the_existing_rom_session_until_final_reset() {
    let common = vec![
        "--chip".to_string(),
        "esp32s3".to_string(),
        "--port".to_string(),
        "/dev/cu.usbmodem2111401".to_string(),
        "--non-interactive".to_string(),
    ];

    assert_eq!(
        build_checksum_md5_args(&common, 0x10000, 0x200000),
        vec![
            "checksum-md5",
            "--chip",
            "esp32s3",
            "--port",
            "/dev/cu.usbmodem2111401",
            "--non-interactive",
            "--before",
            "no-reset",
            "--after",
            "no-reset",
            "0x10000",
            "2097152",
        ]
    );
}

#[test]
fn install_status_accepts_null_setup_reason_after_commissioning() {
    let status: InstallStatus = serde_json::from_value(json!({
            "layoutId": "flux-purr.esp32s3fh4r2.factory",
            "layoutVersion": 1,
            "partitionTableSha256": "sha256:fec3c8b36e60ece8780cf75b4125a7171d3a3def71d5ca6ac706f4e431391f1e",
            "persistenceSource": "eeprom",
            "recordState": "valid",
            "recordSequence": 7,
            "commissioningRequired": false,
            "setupReason": null,
            "sensorState": "ready",
            "heaterLocked": false
        }))
        .unwrap();

    assert_eq!(status.setup_reason, None);
}

#[tokio::test]
async fn install_status_endpoint_rejects_non_native_transports() {
    let state = AppState::test();

    let error = device_install_status(
        State(state),
        AxumPath("mock-fp-lab-01".to_string()),
        Query(LeaseQuery { lease_id: None }),
    )
    .await
    .expect_err("mock transport must not expose install status");

    assert_eq!(error.error.code, "native_serial_required");
}
