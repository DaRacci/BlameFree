#![recursion_limit = "256"]

use std::{pin::Pin, sync::Arc};

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use mti::prelude::MagicTypeId;
use riv_types::{
    benchmark::{golden::GoldenCommentEntry, result::PrResult},
    capabilities::ReasoningEffort,
    review::Review,
    vcs::pr::PrMeta,
    wrappers::Model,
};
#[cfg(target_arch = "wasm32")]
use riv_webui_shared::routes::API_CONFIG;
use riv_webui_shared::{
    admin::LogsResponse,
    auth::AuthUser,
    config::{AgentInfo, DatasetInfo},
    review::ReviewAgentLog,
};

pub mod async_resource;
pub mod components;
pub mod pages;

#[cfg(target_arch = "wasm32")]
pub mod sse;

pub use riv_webui_shared::config::AppConfig;

use crate::components::sidebar::Sidebar;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct LiveAgentInfo {
    pub id: MagicTypeId,
    pub name: String,
    pub abbreviation: String,
}

pub type AppServiceFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;
pub type AppServiceFn<A, T> = Arc<dyn Fn(A) -> AppServiceFuture<T> + Send + Sync>;
pub type AppReadFn<T> = AppServiceFn<(), T>;

#[derive(Clone)]
pub struct AppServices {
    pub list_reviews: AppReadFn<Vec<Review>>,
    pub read_admin_logs: AppReadFn<LogsResponse>,
    pub list_repo_prs: AppServiceFn<(String, String), Vec<PrMeta>>,
    pub fetch_pr_diff: AppServiceFn<(String, String, u32), (String, String)>,
    pub list_datasets: AppReadFn<Vec<DatasetInfo>>,
    pub list_dataset_prs: AppServiceFn<(String,), Vec<GoldenCommentEntry>>,
    pub list_models: AppReadFn<Vec<Model>>,
    pub list_reasoning_efforts: AppServiceFn<(String,), Vec<ReasoningEffort>>,
    pub list_agents: AppReadFn<Vec<AgentInfo>>,
    pub get_review: AppServiceFn<(MagicTypeId,), Review>,
    pub list_pr_results: AppServiceFn<(MagicTypeId,), Vec<PrResult>>,
    pub list_agent_logs: AppServiceFn<(MagicTypeId,), Vec<ReviewAgentLog>>,
    pub list_live_review_agents: AppServiceFn<(MagicTypeId,), Vec<LiveAgentInfo>>,
    pub start_review: AppServiceFn<(String, String, Vec<String>, Option<ReasoningEffort>), Review>,
    pub start_benchmark: AppServiceFn<
        (
            String,
            Vec<String>,
            String,
            Vec<String>,
            Option<ReasoningEffort>,
        ),
        Review,
    >,
}

#[cfg(feature = "ssr")]
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    let leptos_options = options.clone();
    provide_meta_context();
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=leptos_options.clone()/>
                <HydrationScripts options=leptos_options/>
                <MetaTags/>
                <Title text="BlameFree"/>
                <Stylesheet id="leptos" href="/pkg/blamefree.css"/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize)]
struct ConfigResponse {
    auth_enabled: bool,
}

#[derive(Clone)]
pub struct AuthContext {
    pub user: RwSignal<Option<AuthUser>>,
    pub auth_enabled: RwSignal<bool>,
}

#[component]
pub fn App() -> impl IntoView {
    let auth_ctx = AuthContext {
        user: RwSignal::new(None),
        auth_enabled: RwSignal::new(false),
    };
    provide_context(auth_ctx.clone());

    #[cfg(target_arch = "wasm32")]
    leptos::task::spawn_local(async move {
        use gloo_net::http::Request;

        let resp = Request::get(API_CONFIG).send().await;
        if let Ok(resp) = resp {
            if let Ok(config) = resp.json::<ConfigResponse>().await {
                let enabled = config.auth_enabled;
                auth_ctx.auth_enabled.set(enabled);
                if enabled {
                    let user_resp = Request::get("/auth/me").send().await;
                    if let Ok(user_resp) = user_resp {
                        if user_resp.ok() {
                            if let Ok(user) = user_resp.json::<AuthUser>().await {
                                auth_ctx.user.set(Some(user));
                            }
                        }
                    }
                }
            }
        }
    });

    use pages::{
        admin::AdminPage, four_zero_four::FourZeroFourPage, home::HomePage,
        new_benchmark::NewBenchmarkPage, new_review::NewReviewPage,
        review_detail::ReviewDetailPage, review_live::ReviewLivePage,
    };

    view! {
        <Html attr:lang="en" attr:dir="ltr" />
        <Router>
            <div class="app-shell">
                <Sidebar />
                <main class="main-content">
                    <div class="content-container">
                        <Routes fallback=|| view! { <FourZeroFourPage /> }>
                            <Route path=path!("/") view=HomePage />
                            <Route path=path!("/reviews/new") view=NewReviewPage />
                            <Route path=path!("/reviews/:id/live") view=ReviewLivePage />
                            <Route path=path!("/reviews/:id") view=ReviewDetailPage />
                            <Route path=path!("/benchmarks/new") view=NewBenchmarkPage />
                            <Route path=path!("/admin") view=AdminPage />
                        </Routes>
                    </div>
                </main>
            </div>
        </Router>
    }
}
