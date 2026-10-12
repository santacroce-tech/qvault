use std::path::PathBuf;

fn main() {
    // Lets the opt-in sidecar integration test find binaries/qvac-sidecar-<triple>.
    println!("cargo:rustc-env=QVAULT_TARGET_TRIPLE={}", std::env::var("TARGET").unwrap_or_default());
    ensure_sidecar_placeholder();
    tauri_build::build()
}

/// tauri-build refuses to compile when a configured `externalBin` or resource
/// is missing. Release bundles get the real QVAC sidecar from
/// `npm run sidecar:build` (wired into `beforeBuildCommand`); for plain
/// `cargo build` / `cargo test` we create an empty placeholder, which the app
/// detects at runtime (zero-byte binary, no worker.js) and reports as "AI
/// engine not bundled" — Layers 1-2 keep working.
fn ensure_sidecar_placeholder() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("android") || target.contains("ios") {
        return; // the sidecar is desktop-only
    }
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let exe = if target.contains("windows") { ".exe" } else { "" };
    let bin = manifest.join("binaries").join(format!("qvac-sidecar-{}{}", target, exe));
    println!("cargo:rerun-if-changed={}", bin.display());
    if !bin.exists() {
        let _ = std::fs::create_dir_all(bin.parent().unwrap());
        let _ = std::fs::write(&bin, b"");
        println!(
            "cargo:warning=QVAC sidecar not built for {}; AI features will be unavailable in this build (run `npm run sidecar:build`)",
            target
        );
    }
    let res = manifest.join("resources").join("qvac-worker");
    if !res.exists() {
        let _ = std::fs::create_dir_all(&res);
        let _ = std::fs::write(res.join("PLACEHOLDER"), b"QVAC sidecar not built; run `npm run sidecar:build`.\n");
    }
}
