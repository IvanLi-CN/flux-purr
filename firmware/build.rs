fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .expect("firmware must live below the repository root");
    let resolver = repo_root.join("scripts/product-version.py");
    let build_mode = std::env::var("FLUX_PURR_BUILD_MODE").unwrap_or_else(|_| "development".into());
    configure_rtd_assumption(&build_mode);
    let source_sha = match std::env::var("FLUX_PURR_SOURCE_SHA") {
        Ok(value) => {
            if !valid_source_sha(&value) {
                panic!(
                    "FLUX_PURR_SOURCE_SHA must be a 40-character lowercase hexadecimal commit SHA"
                );
            }
            value
        }
        Err(_) => std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| valid_source_sha(value))
            .expect("FLUX_PURR_SOURCE_SHA or a Git checkout is required"),
    };
    let version = std::process::Command::new("python3")
        .arg(&resolver)
        .args(["--mode", &build_mode, "--source-sha", &source_sha])
        .output()
        .expect("failed to run product-version.py");
    if !version.status.success() {
        panic!(
            "product-version.py failed: {}",
            String::from_utf8_lossy(&version.stderr)
        );
    }
    let version = String::from_utf8(version.stdout)
        .expect("product-version.py output is not UTF-8")
        .trim()
        .to_string();
    let expected_build_id = &source_sha[..16];
    let build_id = match std::env::var("FLUX_PURR_BUILD_ID") {
        Ok(value) if value == expected_build_id => value,
        Ok(value) => panic!(
            "FLUX_PURR_BUILD_ID {value:?} does not match source SHA prefix {expected_build_id:?}"
        ),
        Err(_) => expected_build_id.to_string(),
    };

    println!("cargo:rustc-env=FLUX_PURR_FW_VERSION={version}");
    println!("cargo:rustc-env=FLUX_PURR_SOURCE_SHA={source_sha}");
    println!("cargo:rustc-env=FLUX_PURR_BUILD_ID={build_id}");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_BUILD_MODE");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_SOURCE_SHA");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_BUILD_ID");
    println!("cargo:rerun-if-changed=../VERSION");
    println!("cargo:rerun-if-changed=../scripts/product-version.py");
    watch_git_identity(repo_root);

    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_arch == "xtensa" && target_os == "none" {
        println!("cargo:rustc-link-arg=-Tlinkall.x");
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }
}

fn configure_rtd_assumption(build_mode: &str) {
    let offset_uv = std::env::var("FLUX_PURR_RTD_ASSUMED_ADC_OFFSET_UV")
        .unwrap_or_else(|_| "0".into())
        .parse::<i32>()
        .expect("FLUX_PURR_RTD_ASSUMED_ADC_OFFSET_UV must be an integer");
    assert!(
        (-20_000..=20_000).contains(&offset_uv),
        "assumed RTD ADC offset must be within +/-20000uV"
    );
    assert!(
        offset_uv == 0 || build_mode == "development",
        "nonzero assumed RTD ADC offset is restricted to development builds"
    );
    let out_dir = std::env::var_os("OUT_DIR").expect("OUT_DIR");
    std::fs::write(
        std::path::Path::new(&out_dir).join("rtd_adc_assumption.rs"),
        format!("pub(crate) const RTD_ASSUMED_ADC_OFFSET_UV: i32 = {offset_uv};\n"),
    )
    .expect("failed to write RTD ADC assumption configuration");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_RTD_ASSUMED_ADC_OFFSET_UV");
}

fn valid_source_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn watch_git_identity(repo_root: &std::path::Path) {
    let git_path = |args: &[&str]| {
        std::process::Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| {
                let path = std::path::PathBuf::from(value.trim());
                if path.is_absolute() {
                    path
                } else {
                    repo_root.join(path)
                }
            })
    };
    if let Some(path) = git_path(&["rev-parse", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    if let Some(reference) = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        && let Some(path) = git_path(&["rev-parse", "--git-path", reference.trim()])
    {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
