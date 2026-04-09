fn main() {
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_arch == "xtensa" && target_os == "none" {
        println!("cargo:rustc-link-arg=-Tdefmt.x");
    }
}
