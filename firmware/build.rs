fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(&manifest_dir)
        .parent()
        .expect("firmware must live below the repository root");
    let resolver = repo_root.join("scripts/product-version.py");
    let build_mode = std::env::var("FLUX_PURR_BUILD_MODE").unwrap_or_else(|_| "development".into());
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
        .expect("FLUX_PURR_SOURCE_SHA or a Git checkout is required");
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
    let build_id = std::env::var("FLUX_PURR_BUILD_ID")
        .ok()
        .filter(|value| (16..=64).contains(&value.len()))
        .unwrap_or_else(|| source_sha[..16].to_string());

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
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }
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
