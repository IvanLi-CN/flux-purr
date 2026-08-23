use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use flux_purr_devd::firmware_bundle::{BundleChannel, BundleIdentity, build_bundle};

#[derive(Debug, Parser)]
#[command(about = "Build a deterministic Flux Purr firmware bundle")]
struct Args {
    #[arg(long)]
    bootloader: PathBuf,
    #[arg(long)]
    partition_table: PathBuf,
    #[arg(long)]
    factory_app: PathBuf,
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
    #[arg(long = "migration")]
    migrations: Vec<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Channel {
    Stable,
    Rc,
    Local,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let bundle = build_bundle(
        &args.output,
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
        &std::fs::read(args.bootloader)?,
        &std::fs::read(args.partition_table)?,
        &std::fs::read(args.factory_app)?,
        args.migrations,
    )?;
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
