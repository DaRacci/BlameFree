//! API handler for admin endpoints.

use std::cmp::min;
use std::convert::Infallible;
use std::fs::{File, metadata};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;

use axum::extract::State;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use riv_stor::traits::Store;
use riv_webui_shared::routes::API_ADMIN_LOGS_STREAM;
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{instrument, warn};

use crate::routes_register;
use crate::server::AppState;
use riv_webui_shared::admin::LogsResponse;

const READBACK_LINES: usize = 500;

routes_register! {
  get API_ADMIN_LOGS_STREAM => get_logs_stream,
}

pub fn load_logs_response(log_path: &Path) -> LogsResponse {
    match read_last_n_lines(log_path, READBACK_LINES) {
        Ok(lines) => LogsResponse {
            logs: lines.join("\n"),
            available: true,
            message: None,
        },
        Err(e) => {
            warn!("Failed to read log file {}: {e}", log_path.display());
            LogsResponse {
                logs: String::new(),
                available: false,
                message: Some(format!("Error reading log file: {e}")),
            }
        }
    }
}

/// Get SSE stream of server console logs.
///
/// Frontend loads initial readback via regular request, so this SSE stream only
/// emits newly appended log content. New bytes are sent as batched chunks to
/// reduce browser re-render churn.
#[instrument(skip(state), name = API_ADMIN_LOGS_STREAM)]
pub async fn get_logs_stream<S>(State(state): State<AppState<S>>) -> impl IntoResponse
where
    S: Store + Send + Sync + Clone + 'static,
{
    let log_path = state.log_file.clone();
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();

    tokio::spawn(async move {
        let mut last_pos = match metadata(&log_path) {
            Ok(m) => m.len(),
            Err(_) => 0,
        };

        let mut interval = interval(Duration::from_secs(1));
        interval.reset(); // skip the immediate tick

        loop {
            interval.tick().await;

            let current_len = match metadata(&log_path) {
                Ok(m) => m.len(),
                Err(_) => continue,
            };

            if current_len < last_pos {
                last_pos = current_len;
                continue;
            }

            if current_len == last_pos {
                continue; // no new data
            }

            // Read bytes from last known position to current end
            let mut file = match File::open(&log_path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            if file.seek(SeekFrom::Start(last_pos)).is_err() {
                continue;
            }

            let mut buffer = Vec::new();
            if file.read_to_end(&mut buffer).is_err() {
                continue;
            }

            last_pos = current_len;

            let content = String::from_utf8_lossy(&buffer).to_string();
            if !content.is_empty() && tx.send(Ok(Event::default().data(content))).is_err() {
                return; // client disconnected
            }
        }
    });

    let stream = UnboundedReceiverStream::new(rx);
    Sse::new(stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

/// Read the last `n` lines from a text file efficiently.
///
/// Works by seeking near the end of the file and reading backwards,
/// which is O(1) in file length regardless of file size.
fn read_last_n_lines(path: &std::path::Path, n: usize) -> std::io::Result<Vec<String>> {
    const CHUNK_SIZE: u64 = 4096;

    let file = File::open(path)?;
    let metadata = file.metadata()?;

    let file_len = metadata.len();
    if file_len == 0 {
        return Ok(Vec::new());
    }

    let mut reader = BufReader::new(file);
    let mut lines = Vec::new();
    let mut buffer = Vec::new();
    let mut pos = file_len;

    while lines.len() < n && pos > 0 {
        let read_size = min(CHUNK_SIZE, pos);
        let new_pos = pos - read_size;

        reader.seek(SeekFrom::Start(new_pos))?;

        let mut chunk = vec![0u8; read_size as usize];
        reader.read_exact(&mut chunk)?;

        let mut new_buffer = chunk;
        new_buffer.append(&mut buffer);
        buffer = new_buffer;

        let content = String::from_utf8_lossy(&buffer);
        let content = if new_pos == 0 {
            // We're at the start of the file; use the whole buffer
            content.to_string()
        } else {
            // There may be a partial first line; split at the first newline
            let s = content.to_string();
            if let Some(nl_pos) = s.find('\n') {
                if let Some(rest) = s.get(nl_pos + 1..) {
                    rest.to_string()
                } else {
                    s
                }
            } else {
                s
            }
        };

        lines = content.lines().rev().map(String::from).collect();

        pos = new_pos;
    }

    lines.truncate(n);
    lines.reverse();

    Ok(lines)
}
