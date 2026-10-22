//! Builds the Swift shim with SwiftPM and emits the link flags
//! (07-build-and-packaging.md §Shim build).
//!
//! A build script is init-only code; `expect` is acceptable here.

#![allow(clippy::expect_used)]

use std::path::PathBuf;
use std::process::Command;

fn main() {
    assert_eq!(
        std::env::var("CARGO_CFG_TARGET_OS").as_deref(),
        Ok("macos"),
        "Lalela builds only on macOS"
    );
    assert_eq!(
        std::env::var("CARGO_CFG_TARGET_ARCH").as_deref(),
        Ok("aarch64"),
        "Lalela builds only on Apple silicon"
    );

    let deployment_target =
        std::env::var("MACOSX_DEPLOYMENT_TARGET").expect("deployment target env");
    assert_eq!(
        deployment_target, "27.0",
        "MACOSX_DEPLOYMENT_TARGET must be 27.0 (it was {deployment_target})"
    );

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let shim_dir = manifest_dir.join("..").join("..").join("shim");
    assert!(
        shim_dir.join("Package.swift").exists(),
        "shim package missing"
    );

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("out dir"));
    let scratch = out_dir.join("shim-build");

    let status = Command::new("swift")
        .arg("build")
        .arg("-c")
        .arg("release")
        .arg("--package-path")
        .arg(&shim_dir)
        .arg("--arch")
        .arg("arm64")
        .arg("--scratch-path")
        .arg(&scratch)
        .status()
        .expect("swift build failed to start");
    assert!(
        status.success(),
        "swift build failed; see diagnostics above"
    );

    let lib_dir = scratch.join("release");
    assert!(
        lib_dir.join("libLalelaShim.a").exists(),
        "shim static library missing"
    );

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=LalelaShim");
    for framework in [
        "Speech",
        "AVFoundation",
        "CoreMedia",
        "AppKit",
        "Foundation",
    ] {
        println!("cargo:rustc-link-lib=framework={framework}");
    }

    let sdk = std::process::Command::new("xcrun")
        .arg("--show-sdk-path")
        .output()
        .expect("xcrun failed");
    assert!(sdk.status.success());
    let sdk_path = String::from_utf8(sdk.stdout).expect("sdk path utf8");
    let sdk_path = sdk_path.trim();
    let swift_lib = PathBuf::from(sdk_path).join("usr/lib/swift");
    assert!(
        swift_lib.exists(),
        "swift runtime lib missing at {swift_lib:?}"
    );
    println!("cargo:rustc-link-search=native={}", swift_lib.display());

    let swift_path = std::process::Command::new("xcrun")
        .arg("--find")
        .arg("swift")
        .output()
        .expect("xcrun --find swift failed");
    assert!(swift_path.status.success());
    let swift_path = String::from_utf8(swift_path.stdout).expect("swift path utf8");
    let toolchain_swift_lib = PathBuf::from(swift_path.trim())
        .parent()
        .expect("swift parent")
        .parent()
        .expect("bin parent")
        .join("lib/swift/macosx");
    assert!(toolchain_swift_lib.exists(), "toolchain swift lib missing");
    println!(
        "cargo:rustc-link-search=native={}",
        toolchain_swift_lib.display()
    );
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    println!(
        "cargo:rerun-if-changed={}",
        shim_dir.join("Sources").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        shim_dir.join("include").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        shim_dir.join("Package.swift").display()
    );
}
