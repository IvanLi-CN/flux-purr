use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("devd must live below the repository root");
    let resolver = repo_root.join("scripts/product-version.py");
    let mode = env::var("FLUX_PURR_BUILD_MODE").unwrap_or_else(|_| "development".into());
    let source_sha = env::var("FLUX_PURR_SOURCE_SHA").ok();
    let mut command = Command::new("python3");
    command
        .current_dir(repo_root)
        .arg(&resolver)
        .args(["--mode", &mode, "--format", "tsv"]);
    if let Some(source_sha) = source_sha.as_deref() {
        command.args(["--source-sha", source_sha]);
    }
    let output = command.output().expect("failed to run product-version.py");
    if !output.status.success() {
        panic!(
            "product-version.py failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let fields: Vec<_> = String::from_utf8(output.stdout)
        .expect("product-version.py output is not UTF-8")
        .trim()
        .split('\t')
        .map(str::to_owned)
        .collect();
    assert_eq!(
        fields.len(),
        4,
        "product-version.py output must contain four fields"
    );
    for (name, value) in [
        ("FLUX_PURR_PRODUCT_VERSION", &fields[0]),
        ("FLUX_PURR_PRODUCT_CHANNEL", &fields[1]),
        ("FLUX_PURR_PRODUCT_SOURCE_SHA", &fields[2]),
        ("FLUX_PURR_PRODUCT_BUILD_ID", &fields[3]),
    ] {
        println!("cargo:rustc-env={name}={value}");
    }
    println!("cargo:rerun-if-env-changed=FLUX_PURR_BUILD_MODE");
    println!("cargo:rerun-if-env-changed=FLUX_PURR_SOURCE_SHA");
    println!(
        "cargo:rerun-if-changed={}",
        repo_root.join("VERSION").display()
    );
    println!("cargo:rerun-if-changed={}", resolver.display());
    watch_git_identity(repo_root);
}

fn watch_git_identity(repo_root: &std::path::Path) {
    let git_path = |args: &[&str]| {
        Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|value| {
                let path = PathBuf::from(value.trim());
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
    if let Some(reference) = Command::new("git")
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
