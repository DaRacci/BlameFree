use leptos::prelude::*;

#[component]
pub fn MetricsCard(
    value: impl Into<String>,
    label: &'static str,
    #[prop(optional)] value_style: Option<&'static str>,
    #[prop(optional, default = false)] truncate: bool,
) -> impl IntoView {
    let value = value.into();
    let title = value.clone();
    let value_class = if truncate {
        "metric-card__value metric-card__value--truncate"
    } else {
        "metric-card__value"
    };
    view! {
        <div class="metric-card">
            <p class=value_class style=value_style title=title>{value}</p>
            <p class="metric-card__label">{label}</p>
        </div>
    }
}
