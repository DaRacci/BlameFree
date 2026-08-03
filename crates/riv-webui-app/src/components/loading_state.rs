use leptos::prelude::*;

/// Loading state variant selector.
#[derive(Debug, Clone, Copy)]
pub enum LoadingVariant {
    /// Two stacked skeleton cards.
    SkeletonCards,

    /// Skeleton grid (metrics, agent panes, or any card grid).
    /// `count` controls number of items, `grid_class` selects grid CSS class.
    SkeletonGrid,

    /// A single skeleton text line wrapped in `form-loading`.
    FormSkeleton,

    /// Plain text fallback, optionally with a label.
    Text,
}

/// Loading placeholder.
#[component]
pub fn LoadingState(
    variant: LoadingVariant,
    #[prop(optional)] label: Option<&'static str>,
    #[prop(optional)] count: Option<usize>,
    // CSS grid class for `SkeletonGrid`, e.g. `"content-grid--metrics"` or `"content-grid--agent-panes"`
    #[prop(optional)] grid_class: Option<&'static str>,
    // Height for each skeleton item when `SkeletonGrid`
    #[prop(optional)] item_height: Option<&'static str>,
) -> impl IntoView {
    match variant {
        LoadingVariant::SkeletonCards => view! {
            <div class="mt-xl">
                <div class="skeleton skeleton--card mb-lg" style="height: 180px;"></div>
                <div class="skeleton skeleton--card" style="height: 300px;"></div>
            </div>
        }
        .into_any(),
        LoadingVariant::SkeletonGrid => {
            let count = count.unwrap_or(4).max(1);
            let grid = grid_class.unwrap_or("content-grid--metrics");
            let height = item_height.unwrap_or("80px");
            view! {
                <div class=format!("content-grid {}", grid)>
                    {(0..count)
                        .map(|_| {
                            view! {
                                <div class="skeleton skeleton--card" style=format!("height: {};", height)></div>
                            }
                        })
                        .collect::<Vec<_>>()}
                </div>
            }
            .into_any()
        }
        LoadingVariant::FormSkeleton => view! {
            <div class="form-loading">
                <div class="skeleton skeleton--text"></div>
            </div>
        }
        .into_any(),
        LoadingVariant::Text => {
            let text = label.unwrap_or("Loading...");
            view! {
                <div class="loading-state">
                    <span class="text-secondary">{text}</span>
                </div>
            }
            .into_any()
        }
    }
}
