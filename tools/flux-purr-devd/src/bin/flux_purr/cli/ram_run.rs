use super::*;
use espflash::{
    command::{Command as RomCommand, CommandType},
    connection::{Connection, Port, ResetAfterOperation, ResetBeforeOperation},
    target::Chip,
};
use flux_purr_devd::PRODUCT_BUILD_ID;
use flux_purr_devd::serial::{
    ESP32S3_USB_SERIAL_JTAG_PID, ESP32S3_USB_SERIAL_JTAG_VID, SerialPortProcessLock,
    UsbSerialIdentity, serial_port_paths_match, serial_port_usb_identity_matches,
};
use serialport::{FlowControl, SerialPort, SerialPortType, UsbPortInfo};
use std::time::{Duration, Instant};

const IDENTITY_TIMEOUT: Duration = Duration::from_secs(5);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(8);
const RAM_OPERATION_LOCK_TIMEOUT: Duration = Duration::from_secs(30);
const RAM_ELF_RELATIVE_PATH: &str =
    "firmware/ram-bringup/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup";
const RAM_PROTOCOL_VERSION: &str = "flux-purr.usb.v1";
const RAM_FRAMING: &str = "jsonl";

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
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    framing: Option<String>,
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
    #[serde(default)]
    protocol_version: Option<String>,
    #[serde(default)]
    framing: Option<String>,
}

#[derive(Debug, Clone)]
struct ObservedIdentity {
    firmware: ObservedFirmware,
    build_id: Option<String>,
    capabilities: Vec<String>,
    protocol_version: Option<String>,
    framing: Option<String>,
}

pub(crate) fn execute_ram_run(
    command: RamRunCommand,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    match command {
        RamRunCommand::Preview(args) => {
            let color = match (args.scenario, args.color) {
                (RamPreviewScenario::Display, color) => color.map(RamPreviewColor::wire),
                (_, None) => None,
                (_, Some(_)) => {
                    return Err("--color is only valid with 'ram-run preview display'".into());
                }
            };
            run_ram_operation(
                &args.port,
                args.elf.as_deref(),
                args.reload,
                args.scenario.op(),
                color,
            )
        }
        RamRunCommand::Test(args) => run_ram_operation(
            &args.port,
            args.elf.as_deref(),
            args.reload,
            args.test.op(),
            None,
        ),
        RamRunCommand::Exit(args) => exit_ram(&args.port),
    }
}

fn run_ram_operation(
    port: &str,
    elf: Option<&Path>,
    reload: bool,
    op: &str,
    color: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let usb_identity = validate_exact_ram_port(port)?;
    let _serial_lock = acquire_ram_port_lock(port)?;
    ensure_ram_target(port, &usb_identity)?;
    let observed = if reload {
        None
    } else {
        read_identity(port, &usb_identity).ok()
    };
    let matching_ram = observed.as_ref().is_some_and(|(identity, _)| {
        identity.firmware == ObservedFirmware::RamBringup
            && identity.build_id.as_deref() == Some(PRODUCT_BUILD_ID)
            && identity
                .capabilities
                .iter()
                .any(|capability| capability == op)
    });
    let (identity, mut serial) = if matching_ram {
        observed.expect("matching RAM identity must exist")
    } else {
        drop(observed);
        let elf = elf.map(Path::to_path_buf).unwrap_or_else(default_ram_elf);
        validate_local_elf(&elf)?;
        let image = read_validated_ram_elf(&elf)?;
        load_ram_elf(port, image, &usb_identity)?
    };
    verify_ram_identity(&identity, op)?;
    ensure_ram_target(port, &usb_identity)?;
    send_ram_request(&mut serial, op, color)
}

fn exit_ram(port: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let usb_identity = validate_exact_ram_port(port)?;
    let _serial_lock = acquire_ram_port_lock(port)?;
    ensure_ram_target(port, &usb_identity)?;
    let (identity, mut serial) = match read_identity(port, &usb_identity) {
        Ok(result) => result,
        Err(_) => {
            let elf = default_ram_elf();
            validate_local_elf(&elf)?;
            let image = read_validated_ram_elf(&elf)?;
            load_ram_elf(port, image, &usb_identity)?
        }
    };
    verify_ram_identity(&identity, "exit")?;
    ensure_ram_target(port, &usb_identity)?;
    send_ram_request(&mut serial, "exit", None)
}

fn acquire_ram_port_lock(
    port: &str,
) -> Result<SerialPortProcessLock, Box<dyn std::error::Error + Send + Sync>> {
    SerialPortProcessLock::acquire(port, Instant::now() + RAM_OPERATION_LOCK_TIMEOUT)
        .map_err(|error| format!("failed to acquire RAM serial lock: {error:?}").into())
}

fn default_ram_elf() -> PathBuf {
    flux_purr_repo_root().join(RAM_ELF_RELATIVE_PATH)
}

