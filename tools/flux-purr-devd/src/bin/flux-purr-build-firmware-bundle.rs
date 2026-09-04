use std::{
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, ValueEnum};
use espflash::{
    flasher::{FlashData, FlashFrequency, FlashMode, FlashSettings, FlashSize},
    image_format::idf::IdfBootloaderFormat,
    target::{Chip, XtalFrequency},
};
use flux_purr_devd::firmware_bundle::{BundleChannel, BundleIdentity, SegmentKind, build_bundle};

#[derive(Debug, Parser)]
#[command(about = "Build the default local Flux Purr Web firmware bundle")]
struct Args {
    /// Firmware ELF produced by the ESP32-S3 release build.
    #[arg(long)]
    elf: PathBuf,
    /// ESP-IDF partition table CSV used to build the image.
    #[arg(long)]
    partition_table: PathBuf,
    /// Destination bundle; the parent directory is created if needed.
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    version: String,
    #[arg(long)]
    source_sha: String,
    #[arg(long)]
    build_id: String,
    #[arg(long, value_enum, default_value = "local")]
    channel: Channel,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Channel {
    Stable,
    Rc,
    Local,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let elf = fs::read(&args.elf)?;
    let flash_data = FlashData::new(
        FlashSettings::new(
            Some(FlashMode::Dio),
            Some(FlashSize::_4Mb),
            Some(FlashFrequency::_40Mhz),
        ),
        0,
        None,
        Chip::Esp32s3,
        XtalFrequency::_40Mhz,
    );
    let image = IdfBootloaderFormat::new(
        &elf,
        &flash_data,
        Some(&args.partition_table),
        None,
        None,
        Some("factory"),
    )?;
    let segments = image.flash_segments().collect::<Vec<_>>();
    let bootloader = segment_bytes(&segments, 0, SegmentKind::Bootloader)?;
    let partition_table = segment_bytes(&segments, 0x8000, SegmentKind::PartitionTable)?;
    let factory_app = segment_bytes(&segments, 0x10000, SegmentKind::FactoryApp)?;

    if args
        .output
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("fluxpurr-fw")
    {
        return Err("output must have a .fluxpurr-fw extension".into());
    }
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = temporary_output_path(&args.output);
    let result = build_bundle(
        &temporary,
        BundleIdentity {
            version: args.version,
            source_sha: args.source_sha,
            build_id: args.build_id,
            channel: match args.channel {
                Channel::Stable => BundleChannel::Stable,
                Channel::Rc => BundleChannel::Rc,
                Channel::Local => BundleChannel::Local,
            },
        },
        &bootloader,
        &partition_table,
        &factory_app,
    );
    match result {
        Ok(bundle) => {
            if args.output.exists() {
                fs::remove_file(&args.output)?;
            }
            fs::rename(&temporary, &args.output)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "artifactId": bundle.bundle_sha256,
                    "size": bundle.archive_size,
                    "output": args.output,
                }))?
            );
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            Err(error.into())
        }
    }
}

fn segment_bytes(
    segments: &[espflash::image_format::Segment<'_>],
    address: u32,
    kind: SegmentKind,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    segments
        .iter()
        .find(|segment| segment.addr == address)
        .map(|segment| segment.data.to_vec())
        .ok_or_else(|| {
            format!("ESP-IDF image is missing the {kind:?} segment at {address:#x}").into()
        })
}

fn temporary_output_path(output: &Path) -> PathBuf {
    let file_name = output
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("firmware.fluxpurr-fw");
    output.with_file_name(format!(".{file_name}.partial"))
}
