use rust_embed::RustEmbed;

/// Embedded frontend static assets, built by trunk into `../crb-webui-frontend/dist/`.
#[derive(RustEmbed)]
#[folder = "../crb-webui-frontend/dist/"]
pub struct StaticAssets;
