//! Reporting for metrics, analytics, cost etc.

pub mod cost;
pub mod golden;
pub mod history;

use std::{fs, path::Path};

use anyhow::Result;
use crb_types::benchmark::result::PrResult;

/// Write per-PR JSON result files to `output_dir`.
///
/// Each PR gets `<sanitized-title>.json` with its full result.
pub fn write_report(_results: &[PrResult], output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;

    // Per-PR JSON
    todo!();
    // for result in results {
    //     let filename = sanitize_filename(&result.meta.title);
    //     let path = output_dir.join(format!("{filename}.json"));
    //     let json = serde_json::to_string_pretty(result)?;
    //     fs::write(&path, json)?;
    //     info!("Wrote per-PR result: {}", path.display());
    // }

    // Ok(())
}
