use crate::components::log_viewer::LogViewer;
use crate::components::metrics_card::MetricsCard;
use crate::components::progress_bar::ProgressBar;
use crate::components::replay_overlay::ReplayOverlay;
use crate::{api_url, ConvertStats, JudgeResult, LogsListResponse, PrDetailResponse, PrResult, RunDetail};
use leptos::*;
use leptos_router::*;

#[component]
pub fn RunDetailPage() -> impl IntoView {
    let params = use_params_map();
    let run_id = move || params.get().get("id").cloned().unwrap_or_default();

    let (run, set_run) = create_signal::<Option<RunDetail>>(None);
    let (loading, set_loading) = create_signal(true);
    let (error, set_error) = create_signal::<Option<String>>(None);

    // ─── Log viewer state ─────────────────────────────────────
    let (logs_list, set_logs_list) = create_signal::<Option<LogsListResponse>>(None);
    let (logs_loading, set_logs_loading) = create_signal(false);
    let (show_logs, set_show_logs) = create_signal(false);

    let fetch_logs = move || {
        if !show_logs.get() {
            set_show_logs.set(true);
        }
        if logs_list.get().is_some() || logs_loading.get() {
            return;
        }
        set_logs_loading.set(true);
        let run_id = run_id();
        let set_logs = set_logs_list.clone();
        let set_loading = set_logs_loading.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let url = api_url(&format!("/api/runs/{}/logs", run_id));
            match gloo_net::http::Request::get(&url).send().await {
                Ok(r) if r.ok() => match r.json::<LogsListResponse>().await {
                    Ok(logs) => {
                        set_logs.set(Some(logs));
                    }
                    Err(e) => {
                        log::error!("Failed to parse logs list: {}", e);
                    }
                },
                Ok(r) => {
                    log::error!("Logs list fetch returned status: {}", r.status());
                }
                Err(e) => {
                    log::error!("Logs list fetch error: {}", e);
                }
            }
            set_loading.set(false);
        });
    };

    let _fetch = create_local_resource(
        move || run_id(),
        move |id| {
            let set_run = set_run.clone();
            let set_loading = set_loading.clone();
            let set_error = set_error.clone();
            async move {
                set_loading.set(true);
                set_error.set(None);
                match get_run_detail(&id).await {
                    Ok(detail) => {
                        set_run.set(Some(detail));
                        set_loading.set(false);
                    }
                    Err(e) => {
                        set_error.set(Some(e));
                        set_loading.set(false);
                    }
                }
            }
        },
    );

    // ─── Judge state signals ─────────────────────────────────────
    let (convert_result, set_convert_result) = create_signal::<Option<ConvertStats>>(None);
    let (judge_result, set_judge_result) = create_signal::<Option<JudgeResult>>(None);
    let (judge_loading, set_judge_loading) = create_signal(false);
    let (convert_loading, set_convert_loading) = create_signal(false);
    let (judge_error, set_judge_error) = create_signal::<Option<String>>(None);

    // ─── Judge action handlers ───────────────────────────────────
    let run_convert = move |id: String| {
        set_convert_loading.set(true);
        set_judge_error.set(None);
        let c_result = set_convert_result.clone();
        let c_loading = set_convert_loading.clone();
        let c_error = set_judge_error.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let url = api_url(&format!("/api/runs/{}/convert", id));
            match gloo_net::http::Request::post(&url).send().await {
                Ok(r) => {
                    if r.ok() {
                        match r.json::<ConvertStats>().await {
                            Ok(stats) => c_result.set(Some(stats)),
                            Err(e) => c_error.set(Some(format!("Parse error: {}", e))),
                        }
                    } else {
                        let txt = r.text().await.unwrap_or_default();
                        c_error.set(Some(format!("Convert failed ({}): {}", r.status(), txt)));
                    }
                }
                Err(e) => c_error.set(Some(format!("Network error: {}", e))),
            }
            c_loading.set(false);
        });
    };

    let run_judge = move |id: String| {
        set_judge_loading.set(true);
        set_judge_error.set(None);
        let j_result = set_judge_result.clone();
        let j_loading = set_judge_loading.clone();
        let j_error = set_judge_error.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let url = api_url(&format!("/api/runs/{}/judge", id));
            match gloo_net::http::Request::post(&url).send().await {
                Ok(r) => {
                    match r.json::<JudgeResult>().await {
                        Ok(result) => j_result.set(Some(result)),
                        Err(e) => j_error.set(Some(format!("Parse error: {}", e))),
                    }
                }
                Err(e) => j_error.set(Some(format!("Network error: {}", e))),
            }
            j_loading.set(false);
        });
    };

    view! {
        <div class="run-detail-page">
            // ─── Back Link ───────────────────────────────────────────
            <a href="/" class="back-link" style="display: inline-flex; align-items: center; gap: 4px; font-size: 14px; color: var(--text-secondary, #8b949e); margin-bottom: 16px;">
                "← Dashboard"
            </a>

            {move || {
                if loading.get() {
                    view! {
                        <div class="content-grid content-grid--metrics">
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                            <div class="skeleton skeleton--metric"></div>
                        </div>
                        <div style="margin-top: 24px;">
                            <div class="skeleton skeleton--heading"></div>
                            <div class="skeleton skeleton--table-row"></div>
                            <div class="skeleton skeleton--table-row"></div>
                            <div class="skeleton skeleton--table-row"></div>
                        </div>
                    }.into_view()
                } else if let Some(e) = error.get() {
                    view! {
                        <div class="error-state" role="alert">
                            <div class="error-state__icon">"⚠️"</div>
                            <h3 class="error-state__heading">"Failed to load run details"</h3>
                            <p class="error-state__message">{format!("Error: {e}")}</p>
                            <div class="error-state__action">
                                <button class="btn btn--primary" on:click=move |_| set_loading.set(true)>"🔄 Retry"</button>
                            </div>
                        </div>
                    }.into_view()
                } else if let Some(detail) = run.get() {
                    let detail_clone = detail.clone();
                    let _detail_clone2 = detail.clone();
                    let detail_id = detail.id.clone();
                    let detail_id_logs = detail.id.clone();
                    let results_clone = detail.results.clone();
                    let results_clone2 = detail.results.clone();

                    let badge_variant = match detail.status.as_str() {
                        "completed" | "done" => "badge--success",
                        "failed" => "badge--danger",
                        "running" => "badge--warning",
                        _ => "badge--neutral",
                    };

                    let is_running = detail.status == "running" || detail.status == "pending";
                    let _has_results = !detail.results.is_empty();

                    let live_url = format!("/runs/{}/live", detail.id);

                    view! {
                        // ─── Page Header ──────────────────────────────
                        <div class="page-header">
                            <div>
                                <h1 class="page-header__title">{&detail.name}</h1>
                                <div style="display: flex; align-items: center; gap: 8px; margin-top: 4px;">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{&detail.status}</span>
                                    </span>
                                    <span style="font-size: var(--text-sm, 14px); color: var(--text-secondary, #8b949e);">
                                        {format!("Model: ")}<span class="code">{&detail.model}</span>
                                    </span>
                                </div>
                            </div>
                            <div class="page-header__actions">
                                {move || {
                                    if is_running {
                                        view! {
                                            <a href=&live_url class="btn btn--success">
                                                <span class="btn__icon">"🔴"</span>
                                                <span class="btn__label">"Live View"</span>
                                            </a>
                                        }.into_view()
                                    } else {
                                        view! { <span></span> }.into_view()
                                    }
                                }}
                                <button
                                    class="btn btn--primary"
                                    on:click=move |_| fetch_logs()
                                >
                                    {move || if show_logs.get() { "📋 Logs" } else { "📋 View Logs" }}
                                </button>
                            </div>
                        </div>

                        // ─── Progress ──────────────────────────────────
                        {move || {
                            let total = results_clone.len() as u32;
                            let done = results_clone.iter().filter(|r| r.status.as_deref() == Some("done")).count() as u32;
                            if total > 0 && is_running {
                                let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u32 } else { 0 };
                                view! {
                                    <div class="card" style="margin-bottom: var(--spacing-lg, 16px);">
                                        <div class="card__body">
                                            <h3 class="card__title">"Progress"</h3>
                                            <ProgressBar value=done max=total label=format!("{} / {} PRs ({}%)", done, total, pct) />
                                        </div>
                                    </div>
                                }.into_view()
                            } else {
                                view! { <span></span> }.into_view()
                            }
                        }}

                        // ─── Metrics ──────────────────────────────────
                        <div class="content-grid content-grid--metrics">
                            {move || {
                                if let Some(ref agg) = detail_clone.aggregate {
                                    view! {
                                        <div class="metric-card">
                                            <p class="metric-card__label">"F1 Score"</p>
                                            <p class="metric-card__value" style="color: var(--accent-blue, #58a6ff);">{format!("{:.3}", agg.avg_f1)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Precision"</p>
                                            <p class="metric-card__value" style="color: var(--accent-green, #3fb950);">{format!("{:.3}", agg.avg_precision)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Recall"</p>
                                            <p class="metric-card__value" style="color: var(--accent-orange, #f0883e);">{format!("{:.3}", agg.avg_recall)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Total Cost"</p>
                                            <p class="metric-card__value">{format!("${:.4}", agg.total_cost)}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Duration"</p>
                                            <p class="metric-card__value">{format!("{:.0}s", agg.duration_secs)}</p>
                                        </div>
                                    }.into_view()
                                } else {
                                    let cost_str = detail_clone.total_cost.map(|c| format!("${:.4}", c)).unwrap_or_else(|| "—".into());
                                    let dur_str = detail_clone.duration_secs.map(|d| format!("{:.0}s", d)).unwrap_or_else(|| "—".into());
                                    view! {
                                        <div class="metric-card">
                                            <p class="metric-card__label">"F1 Score"</p>
                                            <p class="metric-card__value">"—"</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Precision"</p>
                                            <p class="metric-card__value">"—"</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Recall"</p>
                                            <p class="metric-card__value">"—"</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Total Cost"</p>
                                            <p class="metric-card__value">{cost_str}</p>
                                        </div>
                                        <div class="metric-card">
                                            <p class="metric-card__label">"Duration"</p>
                                            <p class="metric-card__value">{dur_str}</p>
                                        </div>
                                    }.into_view()
                                }
                            }}
                        </div>

                        // ─── Per-PR Results ────────────────────────────
                        <div class="section-header">
                            <h2 class="section-header__title">"Per-PR Results"</h2>
                        </div>

                        <div class="table-wrapper">
                            <table class="table">
                                <thead>
                                    <tr>
                                        <th class="table__th table__th--sortable">"# " <span class="table__sort-icon">"↕"</span></th>
                                        <th class="table__th table__th--sortable">"Title " <span class="table__sort-icon">"↕"</span></th>
                                        <th class="table__th table__th--sortable">"F1 " <span class="table__sort-icon">"↕"</span></th>
                                        <th class="table__th table__th--sortable">"Prec " <span class="table__sort-icon">"↕"</span></th>
                                        <th class="table__th table__th--sortable">"Rec " <span class="table__sort-icon">"↕"</span></th>
                                        <th class="table__th table__th--sortable">"Cost " <span class="table__sort-icon">"↕"</span></th>
                                        <th class="table__th">"Status"</th>
                                        <th class="table__th">"Details"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || {
                                        let results_clone2 = results_clone2.clone();
                                        results_clone2.iter().map(|pr: &PrResult| {
                                        let pr_number = pr.pr_number;
                                        let pr_title = pr.title.clone();
                                        let pr_title_for_fetch = pr_title.clone();
                                        let f1 = pr.f1;
                                        let precision = pr.precision;
                                        let recall = pr.recall;
                                        let cost = pr.cost;
                                        let status = pr.status.clone();
                                        let run_id = detail_id.clone();

                                        // Per-PR detail fetch state
                                        let (pr_detail, set_pr_detail) = create_signal::<Option<PrDetailResponse>>(None);
                                        let (detail_loading, set_detail_loading) = create_signal(false);
                                        let (detail_open, set_detail_open) = create_signal(false);

                                        let toggle_detail = move |_| {
                                            let currently_open = detail_open.get();
                                            set_detail_open.set(!currently_open);
                                            if !currently_open && pr_detail.get().is_none() && !detail_loading.get() {
                                                set_detail_loading.set(true);
                                                let run_id = run_id.clone();
                                                let fetch_title = pr_title_for_fetch.clone();
                                                let set_detail = set_pr_detail.clone();
                                                let set_loading = set_detail_loading.clone();
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    // URL-encode the title to handle spaces, special chars
                                                    let encoded_title = js_sys::encode_uri_component(&fetch_title)
                                                        .as_string()
                                                        .unwrap_or_else(|| fetch_title.clone());
                                                    let url = api_url(&format!("/api/runs/{}/pr-detail/{}", run_id, encoded_title));
                                                    match gloo_net::http::Request::get(&url).send().await {
                                                        Ok(r) if r.ok() => {
                                                            let text = r.text().await.unwrap_or_default();
                                                            match serde_json::from_str::<PrDetailResponse>(&text) {
                                                                Ok(detail) => {
                                                                    set_detail.set(Some(detail));
                                                                }
                                                                Err(e) => {
                                                                    let preview = if text.len() > 500 { format!("{}... (truncated)", &text[..500]) } else { text.clone() };
                                                                    log::error!("PR detail parse error: {:?}. Body preview: {}", e, preview);
                                                                }
                                                            }
                                                        }
                                                        Ok(r) => {
                                                            log::error!("PR detail fetch status: {} URL: {}", r.status(), url);
                                                        }
                                                        Err(e) => {
                                                            log::error!("PR detail fetch/parse error: {:?}. URL: {}", e, url);
                                                        }
                                                    }
                                                    set_loading.set(false);
                                                });
                                            }
                                        };

                                        let pr_badge = match status.as_deref() {
                                            Some("done") => "badge--success",
                                            Some("failed") => "badge--danger",
                                            Some("reviewing") => "badge--warning",
                                            _ => "badge--neutral",
                                        };
                                        let status_text = status.unwrap_or_else(|| "pending".into());
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__td" style="font-weight: var(--weight-semibold, 600);">{format!("#{}", pr_number)}</td>
                                                <td class="table__td">{&pr_title}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{precision.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{recall.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td class="table__td">
                                                    <span class=format!("badge {}", pr_badge)>
                                                        <span class="badge__dot"></span>
                                                        <span class="badge__label">{status_text}</span>
                                                    </span>
                                                </td>
                                                <td class="table__td">
                                                    <button
                                                        style="padding: 0.25rem 0.5rem; border: 1px solid #475569; border-radius: 4px; cursor: pointer; background: transparent; color: #94a3b8; font-size: 0.8rem;"
                                                        on:click=toggle_detail
                                                    >
                                                        {move || if detail_open.get() { "▲ Hide" } else { "▼ Details" }}
                                                    </button>
                                                </td>
                                            </tr>
                                            {move || {
                                                if detail_open.get() {
                                                    if let Some(ref pd) = pr_detail.get() {
                                                        let has_verdicts = !pd.verdicts.is_empty();
                                                        let match_count = pd.verdicts.iter().filter(|v| v.match_).count();
                                                        let total_verdicts = pd.verdicts.len();
                                                        let cost_str = pd.cost.as_ref().map(|c| format!("${:.4}", c.total_usd)).unwrap_or_else(|| "N/A".into());
                                                        let agent_tokens = pd.cost.as_ref().map(|c| format!("{} in / {} out", c.agent_tokens_in, c.agent_tokens_out)).unwrap_or_else(|| "N/A".into());
                                                        let judge_tokens = pd.cost.as_ref().map(|c| format!("{} in / {} out", c.judge_tokens_in, c.judge_tokens_out)).unwrap_or_else(|| "N/A".into());
                                                        view! {
                                                            <tr class="table__row">
                                                                <td colspan="8" style="padding: 0;">
                                                                    <div style="background: #0f172a; padding: 1rem; margin: 0.25rem 0; border-radius: 6px;">
                                                                        // Summary stats
                                                                        <div style="display: flex; gap: 1.5rem; flex-wrap: wrap; margin-bottom: 1rem; font-size: 0.85rem;">
                                                                            <span style="color: #64748b;">Findings: <strong style="color: #e2e8f0;">{pd.findings_count}</strong> / {pd.golden_count} golden</span>
                                                                            <span style="color: #64748b;">Cost: <strong style="color: #e2e8f0;">{cost_str}</strong></span>
                                                                            <span style="color: #64748b;">Verdicts: <strong style="color: #e2e8f0;">{match_count}/{total_verdicts}</strong> matched</span>
                                                                            <span style="color: #64748b;">Agent tokens: <strong style="color: #e2e8f0;">{agent_tokens}</strong></span>
                                                                            <span style="color: #64748b;">Judge tokens: <strong style="color: #e2e8f0;">{judge_tokens}</strong></span>
                                                                        </div>
                                                                        // Verdicts
                                                                        {if has_verdicts {
                                                                            view! {
                                                                                <h4 style="color: #e2e8f0; margin: 0 0 0.5rem 0; font-size: 0.9rem;">"Judge Verdicts"</h4>
                                                                                {pd.verdicts.iter().map(|v| {
                                                                                    let badge_cls = if v.match_ { "✅ Matched" } else { "❌ Not Matched" };
                                                                                    let confidence_pct = format!("{:.0}%", v.confidence * 100.0);
                                                                                    let border_color = if v.match_ { "#22c55e" } else { "#ef4444" };
                                                                                    let style_str = format!("background: #1e2938; padding: 0.75rem; margin-bottom: 0.5rem; border-radius: 4px; border-left: 3px solid {};", border_color);
                                                                                    view! {
                                                                                        <div style={style_str}>
                                                                                            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.25rem;">
                                                                                                <span style="font-weight: 600; font-size: 0.85rem;">{badge_cls}</span>
                                                                                                <span style="color: #64748b; font-size: 0.8rem;">{confidence_pct}</span>
                                                                                            </div>
                                                                                            <pre style="margin: 0; font-size: 0.8rem; color: #cbd5e1; white-space: pre-wrap; word-break: break-word; line-height: 1.4;">{&v.reasoning}</pre>
                                                                                        </div>
                                                                                    }
                                                                                }).collect::<Vec<_>>()}
                                                                            }.into_view()
                                                                        } else {
                                                                            view! { <p style="color: #64748b; font-style: italic; font-size: 0.85rem;">"No verdict data available."</p> }.into_view()
                                                                        }}
                                                                        // Cost details
                                                                        {pd.cost.as_ref().map(|c| {
                                                                            view! {
                                                                                <details style="margin-top: 0.75rem;">
                                                                                    <summary style="cursor: pointer; font-size: 0.85rem; color: #64748b;">"Full Cost Breakdown"</summary>
                                                                                    <div style="margin-top: 0.5rem; padding: 0.75rem; background: #1e2938; border-radius: 4px; font-size: 0.8rem; color: #94a3b8;">
                                                                                        <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem;">
                                                                                            <span>Agent calls: {c.agent_call_count}</span>
                                                                                            <span>Judge calls: {c.judge_call_count}</span>
                                                                                            <span>Agent tokens in: {c.agent_tokens_in}</span>
                                                                                            <span>Agent tokens out: {c.agent_tokens_out}</span>
                                                                                            <span>Judge tokens in: {c.judge_tokens_in}</span>
                                                                                            <span>Judge tokens out: {c.judge_tokens_out}</span>
                                                                                        </div>
                                                                                    </div>
                                                                                </details>
                                                                            }.into_view()
                                                                        })}
                                                                        // Raw agent responses
                                                                        {if !pd.agent_responses.is_empty() {
                                                                            view! {
                                                                                <details style="margin-top: 0.75rem;">
                                                                                    <summary style="cursor: pointer; font-size: 0.85rem; color: #64748b;">"Raw Agent Responses (" {pd.agent_responses.len()} ")"</summary>
                                                                                    <div style="margin-top: 0.5rem;">
                                                                                        {pd.agent_responses.iter().enumerate().map(|(i, resp)| {
                                                                                            view! {
                                                                                                <div style="margin-bottom: 0.5rem;">
                                                                                                    <div style="font-size: 0.75rem; color: #64748b; margin-bottom: 0.25rem;">"Agent #" {i + 1}</div>
                                                                                                    <pre style="background: #0f172a; padding: 0.75rem; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 400px; overflow-y: auto; white-space: pre-wrap; word-break: break-word; line-height: 1.4; color: #cbd5e1;">{resp}</pre>
                                                                                                </div>
                                                                                            }
                                                                                        }).collect::<Vec<_>>()}
                                                                                    </div>
                                                                                </details>
                                                                            }.into_view()
                                                                        } else {
                                                                            view! {}.into_view()
                                                                        }}
                                                                        // Raw findings JSON
                                                                        {if !pd.findings.is_null() {
                                                                            let findings_str = pd.findings.to_string();
                                                                            if !findings_str.is_empty() && findings_str != "null" {
                                                                                view! {
                                                                                    <details style="margin-top: 0.75rem;">
                                                                                        <summary style="cursor: pointer; font-size: 0.85rem; color: #64748b;">"Raw Findings"</summary>
                                                                                        <pre style="background: #0f172a; padding: 0.75rem; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 400px; overflow-y: auto; white-space: pre-wrap; word-break: break-word; line-height: 1.4; color: #cbd5e1;">{findings_str}</pre>
                                                                                    </details>
                                                                                }.into_view()
                                                                            } else {
                                                                                view! {}.into_view()
                                                                            }
                                                                        } else {
                                                                            view! {}.into_view()
                                                                        }}                                                                    </div>
                                                                </td>
                                                            </tr>
                                                        }.into_view()
                                                    } else if detail_loading.get() {
                                                        view! {
                                                            <tr class="table__row">
                                                                <td colspan="8" style="padding: 0.75rem; text-align: center; color: #64748b; font-style: italic; font-size: 0.85rem;">
                                                                    "Loading PR details..."
                                                                </td>
                                                            </tr>
                                                        }.into_view()
                                                    } else {
                                                        view! {
                                                            <tr class="table__row" style="display: none;"></tr>
                                                        }.into_view()
                                                    }
                                                } else {
                                                    view! { <tr class="table__row" style="display: none;"></tr> }.into_view()
                                                }
                                            }}
                                        }
                                    }).collect::<Vec<_>>()}
                                    }
                                </tbody>
                            </table>
                        </div>

                        // ─── Judge Section ────────────────────────────
                        <div class="section-header" style="margin-top: 24px;">
                            <h2 class="section-header__title">"Benchmark Judge"</h2>
                        </div>
                        <div style="display: flex; gap: 8px; margin-bottom: 12px;">
                            <button
                                class="btn btn--primary"
                                disabled=move || convert_loading.get()
                                on:click={
                                    let id = detail.id.clone();
                                    let run_convert = run_convert.clone();
                                    move |_| run_convert(id.clone())
                                }
                            >
                                {move || if convert_loading.get() {
                                    "⏳ Converting..."
                                } else {
                                    "📐 Convert to candidates.json"
                                }}
                            </button>
                            <button
                                class="btn btn--success"
                                disabled=move || judge_loading.get()
                                on:click={
                                    let id = detail.id.clone();
                                    let run_judge = run_judge.clone();
                                    move |_| run_judge(id.clone())
                                }
                            >
                                {move || if judge_loading.get() {
                                    "⏳ Judging..."
                                } else {
                                    "🧪 Run Python Judge"
                                }}
                            </button>
                        </div>

                        // ─── Judge Results ───────────────────────────
                        {move || {
                            if let Some(err) = judge_error.get() {
                                view! {
                                    <div class="card card--error" style="margin-bottom: 12px; border: 1px solid var(--color-danger, #f87171); background: rgba(248, 113, 113, 0.1); padding: 12px; border-radius: 6px;">
                                        <p style="color: var(--color-danger, #f87171); font-weight: 600; margin: 0 0 4px 0;">"Error"</p>
                                        <p style="color: var(--text-secondary, #94a3b8); margin: 0; font-size: 0.85rem;">{err}</p>
                                    </div>
                                }.into_view()
                            } else if let Some(convert) = convert_result.get() {
                                view! {
                                    <div class="card" style="margin-bottom: 12px; padding: 12px; border-radius: 6px; background: var(--bg-card, #1e293b); border: 1px solid var(--border, #334155);">
                                        <p style="font-weight: 600; margin: 0 0 8px 0; color: var(--accent-green, #4ade80);">
                                            "✅ Conversion Complete"
                                        </p>
                                        <table style="font-size: 0.85rem; width: 100%;">
                                            <tbody>
                                                <tr><td style="padding: 4px 8px; color: var(--text-secondary, #94a3b8);">"PRs Converted"</td><td style="padding: 4px 8px;">{format!("{}", convert.pr_count)}</td></tr>
                                                <tr><td style="padding: 4px 8px; color: var(--text-secondary, #94a3b8);">"Findings"</td><td style="padding: 4px 8px;">{format!("{}", convert.finding_count)}</td></tr>
                                                <tr><td style="padding: 4px 8px; color: var(--text-secondary, #94a3b8);">"Output"</td><td style="padding: 4px 8px; font-family: monospace; font-size: 0.8rem;">{&convert.candidates_path}</td></tr>
                                            </tbody>
                                        </table>
                                    </div>
                                }.into_view()
                            } else {
                                view! { <span></span> }.into_view()
                            }
                        }}

                        {move || {
                            if let Some(result) = judge_result.get() {
                                let status_class = if result.success { "badge--success" } else { "badge--danger" };
                                view! {
                                    <div class="card" style="margin-bottom: 12px; padding: 12px; border-radius: 6px; background: var(--bg-card, #1e293b); border: 1px solid var(--border, #334155);">
                                        <div style="display: flex; align-items: center; gap: 8px; margin-bottom: 8px;">
                                            <span class=format!("badge {}", status_class)>
                                                {if result.success { "✅ Judge Passed" } else { "❌ Judge Failed" }}
                                            </span>
                                        </div>
                                        <p style="margin: 0 0 8px 0; color: var(--text-secondary, #94a3b8); font-size: 0.85rem;">
                                            {&result.message}
                                        </p>
                                        {if !result.stdout.is_empty() {
                                            view! {
                                                <details style="margin-bottom: 4px;">
                                                    <summary style="cursor: pointer; font-size: 0.85rem; color: var(--text-secondary, #94a3b8);">"stdout"</summary>
                                                    <pre style="background: #0f172a; padding: 8px; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 300px; overflow-y: auto; margin-top: 4px;">{&result.stdout}</pre>
                                                </details>
                                            }.into_view()
                                        } else {
                                            view! { <span></span> }.into_view()
                                        }}
                                        {if !result.stderr.is_empty() {
                                            view! {
                                                <details>
                                                    <summary style="cursor: pointer; font-size: 0.85rem; color: var(--text-secondary, #94a3b8);">"stderr"</summary>
                                                    <pre style="background: #0f172a; padding: 8px; border-radius: 4px; font-size: 0.75rem; overflow-x: auto; max-height: 300px; overflow-y: auto; margin-top: 4px; color: #f87171;">{&result.stderr}</pre>
                                                </details>
                                            }.into_view()
                                        } else {
                                            view! { <span></span> }.into_view()
                                        }}
                                    </div>
                                }.into_view()
                            } else {
                                view! { <span></span> }.into_view()
                            }
                        }}

                    // ─── Agent Logs Section ─────────────────────────
                    {move || {
                        if show_logs.get() {
                            let detail_id = detail_id_logs.clone();
                            view! {
                                <div style="margin-top: 1.5rem; border-top: 1px solid #334155; padding-top: 1rem;">
                                    <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.75rem;">
                                        <h3 style="color: #e2e8f0; margin: 0;">Agent Logs</h3>
                                        <button
                                            style="padding: 0.25rem 0.75rem; border: 1px solid #475569; border-radius: 4px; cursor: pointer; background: transparent; color: #94a3b8; font-size: 0.8rem;"
                                            on:click=move |_| set_show_logs.set(false)
                                        >
                                            "✕ Hide Logs"
                                        </button>
                                    </div>
                                    {move || {
                                        if logs_loading.get() {
                                            view! {
                                                <p style="color: #94a3b8; font-style: italic;">
                                                    "Loading logs..."
                                                </p>
                                            }.into_view()
                                        } else if let Some(ref logs) = logs_list.get() {
                                            let run_id = detail_id.clone();
                                            view! {
                                                <LogViewer logs=logs.clone() run_id=run_id />
                                            }.into_view()
                                        } else {
                                            view! {
                                                <p style="color: #64748b; font-style: italic;">
                                                    "No agent logs available for this run."
                                                </p>
                                            }.into_view()
                                        }
                                    }}
                                </div>
                            }.into_view()
                        } else {
                            view! { <span></span> }.into_view()
                        }
                    }}

                    }.into_view()
                } else {
                    view! { <p>"No data."</p> }.into_view()
                }
            }}
        </div>
    }
}

