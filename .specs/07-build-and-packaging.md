# 07 — Build and Packaging

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines how Lalela is built for macOS 27 and later: toolchain pinning, the deployment target, how `build.rs` compiles and links the Swift shim, and how `Lalela.app` is bundled. The tool versions are listed in [development-guidelines.md](development-guidelines.md#toolchain). The crate layout is in [architecture-principles.md](architecture-principles.md#workspace-layout).

---

## Responsibilities

1. Produce an `aarch64-apple-darwin` binary whose minimum OS is macOS 27.0.
2. Build `LalelaShim` from Swift sources as part of `cargo build`, with no manual step.
3. Bundle the binary as `Lalela.app` with the Info.plist keys the app needs.

---

## Target and deployment floor

| File | Setting | Effect |
|---|---|---|
| `rust-toolchain.toml` | `channel = "1.95"` (or later pinned stable), `targets = ["aarch64-apple-darwin"]`, `components = ["rustfmt", "clippy"]` | Reproducible toolchain |
| `.cargo/config.toml` | `[build] target = "aarch64-apple-darwin"` | No accidental host-default builds |
| `.cargo/config.toml` | `[env] MACOSX_DEPLOYMENT_TARGET = "27.0"` | rustc and the linker stamp `minos 27.0` |
| `shim/Package.swift` | `platforms: [.macOS("27.0")]` | Swift code may use macOS 27 APIs without availability checks |
| `Cargo.toml` `[package.metadata.bundle.osx]` | `minimum_system_version = "27.0"` | `LSMinimumSystemVersion` in Info.plist |

The four declarations of `27.0` must agree. `lalela-macos/build.rs` reads `MACOSX_DEPLOYMENT_TARGET` and fails the build when it is not `27.0`, so they cannot drift silently.

---

## Shim build — `lalela-macos/build.rs`

```
1. assert target_os == "macos" and target_arch == "aarch64"; else fail the build
2. assert MACOSX_DEPLOYMENT_TARGET == "27.0"
3. run: swift build -c release --package-path <repo>/shim --arch arm64
        --scratch-path $OUT_DIR/shim-build
4. emit:
   cargo:rustc-link-search=native=$OUT_DIR/shim-build/release
   cargo:rustc-link-lib=static=LalelaShim
   cargo:rustc-link-lib=framework=Speech
   cargo:rustc-link-lib=framework=AVFoundation
   cargo:rustc-link-lib=framework=CoreMedia
   cargo:rustc-link-lib=framework=AppKit
   cargo:rustc-link-lib=framework=Foundation
   cargo:rustc-link-search=native=$(xcrun --show-sdk-path)/usr/lib/swift
   cargo:rustc-link-search=native=<toolchain>/usr/lib/swift/macosx
   cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift
5. cargo:rerun-if-changed=<repo>/shim/Sources, shim/include, shim/Package.swift
```

- `swift build` output goes into Cargo's `OUT_DIR`, so `cargo clean` removes it and nothing is written into the source tree.
- A failed `swift build` fails `cargo build`, and Swift's diagnostics are forwarded through `cargo:warning=` lines.
- The `<toolchain>` path comes from `xcrun --find swift`. `build.rs` asserts each path it emits exists.

`shim/Package.swift` declares one library product, `LalelaShim`, with `type: .static`, Swift 6 language mode, and the C header in `include/` as the public module map.

---

## Bundle — cargo-bundle

`crates/lalela-app/Cargo.toml`:

```toml
[package.metadata.bundle]
name = "Lalela"
identifier = "io.senzo.lalela"
version = "0.1.0"
category = "public.app-category.productivity"
icon = ["assets/icon.icns"]

[package.metadata.bundle.osx]
minimum_system_version = "27.0"
info_plist_exts = ["Info.ext.plist"]
```

cargo-bundle 0.12 rejects unknown keys in both tables, and the macOS-only keys live under `.osx`. It writes `LSApplicationCategoryType` from `category` and sets `NSHighResolutionCapable` itself, so neither key appears in the extension file.

`crates/lalela-app/Info.ext.plist` holds bare `<key>`/`<string>` pairs, with no XML header and no `<plist>` or `<dict>` wrapper, because cargo-bundle appends the file's contents inside the generated `<dict>`. It adds:

| Key | Value |
|---|---|
| `NSSpeechRecognitionUsageDescription` | "Lalela transcribes audio files on this Mac." |

No microphone key is present. Lalela reads files only.

Build command: `cargo bundle --release -p lalela-app`, producing `target/aarch64-apple-darwin/release/bundle/osx/Lalela.app`.

---

## Commands

| Task | Command |
|---|---|
| Debug build and run | `cargo run -p lalela-app` |
| Tests | `cargo nextest run --workspace` and `cargo test --doc --workspace` |
| Live provider tests | `MISTRAL_API_KEY=… DEEPGRAM_API_KEY=… cargo nextest run --run-ignored only` |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `swift format lint --strict --recursive shim/` |
| Bundle | `cargo bundle --release -p lalela-app` |

---

## Assumptions and open questions

**Assumptions**

- macOS 27 runs only on Apple silicon, so `aarch64-apple-darwin` is the only target.
- The Swift runtime ships with macOS at `/usr/lib/swift`, so the bundle does not embed Swift libraries.
- Xcode 27, or its Command Line Tools, is installed on build machines. It provides `swift`, `xcrun`, and the macOS 27 SDK.
- cargo-bundle 0.12 reads `minimum_system_version` and `info_plist_exts` from `[package.metadata.bundle.osx]`, and appends extension files to the generated Info.plist without de-duplicating keys.

**Decisions**

- *Bundler.* **cargo-bundle.** It covers Info.plist generation and `.app` layout with the least configuration. cargo-packager adds installers that Lalela does not need.
- *Shim build driver.* **SwiftPM invoked from `build.rs`.** SwiftPM resolves the module map and Swift flags. A raw `swiftc` invocation would duplicate them in Rust.
- *Deployment-target guard.* **`build.rs` fails on mismatch.** A binary stamped with a lower `minos` than the Swift code assumes would crash at load on older systems instead of being refused by the OS.

**Open questions**

- *Code signing and notarisation.* Will `Lalela.app` be signed and notarised, and if so, with which identity, and does the hardened runtime need any entitlements for the Keychain or Speech assets? No signing step exists yet.
- *Icon.* `assets/icon.icns` is named but has no design. Who provides it?
- *Speech usage key.* If `SpeechAnalyzer` file transcription needs no authorisation, should `NSSpeechRecognitionUsageDescription` be removed?
