use leptos::prelude::*;

#[component]
pub fn FourZeroFourPage() -> impl IntoView {
    view! {
      <div class="state-container">
        <h2>"404 - Page Not Found"</h2>
        <p>"The page you're looking for doesn't exist."</p>
        <div class="error-state__action">
            <a href="/" class="btn btn--primary">"Go Home"</a>
        </div>
      </div>
    }
}
