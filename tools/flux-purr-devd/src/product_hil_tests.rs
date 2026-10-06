//! Explicit, ignored product HIL through the production devd serial transport.
//! No hardware is accessed by the ordinary test suite.
use super::*;
use std::fs::OpenOptions;
use std::io::Write;

fn record(file: &mut std::fs::File, started: Instant, kind: &str, value: Value) {
    writeln!(
        file,
        "{}",
        json!({
            "atMs": started.elapsed().as_millis(),
            "wallTimeSeconds": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64(),
            "kind": kind,
            "value": value,
        })
    )
    .unwrap();
    file.flush().unwrap();
}

#[test]
#[ignore = "requires owner-authorized exact-port product reconnect HIL"]
fn native_product_reconnect_hil() {
    let port = env::var("FLUX_PURR_HIL_PORT").expect("exact authorized port");
    let output = PathBuf::from(env::var("FLUX_PURR_HIL_OUTPUT").expect("evidence path"));
    let started = Instant::now();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)
        .unwrap();
    let mut uptimes = Vec::new();
    for phase in ["before", "after"] {
        assert!(Path::new(&port).exists(), "authorized port disappeared");
        let child_output = output.with_extension(format!("{phase}.ndjson"));
        let child = std::process::Command::new(env::current_exe().unwrap())
            .args(["native_product_fixed_idle_hil", "--ignored", "--nocapture"])
            .env("FLUX_PURR_HIL_ACTION", "observe")
            .env("FLUX_PURR_HIL_SECONDS", "2")
            .env("FLUX_PURR_HIL_OUTPUT", &child_output)
            .output()
            .unwrap();
        record(
            &mut file,
            started,
            "childOutcome",
            json!({
                "phase": phase, "success": child.status.success(),
                "evidence": child_output,
            }),
        );
        assert!(
            child.status.success(),
            "{phase} exact-port identity/status failed; inspect child evidence"
        );
        let statuses: Vec<Value> = std::fs::read_to_string(&child_output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .filter(|value| value["kind"] == "status")
            .collect();
        let status = &statuses.last().expect("observed product status")["value"];
        assert_eq!(status["pdContractKind"], "fixed");
        assert_eq!(status["pdContractMv"], 5000);
        assert_eq!(status["heaterPhysicalOutputPercent"], 0);
        assert_eq!(status["fanEnabled"], false);
        uptimes.push(status["uptimeSeconds"].as_u64().unwrap());
        if phase == "before" {
            // The child process has exited, so all host descriptors are closed.
            std::thread::sleep(Duration::from_secs(25));
        }
    }
    assert!(
        uptimes[1] >= uptimes[0] + 25,
        "product restarted during host absence"
    );
    record(
        &mut file,
        started,
        "reconnectPassed",
        json!({"uptimes": uptimes}),
    );
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "read-only boot diagnosis on an explicitly authorized exact port"]
fn native_product_boot_log_hil() {
    use std::io::Read;
    let port = env::var("FLUX_PURR_HIL_PORT").expect("exact owner-authorized port");
    let expected_serial = env::var("FLUX_PURR_HIL_USB_SERIAL").expect("expected USB serial");
    let output = env::var("FLUX_PURR_HIL_OUTPUT").expect("HIL evidence path");
    let started = Instant::now();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let _lock =
        SerialPortProcessLock::acquire(&port, Instant::now() + Duration::from_secs(2)).unwrap();
    let identity = loop {
        if let Ok(identity) = serial_port_usb_identity(&port) {
            assert_eq!(identity.serial_number, expected_serial);
            break identity;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "exact port unavailable"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    record(
        &mut file,
        started,
        "usbIdentity",
        json!({"vid": identity.vid, "pid": identity.pid, "serialNumber": identity.serial_number}),
    );
    let mut serial = match open_raw_usb_serial_jtag_port(&port) {
        Ok(serial) => serial,
        Err(error) => {
            record(&mut file, started, "openFailed", json!(error.to_string()));
            return;
        }
    };
    let mut bytes = [0_u8; 4096];
    while started.elapsed() < Duration::from_secs(15) {
        if !Path::new(&port).exists() {
            record(&mut file, started, "authorizedPortDisappeared", json!(true));
            break;
        }
        match serial.read(&mut bytes) {
            Ok(n) if n > 0 => record(
                &mut file,
                started,
                "bootOutput",
                json!(String::from_utf8_lossy(&bytes[..n])),
            ),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => {
                record(&mut file, started, "readFailed", json!(error.to_string()));
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
#[ignore = "requires owner-authorized product hardware and explicit HIL environment"]
async fn native_product_fixed_idle_hil() {
    let port = env::var("FLUX_PURR_HIL_PORT").expect("exact owner-authorized port");
    let expected_serial = env::var("FLUX_PURR_HIL_USB_SERIAL").expect("expected USB serial");
    let expected_device = env::var("FLUX_PURR_HIL_DEVICE_ID").expect("expected product identity");
    let output = env::var("FLUX_PURR_HIL_OUTPUT").expect("HIL evidence path");
    let action = env::var("FLUX_PURR_HIL_ACTION").expect("explicit HIL action");
    assert!(matches!(
        action.as_str(),
        "observe"
            | "pps"
            | "prepare"
            | "cancel"
            | "fan-on"
            | "fan-off"
            | "heater-on"
            | "heater-off"
            | "sequence"
            | "product-sequence"
            | "wifi-idle"
    ));
    let seconds: u64 = env::var("FLUX_PURR_HIL_SECONDS")
        .unwrap_or_else(|_| "15".into())
        .parse()
        .unwrap();
    assert!((1..=600).contains(&seconds));
    let usb_identity = serial_port_usb_identity(&port).expect("authorized port must exist");
    assert_eq!(usb_identity.serial_number, expected_serial);
    let state = AppState::new(AppConfig {
        serial_port: Some(PathBuf::from(&port)),
        ..AppConfig::default()
    });
    let target = scan_serial_devices(Some(Path::new(&port))).pop().unwrap();
    state
        .lock()
        .unwrap()
        .devices
        .insert(target.id.clone(), target.clone());
    let lease = state.lease_device(&target.id).unwrap();
    let started = Instant::now();
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .unwrap();
    let identity = serial_request_payload_with_identity::<Identity>(
        &state,
        &target,
        "get_identity",
        "identity",
        Some(&usb_identity),
    )
    .await;
    if let Err(error) = &identity {
        record(
            &mut file,
            started,
            "identityExchangeFailure",
            json!({"error": error.error}),
        );
        record_serial_diagnostics(&state, &target, &mut file, started);
    }
    let identity = identity.unwrap();
    assert_eq!(identity.device_id, expected_device);
    assert_eq!(identity.firmware_kind, Some(FirmwareKind::Product));
    record(
        &mut file,
        started,
        "identity",
        serde_json::to_value(identity).unwrap(),
    );
    let mut observer = ProductObserver {
        state: &state,
        target: &target,
        identity: &usb_identity,
        lease: &lease.lease_id,
        file: &mut file,
        started,
    };
    match action.as_str() {
        "product-sequence" => product_sequence(&mut observer).await,
        "wifi-idle" => wifi_idle_sequence(&mut observer).await,
        _ => observer.run_actions(&action, seconds).await.unwrap(),
    }
}

async fn wifi_idle_sequence(observer: &mut ProductObserver<'_>) {
    let cycles: u8 = env::var("FLUX_PURR_HIL_WIFI_CYCLES")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .unwrap();
    assert!((1..=10).contains(&cycles));
    let mut outcome = Ok(());
    for cycle in 0..cycles {
        record(observer.file, observer.started, "wifiCycle", json!(cycle));
        outcome = observer.wifi_idle_load().await;
        if outcome.is_err() {
            break;
        }
    }
    record(
        observer.file,
        observer.started,
        "wifiIdleOutcome",
        json!({"error": outcome.as_ref().err()}),
    );
    record_serial_diagnostics(
        observer.state,
        observer.target,
        observer.file,
        observer.started,
    );
    outcome.unwrap();
}

struct ProductObserver<'a> {
    state: &'a AppState,
    target: &'a DeviceRecord,
    identity: &'a UsbSerialIdentity,
    lease: &'a str,
    file: &'a mut std::fs::File,
    started: Instant,
}

impl ProductObserver<'_> {
    async fn run_actions(&mut self, action: &str, seconds: u64) -> Result<(), String> {
        let actions = if action == "sequence" {
            vec![
                "observe",
                "fan-on",
                "prepare",
                "pps",
                "prepare",
                "fan-off",
                "cancel",
                "pps",
                "prepare",
                "heater-on",
                "cancel",
                "heater-on",
                "heater-off",
                "observe",
            ]
        } else {
            vec![action]
        };
        for action in actions {
            record(self.file, self.started, "phase", json!(action));
            self.apply_action(action).await?;
            self.observe(seconds).await?;
        }
        Ok(())
    }

    async fn apply_action(&mut self, action: &str) -> Result<(), String> {
        match action {
            "prepare" => {
                self.preparation("prepare_flash").await?;
            }
            "cancel" => {
                self.preparation("cancel_flash_preparation").await?;
            }
            "observe" => {}
            _ => {
                let body = action_runtime_request(action);
                // Negative admission cases deliberately retain their recorded response.
                let _ = self.runtime(body).await;
            }
        }
        Ok(())
    }

    async fn verify_preparation_hold(&mut self) -> Result<(), String> {
        self.runtime(json!({"manualPpsEnabled": false, "postHeatCoolingMode": "normal"}))
            .await?;
        self.preparation("prepare_flash").await?;
        self.ready(40).await?;
        self.preparation("prepare_flash").await?;
        self.ready(3).await?;
        for request in [
            json!({"manualPpsEnabled": true, "manualPpsMv": 12000, "manualPpsMa": 3000}),
            json!({"heaterEnabled": true}),
        ] {
            if self.runtime(request).await.err().as_deref() != Some("flash_preparation_active") {
                return Err("hold admitted an output request".into());
            }
        }
        self.preparation("cancel_flash_preparation").await?;
        self.runtime(json!({"manualPpsEnabled": true, "manualPpsMv": 12000, "manualPpsMa": 3000}))
            .await?;
        let (status, _) = self.observe(5).await?;
        if status["pdContractKind"] != "pps" {
            return Err("cancel blocked fresh PPS after confirmed disarm".into());
        }
        self.preparation("prepare_flash").await?;
        self.ready(40).await?;
        self.preparation("cancel_flash_preparation").await?;

        Ok(())
    }

    async fn verify_ordinary_pps_exit(&mut self) -> Result<(), String> {
        record(self.file, self.started, "phase", json!("ordinaryPpsExit"));
        self.runtime(json!({"manualPpsEnabled": true, "manualPpsMv": 12000, "manualPpsMa": 3000}))
            .await?;
        let (status, _) = self.observe(5).await?;
        if status["pdContractKind"] != "pps" {
            return Err("ordinary manual PPS did not start".into());
        }
        self.runtime(json!({"manualPpsEnabled": false})).await?;
        self.ready(40).await?;
        let (status, preparation) = self.read().await?;
        validate_ready_snapshot(&status, &preparation)?;
        let source_mv = self.source_voltage()?;
        record(
            self.file,
            self.started,
            "ordinaryPpsExitPassed",
            json!({"status": status, "preparation": preparation, "sourceMv": source_mv}),
        );
        Ok(())
    }

    async fn stop_hil_heating(&mut self, prepare: bool) -> Result<(), String> {
        if prepare {
            self.preparation("prepare_flash").await?;
        } else {
            self.runtime(json!({"heaterEnabled": false, "manualPpsEnabled": false,
                "calibration": {"mode": "off", "heaterEnabled": false}}))
                .await?;
        }
        Ok(())
    }

    async fn heat_and_cool(&mut self, prepare: bool) -> Result<(), String> {
        record(
            self.file,
            self.started,
            "phase",
            json!(if prepare {
                "preparationHeatCool"
            } else {
                "ordinaryHeatCool"
            }),
        );
        // The 21V case exercises PPS current limiting with the cold heater as
        // a load. This temporary manual-output fixture does not persist a
        // thermal model or claim ordinary production heater arming.
        let heating_mv = env::var("FLUX_PURR_HIL_HEAT_MV")
            .unwrap_or_else(|_| "8000".into())
            .parse::<u16>()
            .map_err(|error| error.to_string())?;
        if !matches!(heating_mv, 8000 | 21000) {
            return Err("HIL heating point must be 8000 or 21000mV".into());
        }
        self.runtime(
            json!({"manualPpsEnabled": true, "manualPpsMv": heating_mv, "manualPpsMa": 3000}),
        )
        .await?;
        let (status, _) = self.observe(5).await?;
        if status["pdContractKind"] != "pps" || status["pdContractMv"] != heating_mv {
            return Err("calibration working power unavailable".into());
        }
        let initial_uptime = status["uptimeSeconds"].as_u64().unwrap();
        self.runtime(json!({"targetTempC": 60, "calibration": {"mode": "heater_curve", "ppsEnabled": true, "ppsMv": heating_mv, "heaterEnabled": true}})).await?;
        let deadline = Instant::now() + Duration::from_secs(180);
        let mut heated = false;
        loop {
            let (status, preparation) = self.read().await?;
            if status["pdContractKind"] != "pps"
                || status["pdContractMv"] != heating_mv
                || status["uptimeSeconds"].as_u64().unwrap() < initial_uptime
            {
                return Err("loaded PPS contract was lost or device restarted".into());
            }
            heated |= status["heaterPhysicalOutputPercent"].as_u64().unwrap_or(0) > 0;
            if heated && preparation["pdFixedOrDefault"] == true {
                return Err("preparation falsely ready during actual heat".into());
            }
            if status["currentTempC"].as_f64().unwrap_or(0.0) >= 55.0 {
                break;
            }
            if Instant::now() >= deadline || status["calibration"]["mode"] != "heater_curve" {
                return Err("calibration heating did not reach bounded HIL target".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if !heated {
            return Err("no physical heat observed".into());
        }
        self.stop_hil_heating(prepare).await?;
        let (status, _) = self.read().await?;
        if status["heaterPhysicalOutputPercent"] != 0 {
            return Err("stop failed to revoke actual heat".into());
        }
        self.await_natural_cooling().await?;
        let (status, preparation) = self.read().await?;
        validate_ready_snapshot(&status, &preparation)?;
        let source_mv = self.source_voltage()?;
        record(
            self.file,
            self.started,
            if prepare {
                "preparationCoolingPassed"
            } else {
                "ordinaryCoolingPassed"
            },
            json!({"status": status, "preparation": preparation, "sourceMv": source_mv}),
        );
        if prepare {
            self.preparation("cancel_flash_preparation").await?;
        }
        Ok(())
    }

    async fn await_natural_cooling(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(240);
        let mut cooled = false;
        loop {
            let (status, preparation) = self.read().await?;
            if status["heaterPhysicalOutputPercent"] != 0 {
                return Err("heat replayed during cooling".into());
            }
            cooled |= status["fanEnabled"] == true;
            validate_cooling_snapshot(&status, &preparation)?;
            if preparation["pdFixedOrDefault"] == true
                && preparation["heating"] == false
                && preparation["cooling"] == false
                && status["fanEnabled"] == false
                && status["pdContractKind"] == "fixed"
                && status["pdContractMv"] == 5000
                && status["pdContractCurrentMa"] == 1000
            {
                break;
            }
            if Instant::now() >= deadline {
                return Err("natural cooling did not finish in Fixed 5V".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        if !cooled {
            return Err("no physical cooling observed".into());
        }
        Ok(())
    }

    async fn exercise_product(&mut self) -> Result<(), String> {
        self.prepare_ordinary_sequence().await?;
        self.verify_ordinary_pps_exit().await?;
        self.heat_and_cool(false).await?;
        self.verify_preparation_hold().await?;
        self.heat_and_cool(true).await?;
        self.wifi_idle_load().await?;
        record(
            self.file,
            self.started,
            "productExerciseCompleted",
            json!(true),
        );
        Ok(())
    }

    async fn prepare_ordinary_sequence(&mut self) -> Result<(), String> {
        // Required cooling uses Normal and product_sequence restores the
        // original mode. A previous run can leave a warm plate.
        self.cool_warm_plate().await?;
        self.runtime(json!({"manualPpsEnabled": false, "postHeatCoolingMode": "normal"}))
            .await?;
        self.ready(40).await
    }

    async fn cool_warm_plate(&mut self) -> Result<(), String> {
        let (status, _) = self.read().await?;
        if status["currentTempC"].as_f64().unwrap_or(100.0) <= 40.0 {
            return Ok(());
        }
        self.runtime(json!({"postHeatCoolingMode": "on"})).await?;
        let deadline = Instant::now() + Duration::from_secs(240);
        loop {
            let (status, _) = self.read().await?;
            if status["currentTempC"].as_f64().unwrap_or(100.0) <= 40.0 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("warm plate did not cool before the ordinary HIL phase".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn read(&mut self) -> Result<(Value, Value), String> {
        require_usb_serial_identity(
            native_port_path(self.target)
                .map_err(|e| e.error.message)?
                .as_str(),
            Some(self.identity),
        )
        .map_err(|e| e.error.message)?;
        let status: Value = serial_request_payload_with_identity(
            self.state,
            self.target,
            "get_status",
            "status",
            Some(self.identity),
        )
        .await
        .map_err(|e| e.error.message)?;
        record(self.file, self.started, "status", status.clone());
        let preparation: Value = serial_request_payload_with_identity(
            self.state,
            self.target,
            "get_flash_preparation",
            "flashPreparation",
            Some(self.identity),
        )
        .await
        .map_err(|e| e.error.message)?;
        record(
            self.file,
            self.started,
            "flashPreparation",
            preparation.clone(),
        );
        Ok((status, preparation))
    }

    async fn runtime(&mut self, mut body: Value) -> Result<Value, String> {
        body["leaseId"] = json!(self.lease);
        let request = serde_json::from_value(body.clone()).map_err(|e| e.to_string())?;
        let response = serial_runtime_config_with_identity(
            self.state,
            self.target,
            &request,
            Some(self.identity),
        )
        .await;
        match response {
            Ok(status) => {
                let status = serde_json::to_value(status).unwrap();
                record(
                    self.file,
                    self.started,
                    "runtime",
                    json!({"request": body, "status": status}),
                );
                Ok(status)
            }
            Err(error) => {
                record(
                    self.file,
                    self.started,
                    "runtimeRejected",
                    json!({"request": body, "error": error.error}),
                );
                Err(error.error.code)
            }
        }
    }

    async fn preparation(&mut self, op: &'static str) -> Result<Value, String> {
        let result: Value = serial_request_payload_with_identity(
            self.state,
            self.target,
            op,
            "flashPreparation",
            Some(self.identity),
        )
        .await
        .map_err(|e| e.error.message)?;
        record(self.file, self.started, op, result.clone());
        Ok(result)
    }

    async fn observe(&mut self, seconds: u64) -> Result<(Value, Value), String> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            let observation = self.read().await?;
            if Instant::now() >= deadline {
                return Ok(observation);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    fn source_voltage(&mut self) -> Result<f64, String> {
        let url = env::var("FLUX_PURR_HIL_SOURCE_URL").map_err(|e| e.to_string())?;
        let output = std::process::Command::new("isolapurr")
            .args(["--json", "--no-auto-start", "power", "show", "--url", &url])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("independent source read failed".into());
        }
        let source: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
        record(self.file, self.started, "source", source.clone());
        if source["diagnostics"]["display"]["mode"]["kind"] != "pd"
            || source["diagnostics"]["sw2303_request"]["mv"] != 5000
            || source["diagnostics"]["usb_c_power_enabled"] != true
        {
            return Err("source did not confirm enabled PD Fixed 5V".into());
        }
        let actual = &source["diagnostics"]["usb_c_actual"];
        if actual["status"] != "ok" {
            return Err("independent VBUS sample unavailable".into());
        }
        actual["voltage_mv"]
            .as_f64()
            .ok_or("independent VBUS missing".into())
    }

    async fn ready(&mut self, seconds: u64) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        loop {
            let (_, preparation) = self.read().await?;
            if preparation["heating"] == false
                && preparation["cooling"] == false
                && preparation["pdFixedOrDefault"] == true
            {
                // Read actual outputs after the readiness response. The preceding
                // get_status can still describe an intermediate PPS ramp step.
                let (status, confirmation) = self.read().await?;
                validate_ready_snapshot(&status, &confirmation)?;
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err("product preparation did not become ready".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    async fn observe_wifi_radio(
        &mut self,
        request: &WifiConfigRequest,
        before: &Value,
    ) -> Result<(), String> {
        let receipt = serial_wifi_config(self.state, self.target, request)
            .await
            .map_err(|e| e.error.message)?;
        // Record only the public receipt, never credentials or request bytes.
        record(
            self.file,
            self.started,
            "wifiRadioLoadStarted",
            serde_json::to_value(receipt).unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut uptime = before["uptimeSeconds"].as_u64().ok_or("uptime missing")?;
        let mut radio_active = false;
        loop {
            let (status, _) = self.read().await?;
            let next_uptime = status["uptimeSeconds"].as_u64().ok_or("uptime missing")?;
            if next_uptime < uptime {
                return Err("product reset during WiFi load".into());
            }
            uptime = next_uptime;
            if status["pdLastI2cError"] != before["pdLastI2cError"]
                || status["pdLastProtocolFault"] != before["pdLastProtocolFault"]
            {
                return Err("new PD transport fault during WiFi load".into());
            }
            radio_active |= matches!(
                status["network"]["state"].as_str(),
                Some("connecting" | "retrying" | "failed")
            );
            if status["pdContractKind"] != "fixed"
                || status["pdContractMv"] != 5000
                || status["heaterPhysicalOutputPercent"] != 0
                || status["fanEnabled"] != false
            {
                return Err("WiFi load left Fixed 5V idle power".into());
            }
            if Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        let vbus = self.source_voltage()?;
        if !radio_active || (vbus - 5000.0).abs() > 250.0 {
            return Err("independent Fixed 5V WiFi load observation failed".into());
        }
        Ok::<(), String>(())
    }

    async fn wifi_idle_load(&mut self) -> Result<(), String> {
        let (before, _) = self.read().await?;
        if before["network"]["state"] != "disabled"
            || !before["network"]["ssid"].is_null()
            || before["network"]["wifiPasswordLength"] != 0
        {
            return Err(
                "WiFi HIL requires the observed empty original network configuration".into(),
            );
        }
        let request = WifiConfigRequest {
            lease_id: self.lease.to_string(),
            op: WifiConfigOp::Set,
            ssid: Some(format!("fp-hil-{}", now_millis())),
            password: Some(format!("hil-{}-radio", now_millis())),
            static_ipv4: None,
            telemetry_interval_ms: None,
        };
        let outcome = self.observe_wifi_radio(&request, &before).await;
        let clear = WifiConfigRequest {
            op: WifiConfigOp::Clear,
            ssid: None,
            password: None,
            ..request
        };
        let restored = serial_wifi_config(self.state, self.target, &clear)
            .await
            .map_err(|e| e.error.message)?;
        record(
            self.file,
            self.started,
            "wifiConfigurationRestored",
            serde_json::to_value(restored).unwrap(),
        );
        let (status, _) = self.observe(3).await?;
        if status["network"]["state"] != "disabled"
            || !status["network"]["ssid"].is_null()
            || status["network"]["wifiPasswordLength"] != 0
        {
            return Err("original empty WiFi configuration was not restored".into());
        }
        if status["pdContractKind"] != "fixed"
            || status["pdContractMv"] != 5000
            || status["pdLastI2cError"] != before["pdLastI2cError"]
            || status["pdLastProtocolFault"] != before["pdLastProtocolFault"]
            || status["uptimeSeconds"].as_u64() < before["uptimeSeconds"].as_u64()
        {
            return Err("WiFi restoration lost the idle contract or added a PD fault".into());
        }
        outcome
    }
}

async fn product_sequence(observer: &mut ProductObserver<'_>) {
    let original = serial_calibration_get(observer.state, observer.target)
        .await
        .unwrap();
    record(
        observer.file,
        observer.started,
        "originalCalibration",
        serde_json::to_value(&original).unwrap(),
    );
    let (runtime, _) = observer.read().await.unwrap();
    assert_eq!(runtime["heaterEnabled"], false);
    assert_eq!(runtime["fanEnabled"], false);
    assert_eq!(runtime["manualPpsEnabled"], false);
    record(
        observer.file,
        observer.started,
        "originalRuntime",
        runtime.clone(),
    );
    let outcome = observer.exercise_product().await;
    record(
        observer.file,
        observer.started,
        "productSequenceOutcome",
        json!({"error": outcome.as_ref().err()}),
    );
    let _ = observer.preparation("prepare_flash").await;
    let _ = observer.preparation("cancel_flash_preparation").await;
    let restored = observer
        .runtime(json!({
            "heaterEnabled": false,
            "targetTempC": runtime["targetTempC"],
            "postHeatCoolingMode": runtime["postHeatCoolingMode"],
            "heatingFanGuardMode": runtime["heatingFanGuardMode"],
        }))
        .await;
    let calibration = serial_calibration_get(observer.state, observer.target)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(calibration).unwrap(),
        serde_json::to_value(original).unwrap(),
        "product HIL changed persistent calibration"
    );
    record(
        observer.file,
        observer.started,
        "originalCalibrationUnchanged",
        json!(true),
    );
    assert!(restored.is_ok(), "runtime restoration failed");
    outcome.unwrap();
    observer.ready(40).await.unwrap();
    let (final_status, preparation) = observer.observe(3).await.unwrap();
    validate_ready_snapshot(&final_status, &preparation).unwrap();
    assert_eq!(final_status["pdContractMv"], 5000);
    assert_eq!(final_status["pdContractCurrentMa"], 1000);
    assert!((observer.source_voltage().unwrap() - 5000.0).abs() <= 250.0);
    record(
        observer.file,
        observer.started,
        "restoredProductFixed5v",
        json!(true),
    );
    record(
        observer.file,
        observer.started,
        "productSequencePassed",
        json!(true),
    );
    record_serial_diagnostics(
        observer.state,
        observer.target,
        observer.file,
        observer.started,
    );
}

fn record_serial_diagnostics(
    state: &AppState,
    target: &DeviceRecord,
    file: &mut std::fs::File,
    started: Instant,
) {
    if let Ok(inner) = state.lock()
        && let Some(device) = inner.devices.get(&target.id)
    {
        for event in &device.events {
            if event.kind == "serial" {
                record(
                    file,
                    started,
                    "serialDiagnostic",
                    serde_json::to_value(event).unwrap(),
                );
            }
        }
    }
}

fn action_runtime_request(action: &str) -> Value {
    match action {
        "pps" => json!({"manualPpsEnabled": true, "manualPpsMv": 12000, "manualPpsMa": 3000}),
        "fan-on" | "fan-off" => json!({"activeCoolingEnabled": action == "fan-on"}),
        "heater-on" | "heater-off" => {
            json!({"heaterEnabled": action == "heater-on", "targetTempC": 60, "postHeatCoolingMode": "normal"})
        }
        _ => unreachable!(),
    }
}

fn validate_ready_snapshot(status: &Value, preparation: &Value) -> Result<(), String> {
    if status["heaterPhysicalOutputPercent"] != 0
        || status["fanEnabled"] != false
        || status["pdContractKind"] != "fixed"
        || preparation["heating"] != false
        || preparation["cooling"] != false
        || preparation["pdFixedOrDefault"] != true
    {
        return Err("preparation reported readiness inconsistent with physical status".into());
    }
    Ok(())
}

fn validate_cooling_snapshot(status: &Value, preparation: &Value) -> Result<(), String> {
    if preparation["cooling"] == true && preparation["pdFixedOrDefault"] != false {
        return Err("preparation ignored physical cooling".into());
    }
    if status["fanEnabled"] == true && status["pdContractMv"].as_u64().unwrap_or(0) < 5500 {
        return Err("fan enabled on standby voltage".into());
    }
    Ok(())
}
