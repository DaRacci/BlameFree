use std::process::Command;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=frontend/src");
    println!("cargo::rerun-if-changed=frontend/Cargo.toml");
    println!("cargo::rerun-if-changed=frontend/index.html");

    let status = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir("frontend")
        .status()
        .expect("Failed to execute 'trunk build' — is trunk installed? (cargo install trunk)");

    if !status.success() {
        panic!(
            "Frontend build failed with exit code {}. \
             Fix frontend compilation errors before building the server. \
             Run 'cd crates/crb-webui/frontend && cargo check' to see errors.",
            status.code().unwrap_or(-1)
        );
    }

    // Note: cargo check does NOT run build scripts, so a `cargo check`
    // won't rebuild the frontend WASM. Always `cargo build` the webui
    // crate before deploying to ensure the frontend is up to date.
}
