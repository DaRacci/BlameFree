# riv-webui-backend

Axum backend for BlameFree web UI.

## Role

- serves REST/auth endpoints
- hosts Leptos SSR routes from `riv-webui-app`
- serves generated browser assets from `target/site/`

## Run

```bash
cargo run -p riv-webui-backend --bin riv-webui-backend -- --port 8080
```

## Development

```bash
cargo leptos watch
```

That command builds:
- server binary from `riv-webui-backend`
- WASM hydration bundle from `riv-webui-frontend`
- shared app from `riv-webui-app`

Generated assets land in `target/site/pkg/`.
