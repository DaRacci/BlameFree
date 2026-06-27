# crb-webui

Web UI dashboard server for the code review benchmark harness, built with [Axum].

- Serves a browser-based GUI with past run history, live agent monitoring via SSE, a benchmark launcher, and per-PR result viewer
- Exposes REST API endpoints for run config, run history, live events, and run management
- Serves the Leptos WASM frontend from `frontend/dist/`

## Key types

- [`CliArgs`](src/main.rs) — CLI flags: `--port`, `--output-dir`, `--dataset-dir`, `--harness-path`, `--static-dir`, `--models`
- [`AppState`](src/server.rs) — Shared application state holding output dir, dataset dir, harness path, and static dir

## CLI usage

```bash
cargo run -p crb-webui -- --port 8080

# With custom paths
cargo run -p crb-webui -- \
  --port 3000 \
  --output-dir /data/output \
  --dataset-dir /data/datasets \
  --harness-path ../target/release/crb-harness
```

## Development with live reload

For a fast development loop:

1. Start the backend server:
   ```bash
   cargo run -p crb-webui -- --port 8080
   ```

2. In another terminal, start trunk's dev server with hot-reload and proxy:
   ```bash
   cd crates/crb-webui/frontend
   trunk serve --port 8081 --proxy-backend http://localhost:8080
   ```

3. Open **http://localhost:8081** in your browser — the frontend auto-reloads on source changes. API requests are proxied to the backend at :8080.

When you're done, run `trunk build --release` to produce the final static bundle for production deployment.
