//! Puts the staged payload inside the installer binary.
//!
//! The release pipeline stages the product, zips it and points
//! `LOCALREEL_PAYLOAD` at the archive; the build script copies it into
//! `OUT_DIR` so `include_bytes!` can embed it. Version and display name come
//! from the same environment, which keeps the installer and the application
//! from disagreeing about what they are.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let payload =
        env::var("LOCALREEL_PAYLOAD").expect("LOCALREEL_PAYLOAD must point at the payload zip");
    let version = env::var("LOCALREEL_VERSION").unwrap_or_else(|_| "0.0.0".to_string());
    let payload = PathBuf::from(&payload);
    if !payload.is_file() {
        panic!("payload not found: {}", payload.display());
    }

    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let target = out.join("payload.zip");
    fs::copy(&payload, &target).expect("copy payload");

    println!("cargo:rerun-if-changed={}", payload.display());
    // Without these, Cargo keeps the previously compiled binary when only the
    // version or the payload changed, and the release ships last version's
    // installer with this version's name.
    println!("cargo:rerun-if-env-changed=LOCALREEL_PAYLOAD");
    println!("cargo:rerun-if-env-changed=LOCALREEL_VERSION");
    println!("cargo:rerun-if-env-changed=LOCALREEL_BUILD_STAMP");
    println!("cargo:rustc-env=LOCALREEL_VERSION={version}");
    println!("cargo:rustc-env=LOCALREEL_BUILT={}", build_stamp());
}

/// A compact build stamp, shown in the "installed" message and kept in
/// install.json so a support question can be traced to an artifact.
fn build_stamp() -> String {
    if let Ok(stamp) = env::var("LOCALREEL_BUILD_STAMP") {
        return stamp;
    }
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => format!("{}", duration.as_secs()),
        Err(_) => "0".to_string(),
    }
}
