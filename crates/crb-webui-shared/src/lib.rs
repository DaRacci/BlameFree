//! Shared types used by both `crb-webui` (backend) and `crb-webui-frontend` (WASM).
//!
//! This crate has minimal dependencies (`serde` + `serde_json`) to stay
//! WASM-compatible.  Every type defined here is `Serialize` + `Deserialize`
//! so the backend can send it over JSON and the frontend can receive it.

pub mod adhoc;
pub mod admin;
pub mod auth;
pub mod config;
pub mod runs;
