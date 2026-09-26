use super::*;
use flux_purr_devd::PRODUCT_BUILD_ID;
use serialport::{FlowControl, SerialPortType, UsbPortInfo};
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

const IDENTITY_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const RAM_ELF_RELATIVE_PATH: &str =
    "firmware/ram-bringup/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedFirmware {
    Product,
    RamBringup,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityBody {
    #[serde(default)]
    firmware_kind: Option<String>,
    #[serde(default)]
    build_id: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdentityFrame {
    #[serde(rename = "type")]
    frame_type: Option<String>,
    #[serde(default)]
    firmware_kind: Option<String>,
    #[serde(default)]
    identity: Option<IdentityBody>,
}

#[derive(Debug, Clone)]
struct ObservedIdentity {
    firmware: ObservedFirmware,
    build_id: Option<String>,
    capabilities: Vec<String>,
}

pub(crate) fn execute_ram_run(
    command: RamRunCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        RamRunCommand::Preview(args) => run_ram_operation(
            &args.port,
            args.elf.as_deref(),
            args.reload,
            args.scenario.op(),
        ),
        RamRunCommand::Test(args) => {
            run_ram_operation(&args.port, args.elf.as_deref(), args.reload, args.test.op())
        }
        RamRunCommand::Exit(args) => exit_ram(&args.port),
    }
}

fn run_ram_operation(
    port: &str,
    elf: Option<&Path>,
    reload: bool,
    op: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_exact_ram_port(port)?;
    let observed = read_identity(port).unwrap_or(ObservedIdentity {
        firmware: ObservedFirmware::Unknown,
        build_id: None,
        capabilities: Vec::new(),
    });
    let matching_ram = observed.firmware == ObservedFirmware::RamBringup
        && observed.build_id.as_deref() == Some(PRODUCT_BUILD_ID)
        && observed
            .capabilities
            .iter()
            .any(|capability| capability == op);
    let identity = if reload || !matching_ram {
        let elf = elf.map(Path::to_path_buf).unwrap_or_else(default_ram_elf);
        validate_local_elf(&elf)?;
        validate_ram_elf(&elf)?;
        load_ram_elf(port, &elf)?;
        read_identity_with_retry(port)?
    } else {
        observed
    };
    verify_ram_identity(&identity, op)?;
    send_ram_request(port, op)
}

fn exit_ram(port: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_exact_ram_port(port)?;
    let identity = read_identity_with_retry(port)?;
    verify_ram_identity(&identity, "exit")?;
    send_ram_request(port, "exit")
}

fn default_ram_elf() -> PathBuf {
    flux_purr_repo_root().join(RAM_ELF_RELATIVE_PATH)
}

const ELF_PT_LOAD: u32 = 1;
const ELF_MACHINE_XTENSA: u16 = 94;
const RAM_VECTORS: (u64, u64) = (0x4037_8000, 0x4037_8400);
const RAM_IRAM: (u64, u64) = (0x4037_8400, 0x403b_8400);
const RAM_DRAM: (u64, u64) = (0x3fc8_8000, 0x3fce_8000);
const RAM_RESERVED: (u64, u64) = (0x3fce_8000, 0x3fce_d710);
const RAM_FLASH_WINDOWS: [(u64, u64); 2] = [(0x4200_0000, 0x4400_0000), (0x3c00_0000, 0x3d00_0000)];

