use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Cursor, Read, Seek, Write},
    path::{Component, Path},
};

use md5::Md5;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::{CompressionMethod, ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const BUNDLE_MEDIA_TYPE: &str = "application/vnd.flux-purr.firmware-bundle+zip";
pub const MAX_BUNDLE_BYTES: u64 = 8 * 1024 * 1024;
pub const LAYOUT_ID: &str = "flux-purr.esp32s3fh4r2.factory";
pub const LAYOUT_VERSION: u32 = 1;
pub const CURRENT_PARTITION_TABLE_SHA256: &str =
    "sha256:1028e62176495136a0180a3f49c291dddec8d4fa7cab5cda72edabafa466013b";
pub const INTEGRITY_CATALOG_FILE: &str = "firmware-integrity-catalog.json";
const REQUIRED_PATHS: [&str; 4] = [
    "images/bootloader.bin",
    "images/factory-app.bin",
    "images/partition-table.bin",
    "manifest.json",
];

#[derive(Debug, Error)]
pub enum BundleError {
    #[error("bundle exceeds the 8 MiB limit")]
    TooLarge,
    #[error("bundle ZIP is invalid: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("bundle I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest JSON is invalid: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("bundle entry is unsafe or unexpected: {0}")]
    UnsafeEntry(String),
    #[error("bundle entry is missing: {0}")]
    MissingEntry(&'static str),
    #[error("bundle contract violation: {0}")]
    Contract(String),
    #[error("firmware integrity catalog violation: {0}")]
    IntegrityCatalog(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FirmwareBundleManifest {
    pub schema_version: u32,
    pub media_type: String,
    pub identity: BundleIdentity,
    pub target: BundleTarget,
    pub layout: BundleLayout,
    pub segments: Vec<BundleSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleIdentity {
    pub version: String,
    pub source_sha: String,
    pub build_id: String,
    pub channel: BundleChannel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleChannel {
    Stable,
    Rc,
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleTarget {
    pub chip: String,
    pub package: String,
    pub flash_size: u64,
    pub psram_size: u64,
    pub flash_mode: String,
    pub flash_frequency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleLayout {
    pub id: String,
    pub version: u32,
    pub partition_table_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BundleSegment {
    pub kind: SegmentKind,
    pub path: String,
    pub address: u64,
    pub length: u64,
    pub sha256: String,
    pub md5: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SegmentKind {
    Bootloader,
    PartitionTable,
    FactoryApp,
}

#[derive(Debug, Clone)]
pub struct FirmwareBundle {
    pub manifest: FirmwareBundleManifest,
    pub bundle_sha256: String,
    pub archive_size: u64,
    pub images: HashMap<String, Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FirmwareIntegrityCatalog {
    pub schema_version: u32,
    pub bundles: Vec<FirmwareIntegrityCatalogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FirmwareIntegrityCatalogEntry {
    pub version: String,
    pub source_sha: String,
    pub build_id: String,
    pub channel: BundleChannel,
    pub hardware_profile: String,
    pub bundle_sha256: String,
}

pub fn read_bundle(path: &Path) -> Result<FirmwareBundle, BundleError> {
    let metadata = path.metadata()?;
    if metadata.len() > MAX_BUNDLE_BYTES {
        return Err(BundleError::TooLarge);
    }
    let bytes = std::fs::read(path)?;
    read_bundle_bytes(&bytes)
}

pub fn read_integrity_catalog(path: &Path) -> Result<FirmwareIntegrityCatalog, BundleError> {
    let bytes = std::fs::read(path)?;
    let catalog: FirmwareIntegrityCatalog = serde_json::from_slice(&bytes)
        .map_err(|error| BundleError::IntegrityCatalog(error.to_string()))?;
    validate_integrity_catalog(&catalog)?;
    Ok(catalog)
}

pub fn verify_bundle_against_catalog(
    bundle: &FirmwareBundle,
    catalog: &FirmwareIntegrityCatalog,
) -> Result<(), BundleError> {
    validate_integrity_catalog(catalog)?;
    let expected = catalog
        .bundles
        .iter()
        .find(|entry| entry.bundle_sha256 == bundle.bundle_sha256)
        .ok_or_else(|| BundleError::IntegrityCatalog("bundle hash is not published".into()))?;
    if expected.version != bundle.manifest.identity.version
        || expected.source_sha != bundle.manifest.identity.source_sha
        || expected.build_id != bundle.manifest.identity.build_id
        || expected.channel != bundle.manifest.identity.channel
        || expected.hardware_profile != bundle.manifest.target.package
    {
        return Err(BundleError::IntegrityCatalog(
            "catalog metadata does not match bundle manifest".into(),
        ));
    }
    Ok(())
}

fn validate_integrity_catalog(catalog: &FirmwareIntegrityCatalog) -> Result<(), BundleError> {
    if catalog.schema_version != 1 || catalog.bundles.is_empty() {
        return Err(BundleError::IntegrityCatalog(
            "unsupported or empty catalog".into(),
        ));
    }
    let mut hashes = HashSet::new();
    for entry in &catalog.bundles {
        if semver::Version::parse(&entry.version).is_err()
            || entry.source_sha.len() != 40
            || !entry
                .source_sha
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || entry.build_id.len() < 16
            || entry.build_id.len() > 64
            || !entry
                .build_id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || entry.hardware_profile != "ESP32-S3FH4R2"
            || !entry.bundle_sha256.starts_with("sha256:")
            || entry.bundle_sha256.len() != 71
            || !entry.bundle_sha256[7..]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || !hashes.insert(entry.bundle_sha256.as_str())
            || matches!(entry.channel, BundleChannel::Local)
        {
            return Err(BundleError::IntegrityCatalog(
                "invalid or duplicate catalog entry".into(),
            ));
        }
    }
    Ok(())
}

pub fn read_bundle_bytes(bytes: &[u8]) -> Result<FirmwareBundle, BundleError> {
    if bytes.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(BundleError::TooLarge);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))?;
    let mut entries = HashMap::new();
    let mut names = HashSet::new();
    let mut uncompressed = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        validate_entry_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(BundleError::UnsafeEntry(format!("duplicate {name}")));
        }
        if !REQUIRED_PATHS.contains(&name.as_str()) {
            return Err(BundleError::UnsafeEntry(name));
        }
        if entry.is_dir()
            || entry
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(BundleError::UnsafeEntry(name));
        }
        uncompressed = uncompressed
            .checked_add(entry.size())
            .ok_or(BundleError::TooLarge)?;
        if uncompressed > MAX_BUNDLE_BYTES {
            return Err(BundleError::TooLarge);
        }
        let mut content = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut content)?;
        entries.insert(name, content);
    }
    for path in REQUIRED_PATHS {
        if !entries.contains_key(path) {
            return Err(BundleError::MissingEntry(path));
        }
    }
    if entries.len() != REQUIRED_PATHS.len() {
        return Err(BundleError::Contract(
            "archive file count is not four".into(),
        ));
    }

    let manifest: FirmwareBundleManifest = serde_json::from_slice(&entries["manifest.json"])?;
    validate_manifest(&manifest, &entries)?;
    Ok(FirmwareBundle {
        manifest,
        bundle_sha256: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
        archive_size: bytes.len() as u64,
        images: entries,
    })
}

fn validate_entry_name(name: &str) -> Result<(), BundleError> {
    let path = Path::new(name);
    if name.contains('\\')
        || name.contains(':')
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(BundleError::UnsafeEntry(name.to_string()));
    }
    Ok(())
}

fn validate_manifest(
    manifest: &FirmwareBundleManifest,
    entries: &HashMap<String, Vec<u8>>,
) -> Result<(), BundleError> {
    if manifest.schema_version != 2 || manifest.media_type != BUNDLE_MEDIA_TYPE {
        return Err(BundleError::Contract(
            "unsupported schema or media type".into(),
        ));
    }
    if semver::Version::parse(&manifest.identity.version).is_err()
        || manifest.identity.source_sha.len() != 40
        || !manifest
            .identity
            .source_sha
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || manifest.identity.build_id.len() < 16
        || manifest.identity.build_id.len() > 64
        || !manifest
            .identity
            .build_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(BundleError::Contract("invalid build identity".into()));
    }
    if manifest.target.chip != "esp32s3"
        || manifest.target.package != "ESP32-S3FH4R2"
        || manifest.target.flash_size != 4 * 1024 * 1024
        || manifest.target.psram_size != 2 * 1024 * 1024
        || manifest.target.flash_mode != "dio"
        || manifest.target.flash_frequency != "40m"
    {
        return Err(BundleError::Contract(
            "target does not match ESP32-S3FH4R2".into(),
        ));
    }
    if manifest.layout.id != LAYOUT_ID
        || manifest.layout.version != LAYOUT_VERSION
        || manifest.layout.partition_table_sha256 != CURRENT_PARTITION_TABLE_SHA256
    {
        return Err(BundleError::Contract(
            "layout identity does not match".into(),
        ));
    }
    let expected = [
        (
            SegmentKind::Bootloader,
            "images/bootloader.bin",
            0,
            1,
            0x8000,
        ),
        (
            SegmentKind::PartitionTable,
            "images/partition-table.bin",
            0x8000,
            0x1000,
            0x1000,
        ),
        (
            SegmentKind::FactoryApp,
            "images/factory-app.bin",
            0x10000,
            1,
            0x200000,
        ),
    ];
    if manifest.segments.len() != expected.len() {
        return Err(BundleError::Contract(
            "exactly three ordered segments are required".into(),
        ));
    }
    for (segment, (kind, path, address, min_len, max_len)) in manifest.segments.iter().zip(expected)
    {
        if segment.kind != kind
            || segment.path != path
            || segment.address != address
            || segment.length < min_len
            || segment.length > max_len
        {
            return Err(BundleError::Contract(format!(
                "invalid segment layout for {path}"
            )));
        }
        let bytes = entries
            .get(path)
            .ok_or(BundleError::Contract(format!("missing bytes for {path}")))?;
        if bytes.len() as u64 != segment.length
            || segment.sha256 != format!("sha256:{}", hex::encode(Sha256::digest(bytes)))
            || segment.md5 != hex::encode(Md5::digest(bytes))
        {
            return Err(BundleError::Contract(format!(
                "hash or length mismatch for {path}"
            )));
        }
    }
    let partition_table = manifest
        .segments
        .iter()
        .find(|segment| segment.kind == SegmentKind::PartitionTable)
        .ok_or_else(|| BundleError::Contract("partition table segment is missing".into()))?;
    if partition_table.sha256 != manifest.layout.partition_table_sha256 {
        return Err(BundleError::Contract(
            "partition table segment hash does not match layout".into(),
        ));
    }
    if entries["images/partition-table.bin"].len() != 0x1000 {
        return Err(BundleError::Contract(
            "partition table must be exactly 4 KiB".into(),
        ));
    }
    Ok(())
}

pub fn build_bundle(
    output: &Path,
    identity: BundleIdentity,
    bootloader: &[u8],
    partition_table: &[u8],
    factory_app: &[u8],
) -> Result<FirmwareBundle, BundleError> {
    if partition_table.len() > 0x1000 {
        return Err(BundleError::Contract(
            "partition table input exceeds the 4 KiB flash segment".into(),
        ));
    }
    let mut padded_partition_table = vec![0xff; 0x1000];
    padded_partition_table[..partition_table.len()].copy_from_slice(partition_table);
    let partition_table = padded_partition_table.as_slice();
    let segments = [
        segment(
            SegmentKind::Bootloader,
            "images/bootloader.bin",
            0,
            bootloader,
        ),
        segment(
            SegmentKind::PartitionTable,
            "images/partition-table.bin",
            0x8000,
            partition_table,
        ),
        segment(
            SegmentKind::FactoryApp,
            "images/factory-app.bin",
            0x10000,
            factory_app,
        ),
    ];
    let manifest = FirmwareBundleManifest {
        schema_version: 2,
        media_type: BUNDLE_MEDIA_TYPE.into(),
        identity,
        target: BundleTarget {
            chip: "esp32s3".into(),
            package: "ESP32-S3FH4R2".into(),
            flash_size: 4 * 1024 * 1024,
            psram_size: 2 * 1024 * 1024,
            flash_mode: "dio".into(),
            flash_frequency: "40m".into(),
        },
        layout: BundleLayout {
            id: LAYOUT_ID.into(),
            version: LAYOUT_VERSION,
            partition_table_sha256: CURRENT_PARTITION_TABLE_SHA256.into(),
        },
        segments: segments.to_vec(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)?;
    let mut manifest_bytes = manifest_bytes;
    manifest_bytes.push(b'\n');
    let file = File::create(output)?;
    write_zip(
        file,
        &manifest_bytes,
        bootloader,
        partition_table,
        factory_app,
    )?;
    read_bundle(output)
}

fn segment(kind: SegmentKind, path: &str, address: u64, bytes: &[u8]) -> BundleSegment {
    BundleSegment {
        kind,
        path: path.into(),
        address,
        length: bytes.len() as u64,
        sha256: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
        md5: hex::encode(Md5::digest(bytes)),
    }
}

fn write_zip<W: Write + Seek>(
    output: W,
    manifest: &[u8],
    bootloader: &[u8],
    partition_table: &[u8],
    factory_app: &[u8],
) -> Result<(), BundleError> {
    let mut zip = ZipWriter::new(output);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644)
        .last_modified_time(zip::DateTime::default());
    for (name, bytes) in [
        ("images/bootloader.bin", bootloader),
        ("images/factory-app.bin", factory_app),
        ("images/partition-table.bin", partition_table),
        ("manifest.json", manifest),
    ] {
        zip.start_file(name, options)?;
        zip.write_all(bytes)?;
    }
    zip.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn identity() -> BundleIdentity {
        BundleIdentity {
            version: "0.1.0".into(),
            source_sha: "e9754917ee23481dd30571fb7a78cb2c486b82a3".into(),
            build_id: "0123456789abcdef".into(),
            channel: BundleChannel::Local,
        }
    }

    #[test]
    fn deterministic_round_trip_enforces_hashes_and_layout() {
        let dir = tempdir().unwrap();
        let one = dir.path().join("one.fluxpurr-fw");
        let two = dir.path().join("two.fluxpurr-fw");
        let bootloader = vec![0x11; 0x4000];
        let partition = include_bytes!("../../../firmware/partitions.bin");
        let app = vec![0x33; 0x4000];
        build_bundle(&one, identity(), &bootloader, partition, &app).unwrap();
        build_bundle(&two, identity(), &bootloader, partition, &app).unwrap();
        assert_eq!(std::fs::read(&one).unwrap(), std::fs::read(&two).unwrap());
        let parsed = read_bundle(&one).unwrap();
        assert_eq!(parsed.manifest.segments.len(), 3);
        assert!(parsed.archive_size < MAX_BUNDLE_BYTES);
    }

    #[test]
    fn rejects_unknown_manifest_fields() {
        let fixture = include_str!(
            "../../../docs/specs/web-firmware-install-recovery/contracts/fixtures/invalid-unknown-field.json"
        );
        assert!(serde_json::from_str::<FirmwareBundleManifest>(fixture).is_err());
    }

    #[test]
    fn rejects_traversal_entry() {
        let mut bytes = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut bytes);
            zip.start_file("../manifest.json", SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"{}").unwrap();
            zip.finish().unwrap();
        }
        assert!(matches!(
            read_bundle_bytes(bytes.get_ref()),
            Err(BundleError::UnsafeEntry(_))
        ));
    }

    #[test]
    fn integrity_catalog_requires_matching_release_identity_and_hash() {
        let dir = tempdir().unwrap();
        let bundle_path = dir.path().join("release.fluxpurr-fw");
        let mut release_identity = identity();
        release_identity.channel = BundleChannel::Stable;
        release_identity.version = "1.2.3".into();
        let bundle = build_bundle(
            &bundle_path,
            release_identity.clone(),
            &[0x11; 0x100],
            include_bytes!("../../../firmware/partitions.bin"),
            &[0x33; 0x100],
        )
        .unwrap();
        let catalog = FirmwareIntegrityCatalog {
            schema_version: 1,
            bundles: vec![FirmwareIntegrityCatalogEntry {
                version: release_identity.version,
                source_sha: release_identity.source_sha,
                build_id: release_identity.build_id,
                channel: release_identity.channel,
                hardware_profile: "ESP32-S3FH4R2".into(),
                bundle_sha256: bundle.bundle_sha256.clone(),
            }],
        };
        verify_bundle_against_catalog(&bundle, &catalog).unwrap();

        let mut tampered = catalog.clone();
        tampered.bundles[0].bundle_sha256 =
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
        assert!(matches!(
            verify_bundle_against_catalog(&bundle, &tampered),
            Err(BundleError::IntegrityCatalog(_))
        ));
    }
}
