# crb-webui-frontend

Leptos WASM frontend for the crb-webui dashboard — a client-side rendered SPA compiled to WebAssembly.

- Provides a dashboard overview, run detail view, live agent monitoring page, and new-run launcher
- Communicates with the Axum backend via REST API and Server-Sent Events (SSE)
- Built with [`Leptos`] 0.6 CSR mode, [`leptos_router`], and [`leptos_meta`]

## Key types

- `RunSummary`, `RunDetail`, `PrResult`, `AggregateMetrics` — API response types
- `NewRunRequest`, `NewRunResponse` — Launch-new-run types
- `AgentEvent` — SSE event type for live agent monitoring

## Building

```bash
# Build the WASM frontend (output goes to frontend/dist/)
cd crates/crb-webui/frontend
trunk build --release

# Or for development with hot-reload (runs on port 8081)
trunk serve --port 8081
```

## Development workflow with live reload

For a fast development loop:

1. Start the backend server in one terminal:
   ```bash
   cd /path/to/review-harness
   cargo run -p crb-webui -- --port 8080 --static-dir crates/crb-webui/frontend/dist
   ```

2. Start trunk's dev server with hot-reload and proxy:
   ```bash
   cd crates/crb-webui/frontend
   trunk serve --port 8081 --proxy-backend http://localhost:8080
   ```

3. Open http://localhost:8081 in your browser — the frontend auto-reloads on file changes. API requests are proxied to the backend at :8080.

When you're done, run `trunk build --release` from `crates/crb-webui/frontend/` to produce the final static bundle that the production server serves.
