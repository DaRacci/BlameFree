use std::path::Path;

use anyhow::Result;
use tracing::info;

/// Extract diffs from scaffolded repos.
pub fn run(repos_dir: &Path, output_dir: &Path) -> Result<()> {
    if !repos_dir.exists() {
        anyhow::bail!("Repos directory does not exist: {}", repos_dir.display());
    }

    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir)?;
        info!("Created output directory: {}", output_dir.display());
    }

    info!("Scanning repos in: {}", repos_dir.display());

    let mut diff_count = 0;
    for entry in std::fs::read_dir(repos_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let repo_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            info!(
                "[PLACEHOLDER] Would extract diffs from repo: {}",
                repo_name
            );
            diff_count += 1;
        }
    }

    info!(
        "Fetch-diffs complete: {} repo(s) scanned, diffs would go to {}",
        diff_count,
        output_dir.display()
    );

    if diff_count == 0 {
        println!("No repos found in {}. Run `crb-benchmark scaffold` first.", repos_dir.display());
    }

    Ok(())
}
