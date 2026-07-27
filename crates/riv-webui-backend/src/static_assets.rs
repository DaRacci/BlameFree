/// Embedded frontend static assets, built by trunk into `../riv-webui-frontend/dist/`.
///
/// Only available when the `embed-frontend` feature is enabled.
#[cfg(feature = "embed-frontend")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../riv-webui-frontend/dist/"]
pub struct StaticAssets;
