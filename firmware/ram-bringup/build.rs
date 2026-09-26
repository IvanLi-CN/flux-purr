use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("ram-bringup must live below firmware");
    let source_sha = match env::var("FLUX_PURR_SOURCE_SHA") {
        Ok(value) => {
            if !valid_source_sha(&value) {
                panic!(
                    "FLUX_PURR_SOURCE_SHA must be a 40-character lowercase hexadecimal commit SHA"
                );
            }
            value
        }
        Err(_) => Command::new("git")
            .current_dir(repo_root)
            .args(["rev-parse", "HEAD"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| valid_source_sha(value))
            .expect("FLUX_PURR_SOURCE_SHA or a Git checkout is required"),
    };
    let expected_build_id = &source_sha[..16];
    let build_id = match env::var("FLUX_PURR_BUILD_ID") {
        Ok(value) if value == expected_build_id => value,
        Ok(value) => panic!(
            "FLUX_PURR_BUILD_ID {value:?} does not match source SHA prefix {expected_build_id:?}"
        ),
        Err(_) => expected_build_id.to_string(),
    };

    println!("cargo:rustc-env=FLUX_PURR_RAM_BUILD_ID={build_id}");
    println!("cargo:rustc-env=FLUX_PURR_RAM_SOURCE_SHA={source_sha}");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_BUILD_ID");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_SOURCE_SHA");
    println!("cargo:rerun-if-changed=memory.x");

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_arch == "xtensa" && target_os == "none" {
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
        let link_script = out_dir.join("ram-linkall.x");
        let memory = manifest_dir.join("memory.x");
        fs::write(
            &link_script,
            format!(
                "INCLUDE \"{}\"\nINCLUDE \"esp32s3.x\"\nINCLUDE \"hal-defaults.x\"\n",
                memory.display()
            ),
        )
        .expect("write RAM linker script");
        println!("cargo:rustc-link-arg=-T{}", link_script.display());
    }
}

fn valid_source_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
