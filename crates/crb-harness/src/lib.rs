use std::time::Duration;

use anyhow::Result;
use tracing::warn;

#[cfg(feature = "binary")]
pub mod config;
pub mod eval;
pub mod finding;
pub mod model_capabilities;
pub mod paths;
pub mod pipeline;
pub mod review;
#[cfg(test)]
pub mod test_utils;

/// Describes which kind of diff to review.
pub enum ReviewMode {
    /// Review a commit range `base..head`.
    Commits { base: String, head: String },

    /// Review the current working tree (unstaged + staged).
    Working,
}

/// Call an async function with exponential backoff retry.
#[doc(hidden)]
pub async fn with_retry<F, Fut, T, E>(f: F, max_retries: usize, base_delay_ms: u64) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0usize;
    loop {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    return Err(e);
                }
                let delay = Duration::from_millis(base_delay_ms * 2u64.pow(attempt as u32));
                warn!(
                    "Attempt {}/{} failed: {}. Retrying in {}ms",
                    attempt,
                    max_retries,
                    e,
                    delay.as_millis()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_mode_commits() {
        let mode = ReviewMode::Commits {
            base: "HEAD~3".to_string(),
            head: "HEAD".to_string(),
        };
        match mode {
            ReviewMode::Commits { base, head } => {
                assert_eq!(base, "HEAD~3");
                assert_eq!(head, "HEAD");
            }
            _ => panic!("Expected Commits variant"),
        }
    }

    #[test]
    fn review_mode_working() {
        let mode = ReviewMode::Working;
        match mode {
            ReviewMode::Working => {} // ok
            _ => panic!("Expected Working variant"),
        }
    }
}
