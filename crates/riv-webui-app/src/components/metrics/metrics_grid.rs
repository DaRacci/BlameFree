use leptos::prelude::*;

/// Metrics-card grid wrapper.
#[component]
pub fn MetricsGrid(children: Children) -> impl IntoView {
    view! {
        <div class="content-grid content-grid--metrics">{children()}</div>
    }
}

/// Skeleton placeholder for a metrics grid.
#[component]
pub fn MetricsGridSkeleton(#[prop(optional, default = 4)] count: usize) -> impl IntoView {
    let count = count.max(1);

    view! {
        <MetricsGrid>
            {(0..count)
                .map(|_| view! { <div class="skeleton skeleton--metric"></div> })
                .collect::<Vec<_>>()}
        </MetricsGrid>
    }
}
