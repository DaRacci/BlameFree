use leptos::prelude::*;

/// Loading state variant selector.
#[derive(Debug, Clone, Copy)]
pub enum LoadingVariant {
    /// Two stacked skeleton cards.
    SkeletonCards,

    /// Skeleton grid (metrics, agent panes, or any card grid).
    SkeletonGrid {
        count: usize,
        grid_class: &'static str,
        item_height: &'static str,
    },

    /// A single skeleton text line wrapped in `form-loading`.
    FormSkeleton,

    /// Plain text fallback.
    Text(&'static str),
}

/// Loading placeholder.
#[component]
pub fn LoadingState(variant: LoadingVariant) -> impl IntoView {
    match variant {
        LoadingVariant::SkeletonCards => view! {
            <div class="mt-xl">
                <div class="skeleton skeleton--card mb-lg" style="height: 180px;"></div>
                <div class="skeleton skeleton--card" style="height: 300px;"></div>
            </div>
        }
        .into_any(),
        LoadingVariant::SkeletonGrid {
            count,
            grid_class,
            item_height,
        } => view! {
            <div class=format!("content-grid {}", grid_class)>
                {(0..count.max(1))
                    .map(|_| {
                        view! {
                            <div class="skeleton skeleton--card" style=format!("height: {};", item_height)></div>
                        }
                    })
                    .collect::<Vec<_>>()}
            </div>
        }
        .into_any(),
        LoadingVariant::FormSkeleton => view! {
            <div class="form-loading">
                <div class="skeleton skeleton--text"></div>
            </div>
        }
        .into_any(),
        LoadingVariant::Text(text) => view! {
            <div class="loading-state">
                <span class="text-secondary">{text}</span>
            </div>
        }
        .into_any(),
    }
}
