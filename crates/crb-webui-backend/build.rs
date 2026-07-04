use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=../crb-webui-frontend/src");
    println!("cargo::rerun-if-changed=../crb-webui-frontend/Cargo.toml");
    println!("cargo::rerun-if-changed=../crb-webui-frontend/index.html");
    println!("cargo::rerun-if-changed=../crb-webui-frontend/dist");

    let status = Command::new("trunk")
        .args(["build", "--release"])
        .current_dir("../crb-webui-frontend")
        .status()?;

    if !status.success() {
        return Err(format!(
            "Frontend build failed with exit code {}. \
             Fix frontend compilation errors before building the server. \
             Run 'cd crates/crb-webui/frontend && cargo check' to see errors.",
            status.code().unwrap_or(-1)
        )
        .into());
    }
    Ok(())
}
