# riv-webui-frontend

Thin WASM hydration shim for BlameFree web UI.

## Role

- depends on `riv-webui-app` with `hydrate` feature
- builds `cdylib`/`rlib` output for `cargo-leptos`
- exports browser entrypoint that hydrates server-rendered HTML

All application UI now lives in `crates/riv-webui-app`.

## Development

```bash
cargo leptos watch
```

`cargo-leptos` builds this crate for `wasm32-unknown-unknown` and writes browser assets to `target/site/pkg/`.