#[component]
fn RunDetailView(detail: RunDetail) -> impl IntoView {
    let id_clone = detail.id.clone();
    let live_url = format!("/runs/{}/live", detail.id);
    let detail2 = detail.clone();
    let detail3 = detail.clone();
    let detail4 = detail.clone();
    let detail4_id = detail4.id.clone();
    let detail4_id_replay = detail4_id.clone();

    // Tab state
    let (active_tab, set_active_tab) = create_signal("results".to_string());
    // Replay overlay visibility
    let (show_replay, set_show_replay) = create_signal(false);

    let status_badge_class = match detail.status.as_str() {
        "completed" | "done" => "badge badge-done",
        "failed" => "badge badge-failed",
        "running" | "pending" => "badge badge-running",
        _ => "badge badge-pending",
    };

    // Fetch logs list (for Logs tab)
    let (logs_list, set_logs_list) = create_signal::<Option<LogsListResponse>>(None);
    let (logs_loading, set_logs_loading) = create_signal(false);
    let (logs_fetched, set_logs_fetched) = create_signal(false);

    // Fetch logs only when Logs tab is activated
    let fetch_logs = move || {
        if !logs_fetched.get() && !logs_loading.get() {
            set_logs_loading.set(true);
            let run_id = id_clone.clone();
            let set_logs = set_logs_list.clone();
            let set_loading = set_logs_loading.clone();
            let set_fetched = set_logs_fetched.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let url = api_url(&format!("/api/runs/{}/logs", run_id));
                match gloo_net::http::Request::get(&url).send().await {
                    Ok(r) if r.ok() => match r.json::<LogsListResponse>().await {
                        Ok(logs) => {
                            set_logs.set(Some(logs));
                        }
                        Err(e) => {
                            log::error!("Failed to parse logs list: {}", e);
                        }
                    },
                    Ok(r) => {
                        log::error!("Logs list fetch returned status: {}", r.status());
                    }
                    Err(e) => {
                        log::error!("Logs list fetch error: {}", e);
                    }
                }
                set_loading.set(false);
                set_fetched.set(true);
            });
        }
    };

    let tab_style = |tab_name: &str| -> String {
        let is_active = active_tab.with(|t| t == tab_name);
        let base = "padding: 0.5rem 1.25rem; border: none; cursor: pointer; font-weight: 600; font-size: 0.9rem; border-radius: 6px 6px 0 0;";
        if is_active {
            format!("{} background: #334155; color: #e2e8f0;", base)
        } else {
            format!("{} background: transparent; color: #64748b;", base)
        }
    };

    view! {
        // ─── Page header ──────────────────────────────────────────────
        <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 1rem;">
            <div>
                <h1 style="margin: 0;">{&detail.name}</h1>
                <p style="color: #64748b; margin: 0.25rem 0 0 0;">
                    {format!("Model: ")}<span class="code">{&detail.model}</span>
                    {format!(" — ")}
                    <span class=status_badge_class>{&detail.status}</span>
                </p>
            </div>
            <div style="display: flex; gap: 0.5rem;">
                {move || {
                    if detail.status == "running" || detail.status == "pending" {
                        view! {
                            <a href=&live_url class="btn btn-green" target="_blank">
                                "Live View"
                            </a>
                        }.into_view()
                    } else {
                        view! { <span></span> }.into_view()
                    }
                }}
                <button
                    style="padding: 0.5rem 1.25rem; border: none; border-radius: 6px; cursor: pointer; background: #3b82f6; color: white; font-weight: 600; font-size: 0.9rem;"
                    on:click=move |_| set_show_replay.set(true)
                >
                    "▶ Replay Run"
                </button>
            </div>
        </div>

        // ─── Status progress ─────────────────────────────────────────
        {move || {
            let total = detail.results.len() as u32;
            let done = detail.results.iter().filter(|r| r.status.as_deref() == Some("done")).count() as u32;
            if total > 0 {
                let pct = if total > 0 { (done as f64 / total as f64 * 100.0) as u32 } else { 0 };
                view! {
                    <div class="card">
                        <h3>"Progress"</h3>
                        <ProgressBar value=done max=total label=format!("{} / {} PRs ({})", done, total, pct) />
                    </div>
                }.into_view()
            } else {
                view! { <span></span> }.into_view()
            }
        }}

        // ─── Metrics ─────────────────────────────────────────────────
        <div style="display: flex; gap: 1rem; flex-wrap: wrap; margin-bottom: 1rem;">
            {move || {
                if let Some(ref agg) = detail2.aggregate {
                    view! {
                        <MetricsCard value={format!("{:.3}", agg.avg_f1)} label="Avg F1" />
                        <MetricsCard value={format!("{:.3}", agg.avg_precision)} label="Avg Precision" />
                        <MetricsCard value={format!("{:.3}", agg.avg_recall)} label="Avg Recall" />
                        <MetricsCard value={format!("${:.4}", agg.total_cost)} label="Total Cost" />
                        <MetricsCard value={format!("{:.1}s", agg.duration_secs)} label="Duration" />
                        <MetricsCard value={format!("{}", agg.total_prs)} label="Total PRs" />
                    }.into_view()
                } else {
                    let cost_str = detail2.total_cost.map(|c| format!("${:.4}", c)).unwrap_or_else(|| "—".into());
                    let dur_str = detail2.duration_secs.map(|d| format!("{:.1}s", d)).unwrap_or_else(|| "—".into());
                    view! {
                        <MetricsCard value="—" label="Avg F1" />
                        <MetricsCard value="—" label="Avg Precision" />
                        <MetricsCard value="—" label="Avg Recall" />
                        <MetricsCard value=cost_str label="Total Cost" />
                        <MetricsCard value=dur_str label="Duration" />
                        <MetricsCard value={format!("{}", detail2.results.len())} label="Total PRs" />
                    }.into_view()
                }
            }}
        </div>

        // ─── Tab bar ──────────────────────────────────────────────────
        <div style="display: flex; gap: 0; border-bottom: 2px solid #334155; margin-bottom: 0;">
            <button
                style=tab_style("results")
                on:click=move |_| set_active_tab.set("results".to_string())
            >
                "Results"
            </button>
            <button
                style=tab_style("logs")
                on:click=move |_| {
                    set_active_tab.set("logs".to_string());
                    fetch_logs();
                }
            >
                "Logs"
            </button>
        </div>

        // ─── Tab content ──────────────────────────────────────────────
        <div style="background: #1e2938; border-radius: 0 0 8px 8px; padding: 1rem; border: 1px solid #334155; border-top: none;">
            {move || {
                let tab = active_tab.get();
                if tab == "results" {
                    // ─── Results tab ──────────────────────────────────
                    view! {
                        <div class="card" style="background: transparent; border: none; padding: 0;">
                            <h3>"PR Results"</h3>
                            <table>
                                <thead>
                                    <tr>
                                        <th>"#"</th>
                                        <th>"Title"</th>
                                        <th>"F1"</th>
                                        <th>"Precision"</th>
                                        <th>"Recall"</th>
                                        <th>"Cost"</th>
                                        <th>"Status"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {detail3.results.iter().map(|pr| {
                                        let status_class = match pr.status.as_deref() {
                                            Some("done") => "badge badge-done",
                                            Some("failed") => "badge badge-failed",
                                            Some("reviewing") => "badge badge-running",
                                            _ => "badge badge-pending",
                                        };
                                        let status_text = pr.status.clone().unwrap_or_else(|| "pending".into());
                                        view! {
                                            <tr>
                                                <td style="font-weight: 600;">{format!("#{}", pr.pr_number)}</td>
                                                <td>{&pr.title}</td>
                                                <td style="font-family: monospace;">{pr.f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td style="font-family: monospace;">{pr.precision.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td style="font-family: monospace;">{pr.recall.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td style="font-family: monospace;">{pr.cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "—".into())}</td>
                                                <td><span class=status_class>{status_text}</span></td>
                                            </tr>
                                        }
                                    }).collect::<Vec<_>>()}
                                </tbody>
                            </table>
                        </div>
                    }.into_view()
                } else {
                    // ─── Logs tab ────────────────────────────────────
                    let detail4_id = detail4_id.clone();
                    view! {
                        <div>
                            <h3 style="color: #e2e8f0; margin: 0 0 1rem 0;">"Agent Logs"</h3>
                            {move || {
                                if logs_loading.get() {
                                    view! {
                                        <p style="color: #94a3b8; font-style: italic;">
                                            "Loading logs..."
                                        </p>
                                    }.into_view()
                                } else if let Some(ref logs) = logs_list.get() {
                                    let run_id = detail4_id.clone();
                                    view! {
                                        <LogViewer logs=logs.clone() run_id=run_id />
                                    }.into_view()
                                } else {
                                    view! {
                                        <p style="color: #64748b; font-style: italic;">
                                            "Click the Logs tab to load agent logs."
                                        </p>
                                    }.into_view()
                                }
                            }}
                        </div>
                    }.into_view()
                }
            }}
        </div>

        // ─── Replay Overlay ───────────────────────────────────────────
        {move || {
            let run_id = detail4_id_replay.clone();
            view! {
                <ReplayOverlay
                    visible=show_replay.get()
                    on_close=move || set_show_replay.set(false)
                    run_id=run_id
                />
            }
        }}
    }
}

async fn get_run_detail(id: &str) -> Result<RunDetail, String> {
    let url = api_url(&format!("/api/runs/{}", id));
    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    if !response.ok() {
        return Err(format!("Server returned {}", response.status()));
    }

    let data: RunDetail = response
        .json()
        .await
        .map_err(|e| format!("Parse error: {}", e))?;

    Ok(data)
}
