use leptos::*;

#[component]
pub fn MetricsCard(
    value: impl Into<String>,
    label: &'static str,
) -> impl IntoView {
    let value = value.into();
    view! {
        <div class="metric-card">
            <div class="value">{&value}</div>
            <div class="label">{label}</div>
        </div>
    }
}
