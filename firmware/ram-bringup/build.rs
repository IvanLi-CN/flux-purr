use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    fs::write(out.join("linkall.x"), include_bytes!("ram-linkall.x"))
        .expect("write RAM linker script");
    fs::write(out.join("ram-memory.x"), include_bytes!("ram-memory.x"))
        .expect("write RAM memory script");
    fs::write(out.join("ram-esp32s3.x"), include_bytes!("ram-esp32s3.x"))
        .expect("write RAM ESP32-S3 linker fragment");
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rerun-if-changed=ram-linkall.x");
    println!("cargo:rerun-if-changed=ram-memory.x");
    println!("cargo:rerun-if-changed=ram-esp32s3.x");
}