const ELF_PT_LOAD: u32 = 1;
const ELF_PF_X: u64 = 0x1;
const ELF_SHF_ALLOC: u64 = 0x2;
const ELF_SHF_EXECINSTR: u64 = 0x4;
const ELF_SHT_PROGBITS: u32 = 1;
const ELF_SHT_INIT_ARRAY: u32 = 14;
const ELF_MACHINE_XTENSA: u16 = 94;
const RAM_VECTORS: (u64, u64) = (0x4037_8000, 0x4037_8400);
const RAM_IRAM: (u64, u64) = (0x4037_8400, 0x403b_8400);
const RAM_DRAM: (u64, u64) = (0x3fc8_8000, 0x3fce_8000);
const RAM_RESERVED: (u64, u64) = (0x3fce_8000, 0x3fce_d710);
const RAM_FLASH_WINDOWS: [(u64, u64); 2] = [(0x4200_0000, 0x4400_0000), (0x3c00_0000, 0x3d00_0000)];
const RAM_BLOCK_SIZE: usize = 0x1800;
const RAM_RESPONSE_BUFFER_LIMIT: usize = 16 * 1024;
const RAM_ELF_FILE_LIMIT: u64 = 8 * 1024 * 1024;
const RTC_CNTL_BASE: u32 = 0x6000_8000;
const RTC_CNTL_SWD_CONF: u32 = RTC_CNTL_BASE + 0x00b4;
const RTC_CNTL_SWD_WPROTECT: u32 = RTC_CNTL_BASE + 0x00b8;
const RTC_CNTL_WDTCONFIG0: u32 = RTC_CNTL_BASE + 0x0098;
const RTC_CNTL_WDTWPROTECT: u32 = RTC_CNTL_BASE + 0x00b0;
const RTC_CNTL_SWD_WKEY: u32 = 0x8f1d_312a;
const RTC_CNTL_WDT_WKEY: u32 = 0x50d8_3aa1;
const RTC_CNTL_SWD_AUTO_FEED_EN: u32 = 1 << 31;

struct RamElfHeader {
    class: u8,
    entry: u64,
    phoff: u64,
    phentsize: u64,
    phnum: u64,
    shoff: u64,
    shentsize: u64,
    shnum: u64,
}

type RamSegmentFields = (u64, u64, u64, u64, u64, u64);
type RamSectionFields = (u32, u64, u64, u64, u64);
type RamLoadSection = (u32, Vec<u8>);

#[derive(Debug, Clone, Copy)]
struct RamLoadSegment {
    p_offset: u64,
    paddr: u64,
    filesz: u64,
    memsz: u64,
}

struct RamElfImage {
    entry: u32,
    sections: Vec<RamLoadSection>,
}

#[derive(Default)]
struct RamLoadAccumulator {
    sections: Vec<RamLoadSection>,
    vectors_section_seen: bool,
    loaded_iram_bytes: u64,
    loaded_dram_bytes: u64,
}

#[cfg(test)]
fn validate_ram_elf(path: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let data = read_ram_elf_file(path)?;
    parse_validated_ram_elf(&data, path).map(|_| ())
}

fn read_validated_ram_elf(
    path: &Path,
) -> Result<RamElfImage, Box<dyn std::error::Error + Send + Sync>> {
    let data = read_ram_elf_file(path)?;
    parse_validated_ram_elf(&data, path)
}

fn read_ram_elf_file(path: &Path) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let size = fs::metadata(path)?.len();
    if size > RAM_ELF_FILE_LIMIT {
        return Err(format!(
            "RAM ELF exceeds the {RAM_ELF_FILE_LIMIT} byte file-size limit: {}",
            path.display()
        )
        .into());
    }
    Ok(fs::read(path)?)
}

