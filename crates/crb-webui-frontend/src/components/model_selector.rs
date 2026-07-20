use crb_webui_shared::config::AppConfig;
use leptos::prelude::*;

/// A reusable dropdown for selecting a model.
#[allow(clippy::redundant_clone)]
#[component]
pub fn ModelSelector(
    /// Reactive config whose `models` field populates the dropdown.
    config: ReadSignal<Option<AppConfig>>,
    /// Currently selected model value.
    model: ReadSignal<String>,
    /// Write-signal to update the selected model.
    set_model: WriteSignal<String>,
) -> impl IntoView {
    view! {
        <div class="form-field">
            <label class="form-field__label" for="model">"Model"</label>
            <select id="model" class="input select"
                prop:value=model.get()
                on:change=move |ev| {
                    set_model.set(event_target_value(&ev));
                }
            >
                {move || {
                    let cfg = config.get();
                    let models = if let Some(ref c) = cfg {
                        c.models.clone()
                    } else {
                        vec![]
                    };

                    models.into_iter().map(|m| {
                        let is_selected = model.get() == m;
                        view! { <option value=m.clone() selected=is_selected>{m.clone()}</option> }
                    }).collect::<Vec<_>>()
                }}
            </select>
            <p class="form-field__helper">"The model used for review agents"</p>
        </div>
    }
}
