use leptos::prelude::*;

/// Loading state variant selector.
#[derive(Debug, Clone, Copy)]
pub enum LoadingVariant {
    /// Two stacked skeleton cards
    SkeletonCards,

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
) -> impl IntoView {
    match variant {
        LoadingVariant::SkeletonCards => view! {
            <div class="mt-xl">
                <div class="skeleton skeleton--card mb-lg" style="height: 180px;"></div>
                <div class="skeleton skeleton--card" style="height: 300px;"></div>
            </div>
        }
        .into_any(),
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
