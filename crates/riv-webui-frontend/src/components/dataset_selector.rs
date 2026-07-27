use leptos::prelude::*;
use riv_webui_shared::config::{AppConfig, DatasetInfo};

/// A reusable dropdown for selecting a dataset.
#[component]
pub fn DatasetSelector(
    /// Application configuration signal
    config: ReadSignal<Option<AppConfig>>,
    /// Live dataset metadata from the `/api/config/datasets` endpoint.
    datasets: ReadSignal<Vec<DatasetInfo>>,
    /// Reactive current dataset value.
    dataset: ReadSignal<String>,
    /// Change handler
    on_change: impl Fn(leptos::ev::Event) + 'static,
) -> impl IntoView {
    view! {
        <div class="form-field">
            <label class="form-field__label" for="dataset">"Dataset"</label>
            <select id="dataset" class="input select"
                prop:value=dataset.get()
                on:change=on_change
            >
                {move || {
                    let ds = datasets.get();
                    if !ds.is_empty() {
                        ds.into_iter().map(|d| {
                            let is_selected = dataset.get() == d.id;
                            let label = format!("{} ({} PRs)", d.id, d.pr_count);
                            view! { <option value=d.id.clone() selected=is_selected>{label}</option> }
                        }).collect::<Vec<_>>()
                    } else {
                        let cfg = config.get();
                        let fallback = if let Some(ref c) = cfg {
                            c.datasets.clone()
                        } else {
                            vec![]
                        };

                        fallback.into_iter().map(|d| {
                            let is_selected = dataset.get() == d;
                            view! { <option value=d.clone() selected=is_selected>{d.clone()}</option> }
                        }).collect::<Vec<_>>()
                    }
                }}
            </select>
            <p class="form-field__helper">"The dataset used for evaluation"</p>
        </div>
    }
}
