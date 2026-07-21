use crb_macros::define_routes;

define_routes! {
    API_RUNS,                 "/api/runs";
    API_RUNS_ID,              "/api/runs/:id";
    API_RUNS_ID_LIVE,         "/api/runs/:id/live";
    API_RUNS_ID_LOGS,         "/api/runs/:id/logs";
    API_RUNS_ID_PRS_KEY,      "/api/runs/:id/prs/:pr_key";
    API_RUNS_ID_DETAILS_KEY,  "/api/runs/:id/pr-detail/:pr_key";
    API_RUNS_ID_LOGS_KEY_ROLE,"/api/runs/:id/logs/:pr_key/:role";

    API_CONFIG,               "/api/config";

    API_BENCHMARK_DATASETS,   "/api/benchmark/datasets";

    API_DISCOVERY_MODELS,       "/api/discovery/models";
    API_DISCOVERY_CAPABILITIES, "/api/discovery/capabilities/:model";

    API_CONFIG_REASONING,     "/api/config/reasoning-efforts";
    API_DATASETS_ID_PRS,      "/api/datasets/:id/prs";
    API_ADHOC_REVIEW,         "/api/ahoc/review";
    API_ADHOC_RUNS,           "/api/adhoc/runs";
    API_ADHOC_RUNS_ID,        "/api/adhoc/runs/:id";
    API_ADHOC_PRS_OWNER_REPO, "/api/adhoc/prs/:owner/:repo";

    API_ADMIN_LOGS,           "/api/admin/logs";
    API_ADMIN_LOGS_STREAM,    "/api/admin/logs/stream";

    API_REVIEWS_LIST,     "/api/reviews";
    API_REVIEWS_DETAILS,  "/api/reviews/:id/details";
    API_REVIEWS_SUBMIT,   "/api/reviews/:id/submit";
    API_REVIEWS_AGENTS,   "/api/reviews/:id/agents";
    API_REVIEWS_LOGS,     "/api/reviews/:id/logs/:agent";
    API_REVIEWS_STREAM,   "/api/reviews/:id/stream";

    AUTH_LOGIN,     "/auth/login";
    AUTH_CALLBACK,  "/auth/callback";
    AUTH_LOGOUT,    "/auth/logout";
    AUTH_ME,        "/auth/me";

    HOME,           "/";
    ADMIN,          "/admin";

    REVIEWS_ID,         "/reviews/:id";
    REVIEWS_ID_LOGS,    "/reviews/:id/logs";
    REVIEWS_ID_DETAILS, "/reviews/:id/details";
}
