use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=../riv-webui-frontend/src");
    println!("cargo::rerun-if-changed=../riv-webui-frontend/Cargo.toml");
    println!("cargo::rerun-if-changed=../riv-webui-frontend/index.html");

    let status = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir("../riv-webui-frontend")
        .status()?;

    if !status.success() {
        return Err(format!(
            "Frontend build failed with exit code {}. \
             Fix frontend compilation errors before building the server. \
             Run 'cd crates/riv-webui/frontend && cargo check' to see errors.",
            status.code().unwrap_or(-1)
        )
        .into());
    }
    Ok(())
}
