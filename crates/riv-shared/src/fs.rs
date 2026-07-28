use std::path::Path;
use std::{fs, time};

/// Compute directory duration from file modification timestamps (max - min).
///
/// Returns 0.0 if no valid timestamps are found.
pub fn compute_duration_from_dir(dir: &Path) -> f64 {
    let mut oldest = f64::MAX;
    let mut newest = 0.0f64;

    let Ok(entries) = fs::read_dir(dir) else {
        return 0.0;
    };

    for entry in entries.flatten() {
        let Ok(meta) = entry.path().metadata() else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };

        let secs = modified
            .duration_since(time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();

        if secs <= 0.0 {
            continue;
        }

        oldest = oldest.min(secs);
        newest = newest.max(secs);
    }

    if !(newest > oldest && oldest < f64::MAX) {
        return 0.0;
    }

    newest - oldest
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_compute_duration_from_dir() {
        let temp_dir = tempfile::tempdir().unwrap();
        let dir_path = temp_dir.path();

        let file1_path = dir_path.join("file1.txt");
        let file2_path = dir_path.join("file2.txt");

        let mut file1 = fs::File::create(&file1_path).unwrap();
        let mut file2 = fs::File::create(&file2_path).unwrap();
        writeln!(file1, "Hello, world!").unwrap();
        writeln!(file2, "Hello, Rust!").unwrap();

        let now = SystemTime::now();
        let filetime_1 = fs::FileTimes::new().set_modified(now - Duration::from_secs(10));
        let filetime_2 = fs::FileTimes::new().set_modified(now - Duration::from_secs(5));
        let file_1 = fs::File::open(&file1_path).unwrap();
        let file_2 = fs::File::open(&file2_path).unwrap();
        fs::File::set_times(&file_1, filetime_1).unwrap();
        fs::File::set_times(&file_2, filetime_2).unwrap();
        drop(file_1);
        drop(file_2);

        // Read back actual mtimes as the FS may truncate/round/silently-ignore timestamps
        let actual_1 = fs::metadata(&file1_path).unwrap().modified().unwrap();
        let actual_2 = fs::metadata(&file2_path).unwrap().modified().unwrap();
        let actual_diff = actual_2
            .duration_since(actual_1)
            .unwrap_or_default()
            .as_secs_f64();

        // If FS didn't honor set_times (e.g. overlayfs, tmpfs), skip assertion
        if actual_diff < 0.5 {
            eprintln!("FS did not honor set_times (diff={actual_diff:.1}s). Skipping assertion.");
            return;
        }

        let duration = compute_duration_from_dir(dir_path);
        assert!((duration - actual_diff).abs() < 0.01);
    }
}
