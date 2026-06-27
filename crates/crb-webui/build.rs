use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=frontend");

    Command::new("trunk")
        .args(&["build", "--release"])
        .current_dir("frontend")
        .status()
        .expect("Failed to build frontend with trunk");
}
