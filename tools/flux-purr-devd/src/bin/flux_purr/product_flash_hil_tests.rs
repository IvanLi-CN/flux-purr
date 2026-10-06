//! Hardware-only ROM dwell fixture using the normal Developer backup and
//! guarded espflash helpers. It adds no behavior to the shipped flash command.
use super::*;

#[test]
#[ignore = "requires explicitly authorized product port, artifact, and real flashing"]
fn direct_product_flash_rom_dwell_hil() {
    let port = std::env::var("FLUX_PURR_HIL_PORT").expect("exact authorized port");
    let expected_serial = std::env::var("FLUX_PURR_HIL_USB_SERIAL").expect("USB identity");
    let elf = PathBuf::from(std::env::var("FLUX_PURR_HIL_ELF").expect("product ELF"));
    let output = PathBuf::from(std::env::var("FLUX_PURR_HIL_OUTPUT").expect("evidence path"));
    assert!(!output.exists(), "do not overwrite HIL evidence");
    validate_local_elf(&elf).unwrap();
    ensure_real_flash_enabled().unwrap();
    let usb_identity = capture_direct_usb_identity(&port).unwrap();
    assert_eq!(usb_identity.serial_number, expected_serial);
    let _lock = acquire_direct_serial_lock(&port).unwrap();
    let snapshot = read_eeprom_snapshot(&port, Some(&usb_identity)).unwrap();
    let directory = developer_backup_directory().unwrap();
    let backup = developer_backup::write_atomic(&directory, &snapshot).unwrap();
    let program = resolve_espflash_program();
    // The espflash reset command always returns to the application. The
    // existing no-stub ROM probe leaves the ROM session running instead.
    let mut reset_args = rom_download_probe_args(&port);
    let before = reset_args.iter().position(|arg| arg == "--before").unwrap();
    reset_args[before + 1] = "usb-reset".into();
    let reset =
        run_guarded_espflash_command(&program, &reset_args, &port, Some(&usb_identity)).unwrap();
    assert!(
        reset.success,
        "ROM entry was not confirmed; do not count a dwell or write"
    );
    let rom_started = StdInstant::now();
    let rom_started_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    println!("HIL ROM dwell started after verified private EEPROM archive");
    while rom_started.elapsed() < Duration::from_secs(15) {
        ensure_direct_usb_identity(&port, Some(&usb_identity)).unwrap();
        std::thread::sleep(Duration::from_millis(250));
    }
    let dwell_ms = rom_started.elapsed().as_millis();
    let partition_table = embedded_partition_table().unwrap();
    let flash_args = direct_elf_flash_args_with_reset_mode(
        &port,
        partition_table.path(),
        &elf,
        "no-reset",
        "hard-reset",
    )
    .unwrap();
    let flash =
        run_guarded_espflash_command(&program, &flash_args, &port, Some(&usb_identity)).unwrap();
    let result = json!({
        "port": port, "usbSerial": expected_serial, "elf": elf,
        "elfSha256": hex::encode(Sha256::digest(fs::read(&elf).unwrap())),
        "backup": {"disposition": "archived", "path": backup, "bytes": snapshot.len()},
        "romDwellMs": dwell_ms, "romStartedAt": rom_started_at, "reset": reset, "flash": flash,
    });
    fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
    assert!(result["flash"]["success"].as_bool().unwrap());
    assert!(dwell_ms >= 15_000);
}
