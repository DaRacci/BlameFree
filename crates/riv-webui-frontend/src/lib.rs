// Required for getrandom wasm_js feature on wasm32 target
#[cfg(target_arch = "wasm32")]
use getrandom as _;

use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(start)]
pub fn hydrate() {
    _ = console_log::init_with_level(log::Level::Debug);
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(riv_webui_app::App);
}
