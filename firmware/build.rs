fn main() {
    let source_sha = std::env::var("FLUX_PURR_SOURCE_SHA")
        .ok()
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .or_else(|| {
            std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .and_then(|output| String::from_utf8(output.stdout).ok())
                .map(|value| value.trim().to_string())
                .filter(|value| value.len() == 40)
        })
        .unwrap_or_else(|| "0000000000000000000000000000000000000000".to_string());
    let version = std::env::var("FLUX_PURR_FIRMWARE_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION"));
    let build_id = std::env::var("FLUX_PURR_BUILD_ID")
        .ok()
        .filter(|value| (16..=64).contains(&value.len()))
        .unwrap_or_else(|| source_sha[..16].to_string());

    println!("cargo:rustc-env=FLUX_PURR_FW_VERSION={version}");
    println!("cargo:rustc-env=FLUX_PURR_SOURCE_SHA={source_sha}");
    println!("cargo:rustc-env=FLUX_PURR_BUILD_ID={build_id}");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_FIRMWARE_VERSION");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_SOURCE_SHA");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_BUILD_ID");
    println!("cargo:rerun-if-changed=../.git/HEAD");

    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_arch == "xtensa" && target_os == "none" {
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }
}
