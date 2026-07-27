use leptos::prelude::*;

#[component]
pub fn FormPage(
    /// Page title displayed in the header
    title: &'static str,
    /// Text on the submit button when not submitting
    submit_label: &'static str,
    /// Whether config is still loading
    config_loading: ReadSignal<bool>,
    /// Config error message if loading failed
    config_error: ReadSignal<Option<String>>,
    /// Whether form is currently submitting
    submitting: ReadSignal<bool>,
    /// Submit error message if submission failed
    submit_error: ReadSignal<Option<String>>,
    /// Submit event handler (should call prevent_default internally)
    on_submit: impl Fn(leptos::ev::SubmitEvent) + Send + 'static,
    /// Returns true if submit button should be disabled (checked in addition to submitting state)
    submit_disabled: impl Fn() -> bool + Send + 'static,
    /// Form field children — section elements to render inside the form
    children: Children,
) -> impl IntoView {
    view! {
        <div class="form-page">
            <div class="page-header">
                <h1 class="page-header__title">{title}</h1>
                <div class="page-header__actions">
                    <a href="/" class="btn btn--ghost">"Cancel"</a>
                </div>
            </div>

            {
                let config_loading = config_loading;
                move || -> AnyView {
                    if config_loading.get() {
                        view! {
                            <div class="form-loading">
                                <div class="skeleton skeleton--text"></div>
                            </div>
                        }.into_view().into_any()
                    } else {
                        view! { <span></span> }.into_view().into_any()
                    }
                }
            }

            {
                let config_error = config_error;
                move || -> AnyView {
                    if let Some(e) = config_error.get() {
                        view! {
                            <div class="card form-error-banner">
                                <div class="card__body">
                                    <p class="text-error">{format!("Failed to load config: {}", e)}</p>
                                    <p class="text-secondary text-sm">"You can still fill in the form manually."</p>
                                </div>
                            </div>
                        }.into_view().into_any()
                    } else {
                        view! { <span></span> }.into_view().into_any()
                    }
                }
            }

            <form on:submit=on_submit>
                {children()}

                <div class="form-actions">
                    <button
                        type="submit"
                        class="btn btn--primary btn--lg btn--full"
                        disabled=move || submitting.get() || submit_disabled()
                    >
                        {move || {
                            if submitting.get() {
                                "Submitting..."
                            } else {
                                submit_label
                            }
                        }}
                    </button>
                </div>

                {
                    let submit_error = submit_error;
                    move || -> AnyView {
                        if let Some(e) = submit_error.get() {
                            view! {
                                <div class="error-state form-submit-error" role="alert">
                                    <p>{format!("Error: {}", e)}</p>
                                </div>
                            }.into_view().into_any()
                        } else {
                            view! { <span></span> }.into_view().into_any()
                        }
                    }
                }
            </form>
        </div>
    }
}
