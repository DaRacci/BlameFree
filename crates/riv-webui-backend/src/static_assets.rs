use rust_embed::RustEmbed;

/// Embedded frontend static assets, built by trunk into `../riv-webui-frontend/dist/`.
#[derive(RustEmbed)]
#[folder = "../riv-webui-frontend/dist/"]
pub struct StaticAssets;
