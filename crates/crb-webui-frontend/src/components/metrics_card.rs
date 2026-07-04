use leptos::{component, view, IntoView};

#[component]
pub fn MetricsCard(value: impl Into<String>, label: &'static str) -> impl IntoView {
    let value = value.into();
    view! {
        <div class="metric-card">
            <p class="metric-card__value">{&value}</p>
            <p class="metric-card__label">{label}</p>
        </div>
    }
}
