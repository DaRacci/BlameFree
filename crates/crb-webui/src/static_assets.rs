use rust_embed::RustEmbed;

/// Embedded frontend static assets, built by trunk into `frontend/dist/`.
#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
pub struct StaticAssets;