fn parse_validated_ram_elf(
    data: &[u8],
    path: &Path,
) -> Result<RamElfImage, Box<dyn std::error::Error + Send + Sync>> {
    let header = parse_ram_elf_header(data, path)?;
    let expected_phentsize = if header.class == 1 { 32 } else { 56 };
    if header.phentsize < expected_phentsize {
        return Err("RAM ELF program header entry is too small".into());
    }
    let mut load_count = 0u32;
    let mut iram_bytes = 0u64;
    let mut dram_bytes = 0u64;
    let mut executable_entry = false;
    let mut segments = Vec::new();
    let mut vectors_segment_seen = false;
    for index in 0..header.phnum {
        let offset = program_header_offset(header.phoff, header.phentsize, index)?;
        let p_type = read_u32(data, offset)?;
        if p_type != ELF_PT_LOAD {
            continue;
        }
        load_count += 1;
        let (segment_iram, segment_dram, contains_executable_entry) =
            validate_ram_segment(data, header.class, index, offset, header.entry)?;
        executable_entry |= contains_executable_entry;
        iram_bytes = iram_bytes
            .checked_add(segment_iram)
            .ok_or("RAM ELF IRAM budget overflow")?;
        dram_bytes = dram_bytes
            .checked_add(segment_dram)
            .ok_or("RAM ELF DRAM budget overflow")?;
        let (p_offset, paddr, _vaddr, filesz, memsz, _flags) =
            ram_segment_fields(data, header.class, offset)?;
        if paddr == RAM_VECTORS.0 && memsz == RAM_VECTORS.1 - RAM_VECTORS.0 {
            if vectors_segment_seen {
                return Err("RAM ELF contains duplicate vectors segments".into());
            }
            vectors_segment_seen = true;
        }
        segments.push(RamLoadSegment {
            p_offset,
            paddr,
            filesz,
            memsz,
        });
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
    if !executable_entry {
        return Err("RAM ELF entry point is not inside an executable RAM segment".into());
    }
    let (entry, sections) = ram_load_sections(data, path, &segments)?;
    Ok(RamElfImage { entry, sections })
}

fn parse_ram_elf_header(
    data: &[u8],
    path: &Path,
) -> Result<RamElfHeader, Box<dyn std::error::Error + Send + Sync>> {
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
    let machine = read_u16(data, 18)?;
    if machine != ELF_MACHINE_XTENSA {
        return Err(format!("RAM ELF machine {machine} is not Xtensa").into());
    }
    let values = match class {
        1 => (
            read_u32(data, 24)? as u64,
            read_u32(data, 28)? as u64,
            read_u32(data, 32)? as u64,
            read_u16(data, 42)? as u64,
            read_u16(data, 44)? as u64,
            read_u16(data, 46)? as u64,
            read_u16(data, 48)? as u64,
        ),
        2 => (
            read_u64(data, 24)?,
            read_u64(data, 32)?,
            read_u64(data, 40)?,
            read_u16(data, 54)? as u64,
            read_u16(data, 56)? as u64,
            read_u16(data, 58)? as u64,
            read_u16(data, 60)? as u64,
        ),
        _ => return Err(format!("unsupported RAM ELF class {class}").into()),
    };
    Ok(RamElfHeader {
        class,
        entry: values.0,
        phoff: values.1,
        shoff: values.2,
        phentsize: values.3,
        phnum: values.4,
        shentsize: values.5,
        shnum: values.6,
    })
}

fn program_header_offset(
    phoff: u64,
    phentsize: u64,
    index: u64,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let offset = phoff
        .checked_add(
            index
                .checked_mul(phentsize)
                .ok_or("RAM ELF program header overflow")?,
        )
        .ok_or("RAM ELF program header overflow")?;
    usize::try_from(offset).map_err(|_| "RAM ELF program header is too large".into())
}

fn section_header_offset(
    shoff: u64,
    shentsize: u64,
    index: u64,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    let offset = shoff
        .checked_add(
            index
                .checked_mul(shentsize)
                .ok_or("RAM ELF section header overflow")?,
        )
        .ok_or("RAM ELF section header overflow")?;
    usize::try_from(offset).map_err(|_| "RAM ELF section header is too large".into())
}

fn validate_ram_segment(
    data: &[u8],
    class: u8,
    index: u64,
    offset: usize,
    entry: u64,
) -> Result<(u64, u64, bool), Box<dyn std::error::Error + Send + Sync>> {
    let (p_offset, vaddr, paddr, filesz, memsz, flags) = ram_segment_fields(data, class, offset)?;
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
    if paddr == RAM_VECTORS.0 && end == RAM_VECTORS.1 {
        return Ok((0, 0, flags & ELF_PF_X != 0 && (paddr..end).contains(&entry)));
    }
    if range_contained(paddr, end, RAM_IRAM) {
        return Ok((
            memsz,
            0,
            flags & ELF_PF_X != 0 && (paddr..end).contains(&entry),
        ));
    }
    if range_contained(paddr, end, RAM_DRAM) {
        return Ok((
            0,
            memsz,
            flags & ELF_PF_X != 0 && (paddr..end).contains(&entry),
        ));
    }
    Err(format!("RAM ELF segment {index} is outside internal RAM").into())
}

fn ram_segment_fields(
    data: &[u8],
    class: u8,
    offset: usize,
) -> Result<RamSegmentFields, Box<dyn std::error::Error + Send + Sync>> {
    Ok(if class == 1 {
        (
            read_u32(data, offset_field(offset, 4)?)? as u64,
            read_u32(data, offset_field(offset, 8)?)? as u64,
            read_u32(data, offset_field(offset, 12)?)? as u64,
            read_u32(data, offset_field(offset, 16)?)? as u64,
            read_u32(data, offset_field(offset, 20)?)? as u64,
            read_u32(data, offset_field(offset, 24)?)? as u64,
        )
    } else {
        (
            read_u64(data, offset_field(offset, 8)?)?,
            read_u64(data, offset_field(offset, 16)?)?,
            read_u64(data, offset_field(offset, 24)?)?,
            read_u64(data, offset_field(offset, 32)?)?,
            read_u64(data, offset_field(offset, 40)?)?,
            read_u32(data, offset_field(offset, 4)?)? as u64,
        )
    })
}

fn ram_section_fields(
    data: &[u8],
    class: u8,
    offset: usize,
) -> Result<RamSectionFields, Box<dyn std::error::Error + Send + Sync>> {
    Ok(if class == 1 {
        (
            read_u32(data, offset_field(offset, 4)?)?,
            read_u32(data, offset_field(offset, 8)?)? as u64,
            read_u32(data, offset_field(offset, 12)?)? as u64,
            read_u32(data, offset_field(offset, 16)?)? as u64,
            read_u32(data, offset_field(offset, 20)?)? as u64,
        )
    } else {
        (
            read_u32(data, offset_field(offset, 4)?)?,
            read_u64(data, offset_field(offset, 8)?)?,
            read_u64(data, offset_field(offset, 16)?)?,
            read_u64(data, offset_field(offset, 24)?)?,
            read_u64(data, offset_field(offset, 32)?)?,
        )
    })
}

fn validate_ram_section(
    address: u64,
    size: u64,
    index: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let end = address
        .checked_add(size)
        .ok_or_else(|| format!("RAM ELF section {index} address overflow"))?;
    if RAM_FLASH_WINDOWS
        .iter()
        .any(|window| ranges_overlap(address, end, *window))
    {
        return Err(format!("RAM ELF section {index} maps to flash").into());
    }
    if ranges_overlap(address, end, RAM_RESERVED) {
        return Err(format!("RAM ELF section {index} overlaps reserved memory").into());
    }
    if address == RAM_VECTORS.0 && end == RAM_VECTORS.1 {
        return Ok(());
    }
    if range_contained(address, end, RAM_IRAM) || range_contained(address, end, RAM_DRAM) {
        return Ok(());
    }
    Err(format!("RAM ELF section {index} is outside internal RAM").into())
}

fn ram_load_sections(
    data: &[u8],
    path: &Path,
    segments: &[RamLoadSegment],
) -> Result<(u32, Vec<RamLoadSection>), Box<dyn std::error::Error + Send + Sync>> {
    let header = parse_ram_elf_header(data, path)?;
    let expected_shentsize = if header.class == 1 { 40 } else { 64 };
    if header.shentsize < expected_shentsize {
        return Err("RAM ELF section header entry is too small".into());
    }
    let entry = u32::try_from(header.entry).map_err(|_| "RAM ELF entry does not fit")?;
    let mut entry_in_executable_section = false;
    let mut load = RamLoadAccumulator::default();
    for index in 0..header.shnum {
        let offset = section_header_offset(header.shoff, header.shentsize, index)?;
        let fields = ram_section_fields(data, header.class, offset)?;
        if append_ram_load_section(data, header.entry, index, fields, segments, &mut load)? {
            entry_in_executable_section = true;
        }
    }
    append_ram_zero_fill_sections(segments, &mut load)?;
    if segments
        .iter()
        .any(|segment| segment.paddr == RAM_VECTORS.0)
        && !load.vectors_section_seen
    {
        return Err("RAM ELF vectors segment has no complete vectors section".into());
    }
    if load.sections.is_empty() {
        return Err("RAM ELF contains no loadable sections".into());
    }
    if !entry_in_executable_section {
        return Err(
            "RAM ELF entry point is not inside a file-backed executable RAM section".into(),
        );
    }
    Ok((entry, load.sections))
}

fn append_ram_load_section(
    data: &[u8],
    entry: u64,
    index: u64,
    (section_type, flags, address, data_offset, size): RamSectionFields,
    segments: &[RamLoadSegment],
    load: &mut RamLoadAccumulator,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    if !matches!(section_type, ELF_SHT_PROGBITS | ELF_SHT_INIT_ARRAY)
        || flags & ELF_SHF_ALLOC == 0
        || address == 0
        || data_offset == 0
        || size == 0
    {
        return Ok(false);
    }
    validate_ram_section(address, size, index)?;
    let section_end = address
        .checked_add(size)
        .ok_or_else(|| format!("RAM ELF section {index} address overflow"))?;
    let data_end = data_offset
        .checked_add(size)
        .ok_or_else(|| format!("RAM ELF section {index} file range overflow"))?;
    let segment = segments
        .iter()
        .find(|segment| {
            let Some(segment_end) = segment.paddr.checked_add(segment.memsz) else {
                return false;
            };
            let Some(segment_file_end) = segment.p_offset.checked_add(segment.filesz) else {
                return false;
            };
            range_contained(address, section_end, (segment.paddr, segment_end))
                && range_contained(data_offset, data_end, (segment.p_offset, segment_file_end))
        })
        .ok_or_else(|| format!("RAM ELF section {index} is not backed by a PT_LOAD segment"))?;
    if address == RAM_VECTORS.0 && section_end == RAM_VECTORS.1 {
        if load.vectors_section_seen
            || segment.paddr != RAM_VECTORS.0
            || segment.memsz != RAM_VECTORS.1 - RAM_VECTORS.0
            || segment.filesz != RAM_VECTORS.1 - RAM_VECTORS.0
            || data_offset != segment.p_offset
            || size != segment.filesz
        {
            return Err(format!(
                "RAM ELF section {index} is not the unique complete vectors section"
            )
            .into());
        }
        load.vectors_section_seen = true;
    } else {
        record_ram_load_budget(
            address,
            size,
            &mut load.loaded_iram_bytes,
            &mut load.loaded_dram_bytes,
        )?;
    }
    let start = usize::try_from(data_offset)
        .map_err(|_| format!("RAM ELF section {index} offset is too large"))?;
    let end = usize::try_from(data_end)
        .map_err(|_| format!("RAM ELF section {index} end is too large"))?;
    let segment_data = data
        .get(start..end)
        .ok_or_else(|| format!("RAM ELF section {index} exceeds the artifact"))?;
    let address_u32 = u32::try_from(address)
        .map_err(|_| format!("RAM ELF section {index} address is too large"))?;
    load.sections.push((address_u32, segment_data.to_vec()));
    Ok(flags & ELF_SHF_EXECINSTR != 0
        && (address..section_end).contains(&entry)
        && (address == RAM_VECTORS.0 && section_end == RAM_VECTORS.1
            || range_contained(address, section_end, RAM_IRAM)))
}

fn append_ram_zero_fill_sections(
    segments: &[RamLoadSegment],
    load: &mut RamLoadAccumulator,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    for segment in segments {
        let zero_fill = segment
            .memsz
            .checked_sub(segment.filesz)
            .ok_or("RAM ELF segment has p_filesz > p_memsz")?;
        if zero_fill == 0 {
            continue;
        }
        let address = segment
            .paddr
            .checked_add(segment.filesz)
            .ok_or("RAM ELF zero-fill address overflow")?;
        record_ram_load_budget(
            address,
            zero_fill,
            &mut load.loaded_iram_bytes,
            &mut load.loaded_dram_bytes,
        )?;
        let zero_fill_len =
            usize::try_from(zero_fill).map_err(|_| "RAM ELF zero-fill section is too large")?;
        load.sections.push((
            u32::try_from(address).map_err(|_| "RAM ELF zero-fill address is too large")?,
            vec![0; zero_fill_len],
        ));
    }
    Ok(())
}

fn record_ram_load_budget(
    address: u64,
    size: u64,
    loaded_iram_bytes: &mut u64,
    loaded_dram_bytes: &mut u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let end = address
        .checked_add(size)
        .ok_or("RAM ELF load range overflow")?;
    if range_contained(address, end, RAM_IRAM) {
        *loaded_iram_bytes = loaded_iram_bytes
            .checked_add(size)
            .ok_or("RAM ELF IRAM load budget overflow")?;
        if *loaded_iram_bytes > RAM_IRAM.1 - RAM_IRAM.0 {
            return Err("RAM ELF IRAM load budget exceeded".into());
        }
    } else if range_contained(address, end, RAM_DRAM) {
        *loaded_dram_bytes = loaded_dram_bytes
            .checked_add(size)
            .ok_or("RAM ELF DRAM load budget overflow")?;
        if *loaded_dram_bytes > RAM_DRAM.1 - RAM_DRAM.0 {
            return Err("RAM ELF DRAM load budget exceeded".into());
        }
    } else {
        return Err("RAM ELF load range is outside internal RAM".into());
    }
    Ok(())
}

fn offset_field(
    offset: usize,
    field: usize,
) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
    offset
        .checked_add(field)
        .ok_or_else(|| "RAM ELF program header field overflow".into())
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

fn validate_exact_ram_port(
    port: &str,
) -> Result<UsbSerialIdentity, Box<dyn std::error::Error + Send + Sync>> {
    validate_serial_port(port)?;
    #[cfg(not(target_os = "windows"))]
    let path = Path::new(port);
    #[cfg(not(target_os = "windows"))]
    if !path.exists() {
        return Err(format!("authorized serial port is unavailable: {port}").into());
    }
    let enumerated = serialport::available_ports()?
        .into_iter()
        .find(|candidate| serial_port_paths_match(port, &candidate.port_name))
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    let identity = UsbSerialIdentity::from_port_info(&enumerated)
        .ok_or_else(|| format!("authorized serial port has no stable USB identity: {port}"))?;
    if identity.vid != ESP32S3_USB_SERIAL_JTAG_VID || identity.pid != ESP32S3_USB_SERIAL_JTAG_PID {
        return Err(format!(
            "authorized serial port is not an ESP32-S3 USB Serial/JTAG target: {port}"
        )
        .into());
    }
    Ok(identity)
}

fn ensure_ram_target(
    port: &str,
    expected: &UsbSerialIdentity,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if serial_port_usb_identity_matches(port, expected) {
        return Ok(());
    }
    Err(format!("authorized USB target changed or is no longer enumerated: {port}").into())
}

fn read_identity(
    port: &str,
    expected: &UsbSerialIdentity,
) -> Result<(ObservedIdentity, Port), Box<dyn std::error::Error + Send + Sync>> {
    ensure_ram_target(port, expected)?;
    let mut serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .timeout(Duration::from_millis(200))
        .open_native()?;
    let identity = read_identity_from_serial(&mut serial)?;
    Ok((identity, serial))
}

fn read_identity_from_serial(
    serial: &mut dyn SerialPort,
) -> Result<ObservedIdentity, Box<dyn std::error::Error + Send + Sync>> {
    let deadline = Instant::now() + IDENTITY_TIMEOUT;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        match serial.read(&mut chunk) {
            Ok(count) => {
                if bytes.len().saturating_add(count) > RAM_RESPONSE_BUFFER_LIMIT {
                    return Err("RAM bring-up response exceeded the JSONL frame limit".into());
                }
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(identity) = next_identity(&mut bytes) {
                    return Ok(identity);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err("no identity frame received".into())
}

fn next_identity(bytes: &mut Vec<u8>) -> Option<ObservedIdentity> {
    while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = bytes.drain(..=index).collect();
        if let Some(identity) = parse_identity_line(&line) {
            return Some(identity);
        }
    }
    None
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
        protocol_version: frame.protocol_version.or(body.protocol_version),
        framing: frame.framing.or(body.framing),
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
    if identity.protocol_version.as_deref() != Some(RAM_PROTOCOL_VERSION)
        || identity.framing.as_deref() != Some(RAM_FRAMING)
    {
        return Err(format!(
            "RAM bring-up protocol mismatch: expected {RAM_PROTOCOL_VERSION}/{RAM_FRAMING}"
        )
        .into());
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
    serial: &mut dyn SerialPort,
    op: &str,
    color: Option<&str>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let request_id = format!("ram-{}", current_unix_millis());
    serial.set_timeout(Duration::from_millis(250))?;
    let request = build_ram_request(&request_id, op, color);
    serial.write_all(request.as_bytes())?;
    serial.flush()?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 256];
    while Instant::now() < deadline {
        match serial.read(&mut chunk) {
            Ok(count) => {
                append_ram_response_bytes(&mut bytes, &chunk[..count])?;
                if let Some(value) = find_ram_response(&mut bytes, op, &request_id)? {
                    return Ok(value);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(format!("RAM bring-up response timed out for {op}").into())
}

fn append_ram_response_bytes(
    bytes: &mut Vec<u8>,
    chunk: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bytes.len().saturating_add(chunk.len()) > RAM_RESPONSE_BUFFER_LIMIT {
        return Err("RAM bring-up response exceeded the JSONL frame limit".into());
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

fn build_ram_request(request_id: &str, op: &str, color: Option<&str>) -> String {
    match color {
        Some(color) => format!(
            "{{\"type\":\"ram_bringup\",\"requestId\":\"{request_id}\",\"op\":\"{op}\",\"capability\":\"{op}\",\"color\":\"{color}\"}}\n"
        ),
        None => format!(
            "{{\"type\":\"ram_bringup\",\"requestId\":\"{request_id}\",\"op\":\"{op}\",\"capability\":\"{op}\"}}\n"
        ),
    }
}

fn find_ram_response(
    bytes: &mut Vec<u8>,
    op: &str,
    request_id: &str,
) -> Result<Option<Value>, Box<dyn std::error::Error + Send + Sync>> {
    while let Some(index) = bytes.iter().position(|byte| *byte == b'\n') {
        let line: Vec<u8> = bytes.drain(..=index).collect();
        let Ok(value) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("response")
            || value.get("firmwareKind").and_then(Value::as_str) != Some("ram_bringup")
            || value.get("capability").and_then(Value::as_str) != Some(op)
            || value.get("requestId").and_then(Value::as_str) != Some(request_id)
        {
            continue;
        }
        if value.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(Some(value));
        }
        return Err(format!("RAM bring-up rejected {op}: {value}").into());
    }
    Ok(None)
}

fn disable_usb_serial_jtag_watchdogs(
    connection: &mut Connection,
    usb_pid: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if usb_pid != ESP32S3_USB_SERIAL_JTAG_PID {
        return Ok(());
    }
    connection.write_reg(RTC_CNTL_WDTWPROTECT, RTC_CNTL_WDT_WKEY, None)?;
    connection.write_reg(RTC_CNTL_WDTCONFIG0, 0, None)?;
    connection.write_reg(RTC_CNTL_WDTWPROTECT, 0, None)?;
    connection.write_reg(RTC_CNTL_SWD_WPROTECT, RTC_CNTL_SWD_WKEY, None)?;
    let swd_config = connection.read_reg(RTC_CNTL_SWD_CONF)?;
    connection.write_reg(
        RTC_CNTL_SWD_CONF,
        swd_config | RTC_CNTL_SWD_AUTO_FEED_EN,
        None,
    )?;
    connection.write_reg(RTC_CNTL_SWD_WPROTECT, 0, None)?;
    Ok(())
}

fn load_ram_elf(
    port: &str,
    image: RamElfImage,
    expected: &UsbSerialIdentity,
) -> Result<(ObservedIdentity, Port), Box<dyn std::error::Error + Send + Sync>> {
    ensure_ram_target(port, expected)?;
    let port_info = serialport::available_ports()?
        .into_iter()
        .find(|candidate| serial_port_paths_match(port, &candidate.port_name))
        .ok_or_else(|| format!("authorized serial port is no longer enumerated: {port}"))?;
    if !expected.matches_port_info(&port_info) {
        return Err(
            format!("authorized USB target changed or is no longer enumerated: {port}").into(),
        );
    }
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
    let usb_pid = usb_info.pid;
    let serial = serialport::new(port, 115_200)
        .flow_control(FlowControl::None)
        .open_native()?;
    let mut connection = Connection::new(
        serial,
        usb_info,
        ResetAfterOperation::HardReset,
        ResetBeforeOperation::DefaultReset,
        115_200,
    );
    connection.begin()?;
    connection.set_timeout(Duration::from_secs(3))?;
    let chip = connection.detect_chip(false)?;
    if chip != Chip::Esp32s3 {
        return Err(format!("RAM loader detected unexpected chip: {chip}").into());
    }
    ensure_ram_target(port, expected)?;
    disable_usb_serial_jtag_watchdogs(&mut connection, usb_pid)?;
    let RamElfImage { entry, sections } = image;
    for (address, mut data) in sections {
        let padding = (4 - data.len() % 4) % 4;
        data.resize(data.len() + padding, 0);
        let blocks = data.len().div_ceil(RAM_BLOCK_SIZE);
        connection.command(RomCommand::MemBegin {
            size: data.len() as u32,
            blocks: blocks as u32,
            block_size: RAM_BLOCK_SIZE as u32,
            offset: address,
            supports_encryption: false,
        })?;
        for (sequence, chunk) in data.chunks(RAM_BLOCK_SIZE).enumerate() {
            connection.command(RomCommand::MemData {
                data: chunk,
                pad_to: 4,
                pad_byte: 0,
                sequence: sequence as u32,
            })?;
        }
    }
    connection.with_timeout(CommandType::MemEnd.timeout(), |connection| {
        connection.command(RomCommand::MemEnd {
            no_entry: false,
            entry,
        })
    })?;
    let mut serial = connection.into_serial();
    let identity = read_identity_from_serial(&mut serial)?;
    Ok((identity, serial))
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

    #[test]
    fn ram_identity_requires_the_declared_protocol_and_framing() {
        let identity = ObservedIdentity {
            firmware: ObservedFirmware::RamBringup,
            build_id: Some(PRODUCT_BUILD_ID.to_string()),
            capabilities: vec!["test_fan".to_string()],
            protocol_version: Some(RAM_PROTOCOL_VERSION.to_string()),
            framing: Some(RAM_FRAMING.to_string()),
        };
        assert!(verify_ram_identity(&identity, "test_fan").is_ok());

        let mut mismatched = identity;
        mismatched.framing = Some("raw".to_string());
        assert!(verify_ram_identity(&mismatched, "test_fan").is_err());
    }

    #[test]
    fn ram_display_request_can_select_a_named_color_without_changing_capability() {
        let request = build_ram_request("ram-1", "preview_display", Some("red"));
        let value: Value = serde_json::from_str(&request).expect("request should be JSON");
        assert_eq!(value["op"], "preview_display");
        assert_eq!(value["capability"], "preview_display");
        assert_eq!(value["color"], "red");
    }

    #[test]
    fn ram_display_request_omits_color_by_default() {
        let request = build_ram_request("ram-1", "preview_display", None);
        let value: Value = serde_json::from_str(&request).expect("request should be JSON");
        assert!(value.get("color").is_none());
    }

    #[test]
    fn ram_response_buffer_rejects_an_unbounded_jsonl_frame() {
        let mut bytes = vec![b'x'; RAM_RESPONSE_BUFFER_LIMIT];
        assert!(append_ram_response_bytes(&mut bytes, b"y").is_err());
    }

    #[test]
    fn ram_elf_gate_rejects_an_oversized_file_before_reading_it() {
        let mut invalid = tempfile::NamedTempFile::new().unwrap();
        invalid
            .as_file_mut()
            .set_len(RAM_ELF_FILE_LIMIT + 1)
            .unwrap();

        let error = read_validated_ram_elf(invalid.path()).err().unwrap();

        assert!(error.to_string().contains("file-size limit"));
    }

    #[cfg(unix)]
    #[test]
    fn ram_port_lock_is_exclusive() {
        let port = format!("/tmp/flux-purr-devd-ram-test-port-{}", std::process::id());
        let first = acquire_ram_port_lock(&port).expect("first RAM lock should succeed");
        let second =
            match SerialPortProcessLock::acquire(&port, Instant::now() + Duration::from_millis(25))
            {
                Ok(_) => panic!("second RAM lock should be rejected"),
                Err(error) => error,
            };
        assert!(format!("{second:?}").contains("serial_lock_timeout"));
        drop(first);
    }

    fn test_elf(address: u32) -> Vec<u8> {
        test_elf_with_payload(address, 32)
    }

    fn test_elf_with_payload(address: u32, payload_size: usize) -> Vec<u8> {
        test_elf_with_payload_flags(address, payload_size, 5)
    }

    fn test_elf_with_mem_size(address: u32, payload_size: usize, mem_size: usize) -> Vec<u8> {
        let mut data = test_elf_with_payload_flags(address, payload_size, 5);
        data[52 + 20..52 + 24].copy_from_slice(&(mem_size as u32).to_le_bytes());
        data
    }

    fn test_elf_with_payload_flags(address: u32, payload_size: usize, flags: u32) -> Vec<u8> {
        let payload_offset = 52 + 32;
        let section_offset = payload_offset + payload_size;
        let mut data = vec![0u8; section_offset + 2 * 40];
        data[0..4].copy_from_slice(b"\x7fELF");
        data[4] = 1;
        data[5] = 1;
        data[16..18].copy_from_slice(&2u16.to_le_bytes());
        data[18..20].copy_from_slice(&ELF_MACHINE_XTENSA.to_le_bytes());
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        data[24..28].copy_from_slice(&address.to_le_bytes());
        data[28..32].copy_from_slice(&52u32.to_le_bytes());
        data[32..36].copy_from_slice(&(section_offset as u32).to_le_bytes());
        data[40..42].copy_from_slice(&52u16.to_le_bytes());
        data[42..44].copy_from_slice(&32u16.to_le_bytes());
        data[44..46].copy_from_slice(&1u16.to_le_bytes());
        data[46..48].copy_from_slice(&40u16.to_le_bytes());
        data[48..50].copy_from_slice(&2u16.to_le_bytes());
        let ph = 52;
        data[ph..ph + 4].copy_from_slice(&ELF_PT_LOAD.to_le_bytes());
        data[ph + 4..ph + 8].copy_from_slice(&(payload_offset as u32).to_le_bytes());
        data[ph + 8..ph + 12].copy_from_slice(&address.to_le_bytes());
        data[ph + 12..ph + 16].copy_from_slice(&address.to_le_bytes());
        data[ph + 16..ph + 20].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[ph + 20..ph + 24].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[ph + 24..ph + 28].copy_from_slice(&flags.to_le_bytes());
        data[ph + 28..ph + 32].copy_from_slice(&4u32.to_le_bytes());
        let section = section_offset + 40;
        data[section + 4..section + 8].copy_from_slice(&ELF_SHT_PROGBITS.to_le_bytes());
        let section_flags = ELF_SHF_ALLOC
            | if u64::from(flags) & ELF_PF_X != 0 {
                ELF_SHF_EXECINSTR
            } else {
                0
            };
        data[section + 8..section + 12].copy_from_slice(&(section_flags as u32).to_le_bytes());
        data[section + 12..section + 16].copy_from_slice(&address.to_le_bytes());
        data[section + 16..section + 20].copy_from_slice(&(payload_offset as u32).to_le_bytes());
        data[section + 20..section + 24].copy_from_slice(&(payload_size as u32).to_le_bytes());
        data[section + 32..section + 36].copy_from_slice(&4u32.to_le_bytes());
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

    #[test]
    fn ram_elf_gate_accepts_only_the_complete_vectors_segment() {
        let mut valid = tempfile::NamedTempFile::new().unwrap();
        valid
            .write_all(&test_elf_with_payload(0x4037_8000, 0x400))
            .unwrap();
        assert!(validate_ram_elf(valid.path()).is_ok());

        for payload_size in [32, 0x401] {
            let mut invalid = tempfile::NamedTempFile::new().unwrap();
            invalid
                .write_all(&test_elf_with_payload(0x4037_8000, payload_size))
                .unwrap();
            assert!(validate_ram_elf(invalid.path()).is_err());
        }
    }

    #[test]
    fn ram_elf_gate_rejects_entry_outside_executable_segment() {
        let mut invalid = tempfile::NamedTempFile::new().unwrap();
        invalid
            .write_all(&test_elf_with_payload_flags(0x3fc8_8000, 32, 6))
            .unwrap();
        assert!(validate_ram_elf(invalid.path()).is_err());
    }

    #[test]
    fn ram_elf_gate_rejects_entry_in_zero_fill_tail() {
        let mut artifact = test_elf_with_payload(0x4037_8400, 32);
        artifact[24..28].copy_from_slice(&0x4037_8420u32.to_le_bytes());
        artifact[52 + 20..52 + 24].copy_from_slice(&64u32.to_le_bytes());
        let mut invalid = tempfile::NamedTempFile::new().unwrap();
        invalid.write_all(&artifact).unwrap();
        assert!(validate_ram_elf(invalid.path()).is_err());
    }

    #[test]
    fn ram_elf_loader_zero_fills_segment_memory_tail() {
        let mut artifact = tempfile::NamedTempFile::new().unwrap();
        artifact
            .write_all(&test_elf_with_mem_size(0x4037_8400, 32, 48))
            .unwrap();

        let image = read_validated_ram_elf(artifact.path()).unwrap();

        assert_eq!(image.sections.len(), 2);
        assert_eq!(image.sections[1].0, 0x4037_8420);
        assert_eq!(image.sections[1].1, vec![0; 16]);
    }

    #[test]
    fn ram_elf_loader_rejects_sections_outside_load_segments() {
        let mut data = test_elf(0x4037_8400);
        let section_offset = 52 + 32 + 32 + 40;
        data[section_offset + 12..section_offset + 16]
            .copy_from_slice(&0x4037_8420u32.to_le_bytes());
        let mut artifact = tempfile::NamedTempFile::new().unwrap();
        artifact.write_all(&data).unwrap();

        assert!(read_validated_ram_elf(artifact.path()).is_err());
    }
}