pub(crate) fn validate_ram_elf(
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data = fs::read(path)?;
    if data.get(0..4) != Some(b"\x7fELF") {
        return Err(format!("RAM artifact is not an ELF: {}", path.display()).into());
    }
    let class = *data.get(4).ok_or_else(|| {
        format!(
            "RAM artifact has a truncated ELF header: {}",
            path.display()
        )
    })?;
    if data.get(5) != Some(&1) {
        return Err("RAM ELF must use little-endian encoding".into());
    }
    let machine = read_u16(&data, 18)?;
    if machine != ELF_MACHINE_XTENSA {
        return Err(format!("RAM ELF machine {machine} is not Xtensa").into());
    }
    let (entry, phoff, phentsize, phnum) = match class {
        1 => (
            read_u32(&data, 24)? as u64,
            read_u32(&data, 28)? as u64,
            read_u16(&data, 42)? as u64,
            read_u16(&data, 44)? as u64,
        ),
        2 => (
            read_u64(&data, 24)?,
            read_u64(&data, 32)?,
            read_u16(&data, 54)? as u64,
            read_u16(&data, 56)? as u64,
        ),
        _ => return Err(format!("unsupported RAM ELF class {class}").into()),
    };
    let expected_phentsize = if class == 1 { 32 } else { 56 };
    if phentsize < expected_phentsize {
        return Err("RAM ELF program header entry is too small".into());
    }
    let mut load_count = 0u32;
    let mut iram_bytes = 0u64;
    let mut dram_bytes = 0u64;
    for index in 0..phnum {
        let offset = phoff
            .checked_add(
                index
                    .checked_mul(phentsize)
                    .ok_or("RAM ELF program header overflow")?,
            )
            .ok_or("RAM ELF program header overflow")?;
        let offset = usize::try_from(offset).map_err(|_| "RAM ELF program header is too large")?;
        let p_type = read_u32(&data, offset)?;
        if p_type != ELF_PT_LOAD {
            continue;
        }
        load_count += 1;
        let (p_offset, vaddr, paddr, filesz, memsz) = if class == 1 {
            (
                read_u32(&data, offset + 4)? as u64,
                read_u32(&data, offset + 8)? as u64,
                read_u32(&data, offset + 12)? as u64,
                read_u32(&data, offset + 16)? as u64,
                read_u32(&data, offset + 20)? as u64,
            )
        } else {
            (
                read_u64(&data, offset + 8)?,
                read_u64(&data, offset + 16)?,
                read_u64(&data, offset + 24)?,
                read_u64(&data, offset + 32)?,
                read_u64(&data, offset + 40)?,
            )
        };
        if filesz > memsz {
            return Err(format!("RAM ELF segment {index} has p_filesz > p_memsz").into());
        }
        if paddr != vaddr {
            return Err(format!("RAM ELF segment {index} has a non-identity load address").into());
        }
        let end = paddr
            .checked_add(memsz)
            .ok_or_else(|| format!("RAM ELF segment {index} address overflow"))?;
        let file_end = p_offset
            .checked_add(filesz)
            .ok_or_else(|| format!("RAM ELF segment {index} file range overflow"))?;
        if usize::try_from(file_end).map_or(true, |end| end > data.len()) {
            return Err(format!("RAM ELF segment {index} exceeds the artifact").into());
        }
        if RAM_FLASH_WINDOWS
            .iter()
            .any(|window| ranges_overlap(paddr, end, *window))
        {
            return Err(format!("RAM ELF segment {index} maps to flash").into());
        }
        if ranges_overlap(paddr, end, RAM_RESERVED) {
            return Err(format!("RAM ELF segment {index} overlaps reserved memory").into());
        }
        if paddr == RAM_VECTORS.0 && end <= RAM_VECTORS.1 {
            continue;
        }
        if range_contained(paddr, end, RAM_IRAM) {
            iram_bytes = iram_bytes
                .checked_add(memsz)
                .ok_or("RAM ELF IRAM budget overflow")?;
        } else if range_contained(paddr, end, RAM_DRAM) {
            dram_bytes = dram_bytes
                .checked_add(memsz)
                .ok_or("RAM ELF DRAM budget overflow")?;
        } else {
            return Err(format!("RAM ELF segment {index} is outside internal RAM").into());
        }
    }
    if load_count == 0 {
        return Err("RAM ELF contains no PT_LOAD segments".into());
    }
    if iram_bytes > RAM_IRAM.1 - RAM_IRAM.0 {
        return Err("RAM ELF IRAM budget exceeded".into());
    }
    if dram_bytes > RAM_DRAM.1 - RAM_DRAM.0 {
        return Err("RAM ELF DRAM budget exceeded".into());
    }
    if !(RAM_VECTORS.0..RAM_IRAM.1).contains(&entry) {
        return Err("RAM ELF entry point is outside internal RAM".into());
    }
    Ok(())
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, Box<dyn std::error::Error + Send + Sync>> {
    let end = offset.checked_add(2).ok_or("truncated RAM ELF field")?;
    let bytes = data.get(offset..end).ok_or("truncated RAM ELF field")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let end = offset.checked_add(4).ok_or("truncated RAM ELF field")?;
    let bytes = data.get(offset..end).ok_or("truncated RAM ELF field")?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let end = offset.checked_add(8).ok_or("truncated RAM ELF field")?;
    let bytes = data.get(offset..end).ok_or("truncated RAM ELF field")?;
    Ok(u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn ranges_overlap(start: u64, end: u64, window: (u64, u64)) -> bool {
    start < window.1 && end > window.0
}

fn range_contained(start: u64, end: u64, window: (u64, u64)) -> bool {
    window.0 <= start && end <= window.1
}

fn validate_exact_ram_port(port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(port)?;
    let path = Path::new(port);
    if !path.exists() {
        return Err(format!("authorized serial port is unavailable: {port}").into());
    }
    let enumerated = serialport::available_ports()?
        .into_iter()
        .find(|candidate| candidate.port_name == port)
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    if !matches!(
        enumerated.port_type,
        SerialPortType::UsbPort(_) | SerialPortType::Unknown
    ) {
        return Err(format!("authorized serial port is not a USB serial target: {port}").into());
    }
    Ok(())
}

fn read_identity_with_retry(
    port: &str,
) -> Result<ObservedIdentity, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = Instant::now() + IDENTITY_TIMEOUT;
    loop {
        if let Ok(identity) = read_identity(port) {
            return Ok(identity);
        }
        if Instant::now() >= deadline {
            return Err(format!("RAM bring-up identity was not received on {port}").into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn read_identity(port: &str) -> Result<ObservedIdentity, Box<dyn std::error::Error + Send + Sync>> {
    validate_exact_ram_port(port)?;
    let mut serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(200))
        .open_native()?;
    let deadline = Instant::now() + IDENTITY_TIMEOUT;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        match serial.read(&mut chunk) {
            Ok(count) => {
                bytes.extend_from_slice(&chunk[..count]);
                while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = bytes.drain(..=index).collect();
                    if let Some(identity) = parse_identity_line(&line) {
                        return Ok(identity);
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err("no identity frame received".into())
}

fn parse_identity_line(line: &[u8]) -> Option<ObservedIdentity> {
    let frame: IdentityFrame = serde_json::from_slice(line).ok()?;
    if frame.frame_type.as_deref() != Some("hello") {
        return None;
    }
    let body = frame.identity?;
    let kind = frame
        .firmware_kind
        .as_deref()
        .or(body.firmware_kind.as_deref());
    let firmware = match kind {
        Some("product") => ObservedFirmware::Product,
        Some("ram_bringup") => ObservedFirmware::RamBringup,
        _ => ObservedFirmware::Unknown,
    };
    Some(ObservedIdentity {
        firmware,
        build_id: body.build_id,
        capabilities: body.capabilities,
    })
}

fn verify_ram_identity(
    identity: &ObservedIdentity,
    op: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if identity.firmware != ObservedFirmware::RamBringup {
        return Err(
            "RAM bring-up identity is unknown or the target is still product firmware".into(),
        );
    }
    if identity.build_id.as_deref() != Some(PRODUCT_BUILD_ID) {
        return Err(format!(
            "RAM bring-up buildId mismatch: expected {PRODUCT_BUILD_ID}, got {:?}",
            identity.build_id
        )
        .into());
    }
    if !identity
        .capabilities
        .iter()
        .any(|capability| capability == op)
    {
        return Err(format!("RAM bring-up does not advertise capability {op}").into());
    }
    Ok(())
}

fn send_ram_request(
    port: &str,
    op: &str,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    validate_exact_ram_port(port)?;
    let request_id = format!("ram-{}", current_unix_millis());
    let mut serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(250))
        .open_native()?;
    let request = format!(
        "{{\"type\":\"ram_bringup\",\"requestId\":\"{request_id}\",\"op\":\"{op}\",\"capability\":\"{op}\"}}\n"
    );
    serial.write_all(request.as_bytes())?;
    serial.flush()?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        match serial.read(&mut chunk) {
            Ok(count) => {
                bytes.extend_from_slice(&chunk[..count]);
                while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
                    let line: Vec<u8> = bytes.drain(..=index).collect();
                    if let Ok(value) = serde_json::from_slice::<Value>(&line) {
                        if value.get("type").and_then(Value::as_str) != Some("response")
                            || value.get("firmwareKind").and_then(Value::as_str)
                                != Some("ram_bringup")
                            || value.get("capability").and_then(Value::as_str) != Some(op)
                            || value.get("requestId").and_then(Value::as_str) != Some(&request_id)
                        {
                            continue;
                        }
                        if value.get("ok").and_then(Value::as_bool) == Some(true) {
                            return Ok(value);
                        }
                        return Err(format!("RAM bring-up rejected {op}: {value}").into());
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(format!("RAM bring-up response timed out for {op}").into())
}

fn load_ram_elf(port: &str, elf: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    validate_exact_ram_port(port)?;
    let port_info = serialport::available_ports()?
        .into_iter()
        .find(|candidate| candidate.port_name == port)
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    let usb_info = match port_info.port_type {
        SerialPortType::UsbPort(info) => info,
        SerialPortType::Unknown => UsbPortInfo {
            vid: 0,
            pid: 0,
            serial_number: None,
            manufacturer: None,
            product: None,
        },
        _ => return Err("RAM loader requires a USB serial target".into()),
    };
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .open_native()?;
    let connection = ::espflash::connection::Connection::new(
        serial,
        usb_info,
        ::espflash::connection::ResetAfterOperation::HardReset,
        ::espflash::connection::ResetBeforeOperation::DefaultReset,
        115_200,
    );
    let mut flasher = ::espflash::flasher::Flasher::connect(
        connection,
        false,
        true,
        false,
        Some(::espflash::target::Chip::Esp32s3),
        None,
    )?;
    let elf_data = fs::read(elf)?;
    flasher.load_elf_to_ram(&elf_data, &mut ::espflash::target::DefaultProgressCallback)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_and_ram_frames_are_classified_separately() {
        let product = parse_identity_line(
            br#"{"type":"hello","firmwareKind":"product","identity":{"firmwareKind":"product","buildId":"p","capabilities":["identity"]}}"#,
        )
        .unwrap();
        assert_eq!(product.firmware, ObservedFirmware::Product);
        let ram = parse_identity_line(
            br#"{"type":"hello","firmwareKind":"ram_bringup","identity":{"firmwareKind":"ram_bringup","buildId":"r","capabilities":["test_fan"]}}"#,
        )
        .unwrap();
        assert_eq!(ram.firmware, ObservedFirmware::RamBringup);
    }

    fn test_elf(address: u32) -> Vec<u8> {
        let payload_offset = 52 + 32;
        let payload_size = 32usize;
        let mut data = vec![0u8; payload_offset + payload_size];
        data[0..4].copy_from_slice(b"\x7fELF");
        data[4] = 1;
        data[5] = 1;
        data[16..18].copy_from_slice(&2u16.to_le_bytes());
        data[18..20].copy_from_slice(&ELF_MACHINE_XTENSA.to_le_bytes());
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        data[24..28].copy_from_slice(&0x4037_8400u32.to_le_bytes());
        data[28..32].copy_from_slice(&52u32.to_le_bytes());
        data[40..42].copy_from_slice(&52u16.to_le_bytes());
        data[42..44].copy_from_slice(&32u16.to_le_bytes());
        data[44..46].copy_from_slice(&1u16.to_le_bytes());
        let ph = 52;
        data[ph..ph + 4].copy_from_slice(&ELF_PT_LOAD.to_le_bytes());
        data[ph + 4..ph + 8].copy_from_slice(&(payload_offset as u32).to_le_bytes());
        data[ph + 8..ph + 12].copy_from_slice(&address.to_le_bytes());
        data[ph + 12..ph + 16].copy_from_slice(&address.to_le_bytes());
        data[ph + 16..ph + 20].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[ph + 20..ph + 24].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[ph + 24..ph + 28].copy_from_slice(&5u32.to_le_bytes());
        data[ph + 28..ph + 32].copy_from_slice(&4u32.to_le_bytes());
        data
    }

    #[test]
    fn ram_elf_gate_accepts_internal_ram_and_rejects_flash() {
        let mut valid = tempfile::NamedTempFile::new().unwrap();
        valid.write_all(&test_elf(0x4037_8400)).unwrap();
        assert!(validate_ram_elf(valid.path()).is_ok());

        let mut flash = tempfile::NamedTempFile::new().unwrap();
        flash.write_all(&test_elf(0x4200_0000)).unwrap();
        assert!(validate_ram_elf(flash.path()).is_err());
    }
}
