//! API handler for admin endpoints.
//!
//! Currently provides:
//! - `GET /api/admin/logs` — returns recent server console logs
//! - `GET /api/admin/logs/stream` — SSE stream of live logs
//!
//! This module is designed for future expansion with additional admin
//! features such as cache inspection, config management, etc.

use std::cmp::min;
use std::convert::Infallible;
use std::fs::{File, metadata};
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::sse::{Event, KeepAlive, Sse};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::server::AppState;
use crb_webui_shared::admin::LogsResponse;

/// GET /api/admin/logs return recent server console logs.
///
/// Reads the last 500 lines from the server's log file.
pub async fn get_logs(State(state): State<AppState>) -> Json<LogsResponse> {
    tracing::info!("GET /api/admin/logs");

    let log_path = &state.log_file;

    match read_last_n_lines(log_path, 500) {
        Ok(lines) => {
            let text = lines.join("\n");
            Json(LogsResponse {
                logs: text,
                available: true,
                message: None,
            })
        }
        Err(e) => {
            tracing::warn!("Failed to read log file {}: {e}", log_path.display());
            Json(LogsResponse {
                logs: String::new(),
                available: false,
                message: Some(format!("Error reading log file: {e}")),
            })
        }
    }
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

/// GET /api/admin/logs/stream — SSE stream of server console logs.
///
/// On first connection, sends the last 500 lines as a batch.
/// Then polls the log file every second for new lines and streams them.
/// Uses Server-Sent Events (SSE) for real-time log delivery.
pub async fn get_logs_stream(State(state): State<AppState>) -> impl IntoResponse {
    tracing::info!("GET /api/admin/logs/stream (SSE)");

    let log_path = state.log_file.clone();

    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();

    tokio::spawn(async move {
        match read_last_n_lines(&log_path, 500) {
            Ok(lines) => {
                for line in &lines {
                    if tx.send(Ok(Event::default().data(line.clone()))).is_err() {
                        return; // client disconnected
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to read initial log lines: {e}");
                // Continue anyway so polling can pick up future writes
            }
        }

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

            if current_len <= last_pos {
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

            let content = String::from_utf8_lossy(&buffer);
            for line in content.lines() {
                if tx
                    .send(Ok(Event::default().data(line.to_string())))
                    .is_err()
                {
                    return; // client disconnected
                }
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
