use leptos::prelude::*;

#[component]
pub fn MetricsCard(
    value: impl Into<String>,
    label: &'static str,
    #[prop(optional)] value_style: Option<&'static str>,
) -> impl IntoView {
    let value = value.into();
    view! {
        <div class="metric-card">
            <p class="metric-card__value" style=value_style>{value}</p>
            <p class="metric-card__label">{label}</p>
        </div>
    }
}
