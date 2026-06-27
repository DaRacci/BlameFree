use leptos::*;

#[component]
pub fn ProgressBar(
    value: u32,
    max: u32,
    label: String,
) -> impl IntoView {
    let pct = move || -> f64 {
        if max > 0 {
            (value as f64 / max as f64 * 100.0).min(100.0)
        } else {
            0.0
        }
    };

    view! {
        <div class="progress-bar" style="position: relative;">
            <div class="fill" style=move || format!("width: {}%", pct())></div>
            <div class="label">{label}</div>
        </div>
    }
}
