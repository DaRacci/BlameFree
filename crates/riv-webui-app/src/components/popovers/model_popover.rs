use leptos::prelude::*;
use riv_webui_shared::config::AvailableModel;

use crate::components::popovers::popover::Popover;

/// Hover/focus popover showing a model's capabilities.
///
/// Renders the trigger (`children`) and the model detail card inside a
/// `.popover-host` wrapper (via the reusable [`Popover`]). The popover opens
/// above the host by default; when the host sits near the top of the viewport
/// it flips below so it isn't clipped.
#[component]
pub fn ModelPopover(model: AvailableModel, children: Children) -> impl IntoView {
    let reasoning = model.reasoning;
    let id = model.id.clone();
    let name = model.name.clone();
    let description = model.description.clone();
    let modality = model.modality.clone();
    let context = model.context_length;
    let prompt_price = model.prompt_price;
    let completion_price = model.completion_price;
    let supports_tools = model.supports_tools;
    let supports_structured = model.supports_structured_outputs;
    let supports_web_search = model.supports_web_search;

    let content = move || -> AnyView {
        view! {
            <div class="model-popover__head">
                <span class="model-popover__name">
                    {name.clone().unwrap_or_else(|| id.clone())}
                </span>
                {reasoning.then(|| {
                    view! { <span class="model-popover__tag">"reasoning"</span> }
                })}
            </div>
            <div class="model-popover__id">{id.clone()}</div>
            {description.clone().map(|d| {
                view! { <p class="model-popover__desc">{d}</p> }
            })}
            <dl class="model-popover__specs">
                {modality.clone().map(|m| {
                    view! { <div class="model-popover__spec"><dt>"Modality"</dt><dd>{m}</dd></div> }
                })}
                {context.map(|c| {
                    view! { <div class="model-popover__spec"><dt>"Context"</dt><dd>{format!("{}", c)}</dd></div> }
                })}
                {prompt_price.map(|p| {
                    view! { <div class="model-popover__spec"><dt>"Input"</dt><dd>{"$"} {format!("{:.6}", p)}</dd></div> }
                })}
                {completion_price.map(|p| {
                    view! { <div class="model-popover__spec"><dt>"Output"</dt><dd>{"$"} {format!("{:.6}", p)}</dd></div> }
                })}
            </dl>
            <div class="model-popover__caps">
                {if supports_tools {
                    view! { <span class="model-popover__cap">"tools"</span> }.into_any()
                } else {
                    view! {}.into_any()
                }}
                {if supports_structured {
                    view! { <span class="model-popover__cap">"structured outputs"</span> }.into_any()
                } else {
                    view! {}.into_any()
                }}
                {if supports_web_search {
                    view! { <span class="model-popover__cap">"web search"</span> }.into_any()
                } else {
                    view! {}.into_any()
                }}
            </div>
        }
        .into_any()
    };

    view! {
        <Popover content=content>
            {children()}
        </Popover>
    }
}
