use leptos::prelude::*;

#[component]
pub fn ProgressBar(value: u32, max: u32, label: String) -> impl IntoView {
    let pct = move || -> f64 {
        if max > 0 {
            (value as f64 / max as f64 * 100.0).min(100.0)
        } else {
            0.0
        }
    };

    let is_complete = move || max > 0 && value >= max;

    view! {
        <div
            class=move || {
                let mut cls = "progress".to_string();
                if is_complete() { cls.push_str(" progress--complete"); }
                cls
            }
            role="progressbar"
            aria-valuenow=value
            aria-valuemin=0u32
            aria-valuemax=max
        >
            <div class="progress__track">
                <div class="progress__fill" style=move || format!("width: {}%", pct())></div>
            </div>
            <span class="progress__label">{label}</span>
        </div>
    }
}
