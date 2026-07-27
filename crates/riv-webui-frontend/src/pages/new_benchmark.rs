use std::sync::Arc;

use riv_types::{capabilities::ReasoningEffort, vcs::pr::PrMeta};
use riv_webui_shared::{
    config::{AppConfig, DatasetInfo},
    route,
    routes::{API_CONFIG, API_CONFIG_DATASETS, API_CONFIG_REASONING},
};
use leptos::prelude::*;
use leptos::reactive::{spawn_local, traits::Set};

use crate::components::dataset_selector::DatasetSelector;
use crate::components::form_page::FormPage;
use crate::components::model_selector::ModelSelector;
use crate::components::pr_selection::{PrItem, PrSelection};
use crate::components::reasoning_effort_selector::ReasoningEffortSelector;
use crate::signal_struct;

signal_struct! {
    #[derive(Debug)]
    struct NewBenchmarkSignals {
        config: Option<AppConfig> = None,
        config_loading: bool = true,
        config_error: Option<String> = None,

        datasets: Vec<DatasetInfo> = Vec::new(),
        datasets_loading: bool = true,

        model: String = String::new(),
        dataset: String = String::new(),

        available_prs: Vec<PrMeta> = Vec::new(),
        selected_prs: Vec<String> = Vec::new(),
        prs_loading: bool = false,

        reasoning_effort: Option<ReasoningEffort> = Some(ReasoningEffort::Medium),
        effort_levels: Vec<ReasoningEffort> = Vec::new(),
        effort_loading: bool = true,

        submitting: bool = false,
        submit_error: Option<String> = None,
    }
    write_only {
        set_submit_result: Option<String> = None,
    }
}

impl PrItem for PrMeta {
    fn pr_key(&self) -> &str {
        &self.title
    }

    fn pr_label(&self) -> String {
        self.title.clone()
    }
}

#[inline]
async fn fetch_json_and_signal<T>(
    url: &str,
    set_signal: WriteSignal<T>,
    error_signal: WriteSignal<Option<String>>,
    loading_signal: WriteSignal<bool>,
) -> Result<T, String>
where
    T: serde::de::DeserializeOwned + Send + Sync + Clone,
{
    let url = url.to_string();
    let res = match crate::fetch_json::<T>(&url).await {
        Ok(data) => {
            set_signal.set(data.clone());
            Ok(data)
        }
        Err(err) => {
            error_signal.set(Some(err.to_string()));
            Err(err.to_string())
        }
    };

    loading_signal.set(false);
    res
}

#[component]
pub fn NewBenchmarkPage() -> impl IntoView {
    let signals = NewBenchmarkSignals::new();

    let fetch_prs = Arc::new(create_fetch_prs_handler(signals.clone()));
    let dataset_change_handler = create_dataset_change_handler(signals.clone(), fetch_prs.clone());

    spawn_local({
        let signals = signals.clone();
        async move {
            match crate::fetch_json::<AppConfig>(&route!(API_CONFIG)).await {
                Ok(config) => {
                    signals.set_config.set(Some(config));
                }
                Err(err) => {
                    signals.set_config_error.set(Some(err.to_string()));
                }
            }
            signals.set_config_loading.set(false);

            match crate::fetch_json::<Vec<DatasetInfo>>(&route!(API_CONFIG_DATASETS)).await {
                Ok(datasets) => {
                    signals.set_datasets.set(datasets);
                }
                Err(err) => {
                    signals.set_config_error.set(Some(err.to_string()));
                }
            }
            signals.set_datasets_loading.set(false);

            match crate::fetch_json::<Vec<ReasoningEffort>>(&route!(API_CONFIG_REASONING)).await {
                Ok(levels) => {
                    signals.set_effort_levels.set(levels);
                }
                Err(_) => {}
            }
            signals.set_effort_loading.set(false);
        }
    });

    view! { <NewBenchmarkPageView signals=signals dataset_change_handler=dataset_change_handler /> }
}

fn create_fetch_prs_handler(signals: NewBenchmarkSignals) -> impl Fn(String) {
    move |ds_id: String| {
        if ds_id.is_empty() {
            signals.set_available_prs.set(Vec::new());
            signals.set_selected_prs.set(Vec::new());
            return;
        }
        signals.set_prs_loading.set(true);
        spawn_local(async move {
            match crate::fetch_json::<Vec<PrMeta>>(&route!(API_DATASETS_ID_PRS, ds_id)).await {
                Ok(prs) => {
                    let all_keys: Vec<String> = prs.iter().map(|p| p.title.clone()).collect();
                    signals.set_available_prs.set(prs);
                    signals.set_selected_prs.set(all_keys);
                }
                Err(_) => {
                    signals.set_available_prs.set(Vec::new());
                    signals.set_selected_prs.set(Vec::new());
                }
            }
            signals.set_prs_loading.set(false);
        });
    }
}

fn create_dataset_change_handler(
    signals: NewBenchmarkSignals,
    fetch_prs: Arc<dyn Fn(String) + 'static>,
) -> impl Fn(leptos::ev::Event) {
    move |ev: leptos::ev::Event| {
        let new_ds = event_target_value(&ev);
        signals.set_dataset.set(new_ds.clone());

        fetch_prs(new_ds);
    }
}

#[component]
fn NewBenchmarkPageView(
    signals: NewBenchmarkSignals,
    dataset_change_handler: impl Fn(leptos::ev::Event) + 'static,
) -> impl IntoView {
    let on_submit = create_benchmark_submit_handler(signals);

    view! {
        <FormPage
            title="New Benchmark"
            submit_label="Create Benchmark"
            config_loading=signals.config_loading
            config_error=signals.config_error
            submitting=signals.submitting
            submit_error=signals.submit_error
            on_submit=on_submit
            submit_disabled=move || false
        >
            <section class="form-section">
                <h2 class="form-section__title">"Configuration"</h2>
                <div class="form-section__fields">
                    <ModelSelector
                        config=signals.config
                        model=signals.model
                        set_model=signals.set_model
                    />
                    <DatasetSelector
                        config=signals.config
                        datasets=signals.datasets
                        dataset=signals.dataset
                        on_change=dataset_change_handler
                    />
                </div>
            </section>

            <PrSelection<PrMeta>
                prs_loading=signals.prs_loading
                available_prs=signals.available_prs
                selected_prs=signals.selected_prs
                set_selected_prs=signals.set_selected_prs
                empty_message="Select a dataset to see available PRs."
                helper_text="Uncheck PRs you want to skip. All PRs selected = run entire dataset."
            />

            <section class="form-section">
                <h2 class="form-section__title">"Advanced"</h2>
                <div class="form-section__fields">
                    <ReasoningEffortSelector
                        reasoning_effort=signals.reasoning_effort
                        set_reasoning_effort=signals.set_reasoning_effort
                        effort_levels=signals.effort_levels
                        effort_loading=signals.effort_loading
                    />
                </div>
            </section>
        </FormPage>
    }
}

fn create_benchmark_submit_handler(
    signals: NewBenchmarkSignals,
) -> impl Fn(leptos::ev::SubmitEvent) {
    move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        signals.set_submitting.set(true);
        signals.set_submit_error.set(None);
        signals.set_submit_result.set(None);

        spawn_local(async move {
            signals.set_submitting.set(false);
            signals
                .set_submit_error
                .set(Some("Benchmark submission not yet implemented".to_string()));
        });
    }
}
