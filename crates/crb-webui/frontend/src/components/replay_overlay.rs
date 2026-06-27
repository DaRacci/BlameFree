use leptos::*;
use crate::{ReplayStatusResponse, RunDetail, api_url};
use crate::components::progress_bar::ProgressBar;
use std::rc::Rc;
use std::cell::Cell;

#[component]
pub fn ReplayOverlay(
    visible: bool,
    on_close: impl Fn() + 'static,
    run_id: String,
) -> impl IntoView {
    let (replay_state, set_replay_state) = create_signal::<ReplayState>(ReplayState::Idle);
    let (status_text, set_status_text) = create_signal("Ready to replay".to_string());
    let (progress_pct, set_progress_pct) = create_signal(0u32);
    let (completed_prs, set_completed_prs) = create_signal(0u32);
    let (total_prs, set_total_prs) = create_signal(0u32);
    let (original_detail, set_original_detail) = create_signal::<Option<RunDetail>>(None);
    let (replay_detail, set_replay_detail) = create_signal::<Option<RunDetail>>(None);
    let (error, set_error) = create_signal::<Option<String>>(None);

    let overlay_style = "position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.6); display: flex; align-items: center; justify-content: center; z-index: 1000;";
    let modal_style = "background: #1e2938; border-radius: 12px; padding: 1.5rem; max-width: 700px; width: 90%; max-height: 80vh; overflow-y: auto; box-shadow: 0 8px 32px rgba(0,0,0,0.4);";
    let btn_style = "padding: 0.5rem 1.25rem; border: none; border-radius: 6px; cursor: pointer; font-weight: 600; font-size: 0.9rem;";
    let btn_primary = format!("{} background: #3b82f6; color: white;", btn_style);
    let btn_secondary = format!("{} background: #475569; color: #e2e8f0;", btn_style);
    let btn_danger = format!("{} background: #ef4444; color: white;", btn_style);
    let tbl_style = "width: 100%; border-collapse: collapse; margin-top: 1rem;";
    let th_style = "padding: 0.5rem; text-align: left; border-bottom: 1px solid #475569; color: #94a3b8; font-size: 0.85rem;";
    let td_style = "padding: 0.5rem; border-bottom: 1px solid #334155; color: #e2e8f0; font-size: 0.85rem;";

    // Wrap on_close in Rc so it can be cloned for multiple event handlers
    let on_close = Rc::new(on_close);

    // Start replay
    let start_replay = {
        let run_id = run_id.clone();
        let set_state = set_replay_state.clone();
        let set_status = set_status_text.clone();
        let set_progress = set_progress_pct.clone();
        let set_completed = set_completed_prs.clone();
        let set_total = set_total_prs.clone();
        let set_error = set_error.clone();

        move || {
            set_state.set(ReplayState::Starting);
            set_status.set("Starting replay...".to_string());
            set_error.set(None);

            let run_id = run_id.clone();
            let set_state = set_state.clone();
            let set_status = set_status.clone();
            let set_progress = set_progress.clone();
            let set_completed = set_completed.clone();
            let set_total = set_total.clone();
            let set_error = set_error.clone();
            let set_original = set_original_detail.clone();

            wasm_bindgen_futures::spawn_local(async move {
                // First fetch the original run detail
                let orig_url = api_url(&format!("/api/runs/{}", run_id));
                match gloo_net::http::Request::get(&orig_url).send().await {
                    Ok(r) if r.ok() => {
                        if let Ok(detail) = r.json::<RunDetail>().await {
                            set_original.set(Some(detail));
                        }
                    }
                    _ => {}
                }

                // Start replay
                let replay_url = api_url(&format!("/api/runs/{}/replay", run_id));
                match gloo_net::http::Request::post(&replay_url).send().await {
                    Ok(r) if r.ok() => {
                        set_state.set(ReplayState::Running);
                        set_status.set("Replay started. Monitoring progress...".to_string());

                        // Start polling
                        let run_id = run_id.clone();
                        let set_state = set_state.clone();
                        let set_status = set_status.clone();
                        let set_progress = set_progress.clone();
                        let set_completed = set_completed.clone();
                        let set_total = set_total.clone();
                        let set_error = set_error.clone();
                        let set_replay = set_replay_detail.clone();

                        // Use a Cell to track if we're still running
                        let running = Rc::new(Cell::new(true));
                        let running_clone = running.clone();

                        // Poll every 500ms
                        let run_id_poll = run_id.clone();
                        wasm_bindgen_futures::spawn_local(async move {
                            let poll_url = api_url(&format!("/api/runs/{}/replay/status", run_id_poll));

                            loop {
                                if !running_clone.get() {
                                    break;
                                }

                                // Wait 500ms
                                gloo_timers::future::sleep(std::time::Duration::from_millis(500)).await;

                                if !running_clone.get() {
                                    break;
                                }

                                match gloo_net::http::Request::get(&poll_url).send().await {
                                    Ok(r) if r.ok() => {
                                        if let Ok(status) = r.json::<ReplayStatusResponse>().await {
                                            set_progress.set(status.progress_pct);
                                            set_completed.set(status.completed_prs);
                                            set_total.set(status.total_prs);
                                            set_status.set(status.message.clone());

                                            match status.status.as_str() {
                                                "completed" | "done" => {
                                                    running_clone.set(false);
                                                    set_state.set(ReplayState::Completed);
                                                    set_status.set("Replay completed!".to_string());

                                                    // Fetch replay run detail
                                                    let replay_run_id = status.run_id.clone();
                                                    let detail_url = api_url(&format!("/api/runs/{}", replay_run_id));
                                                    if let Ok(r) = gloo_net::http::Request::get(&detail_url).send().await {
                                                        if r.ok() {
                                                            if let Ok(detail) = r.json::<RunDetail>().await {
                                                                set_replay.set(Some(detail));
                                                            }
                                                        }
                                                    }
                                                }
                                                "failed" => {
                                                    running_clone.set(false);
                                                    set_state.set(ReplayState::Failed(status.message.clone()));
                                                    set_error.set(Some(status.message.clone()));
                                                }
                                                _ => {
                                                    // Keep polling
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Poll error: {}", e);
                                    }
                                    Ok(_) => {}
                                }
                            }
                        });
                    }
                    Ok(r) => {
                        let status_code = r.status();
                        set_state.set(ReplayState::Idle);
                        let msg = format!("Replay request failed (HTTP {})", status_code);
                        set_status.set(msg.clone());
                        set_error.set(Some(msg));
                    }
                    Err(e) => {
                        set_state.set(ReplayState::Idle);
                        let msg = format!("Network error: {}", e);
                        set_status.set(msg.clone());
                        set_error.set(Some(msg));
                    }
                }
            });
        }
    };

    // Pre-clone start_replay for multiple usage sites
    let start_replay_idle = start_replay.clone();
    let start_replay_failed = start_replay.clone();

    // Pre-clone Rc for each usage site in the view
    let on_close_bg = on_close.clone();
    let on_close_close_btn = on_close.clone();
    let on_close_cancel = on_close.clone();
    let on_close_completed = on_close.clone();
    let on_close_failed = on_close.clone();

    if !visible {
        return view! { <span></span> }.into_view();
    }

    view! {
        <div style=overlay_style on:click=move |ev: leptos::ev::MouseEvent| {
            // Close if clicking the overlay background (not the modal)
            let target = event_target::<web_sys::Element>(&ev);
            if target.tag_name().to_lowercase() == "div" {
                (on_close_bg)();
            }
        }>
            <div style=modal_style on:click=move |ev: leptos::ev::MouseEvent| {
                ev.stop_propagation();
            }>
                <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem;">
                    <h2 style="margin: 0; color: #e2e8f0;">"Replay Run"</h2>
                    <button
                        style=btn_secondary.clone()
                        on:click=move |_| (on_close_close_btn)()
                    >
                        "✕"
                    </button>
                </div>

                <p style="color: #94a3b8; margin-bottom: 1rem;">
                    "Replay this run with the same configuration to verify reproducibility."
                </p>

                // Error message
                {move || {
                    error.get().map(|e| {
                        view! {
                            <div style="background: #7f1d1d; border: 1px solid #ef4444; border-radius: 6px; padding: 0.75rem; margin-bottom: 1rem;">
                                <p style="color: #fca5a5; margin: 0; font-size: 0.9rem;">{e}</p>
                            </div>
                        }
                    })
                }}

                // Status area (during running/completed)
                {move || {
                    let state = replay_state.get();
                    match state {
                        ReplayState::Idle | ReplayState::Starting => {
                            view! { <span></span> }.into_view()
                        }
                        ReplayState::Running => {
                            view! {
                                <div style="margin-bottom: 1rem;">
                                    <ProgressBar
                                        value=progress_pct.get()
                                        max=100u32
                                        label=format!("{} — {} / {} PRs completed",
                                            status_text.get(),
                                            completed_prs.get(),
                                            total_prs.get()
                                        )
                                    />
                                </div>
                            }.into_view()
                        }
                        ReplayState::Completed => {
                            view! {
                                <div style="margin-bottom: 1rem;">
                                    <div style="background: #14532d; border: 1px solid #22c55e; border-radius: 6px; padding: 0.75rem; text-align: center;">
                                        <p style="color: #86efac; font-weight: 600; margin: 0;">"✓ Replay Completed"</p>
                                    </div>
                                </div>
                            }.into_view()
                        }
                        ReplayState::Failed(_) => {
                            view! { <span></span> }.into_view()
                        }
                    }
                }}

                // Comparison table (when completed)
                {move || {
                    let state = replay_state.get();
                    if !matches!(state, ReplayState::Completed) {
                        return view! { <span></span> }.into_view();
                    }

                    let orig = original_detail.get();
                    let repl = replay_detail.get();

                    match (orig, repl) {
                        (Some(orig), Some(repl)) => {
                            let orig_results_len = orig.results.len();
                            let repl_results_len = repl.results.len();
                            let orig_for_table = orig.clone();
                            let repl_for_table = repl.clone();
                            view! {
                                <div>
                                    <h3 style="color: #e2e8f0; margin-bottom: 0.75rem;">"Comparison: Original vs Replay"</h3>
                                    <table style=tbl_style>
                                        <thead>
                                            <tr>
                                                <th style=th_style>"Metric"</th>
                                                <th style=th_style>"Original"</th>
                                                <th style=th_style>"Replay"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {move || {
                                                let o_agg = orig.aggregate.as_ref();
                                                let r_agg = repl.aggregate.as_ref();

                                                let rows: Vec<(&str, String, String)> = vec![
                                                    ("Avg F1",
                                                        o_agg.map(|a| format!("{:.3}", a.avg_f1)).unwrap_or("—".into()),
                                                        r_agg.map(|a| format!("{:.3}", a.avg_f1)).unwrap_or("—".into())
                                                    ),
                                                    ("Avg Precision",
                                                        o_agg.map(|a| format!("{:.3}", a.avg_precision)).unwrap_or("—".into()),
                                                        r_agg.map(|a| format!("{:.3}", a.avg_precision)).unwrap_or("—".into())
                                                    ),
                                                    ("Avg Recall",
                                                        o_agg.map(|a| format!("{:.3}", a.avg_recall)).unwrap_or("—".into()),
                                                        r_agg.map(|a| format!("{:.3}", a.avg_recall)).unwrap_or("—".into())
                                                    ),
                                                    ("Total Cost",
                                                        o_agg.map(|a| format!("${:.4}", a.total_cost)).unwrap_or("—".into()),
                                                        r_agg.map(|a| format!("${:.4}", a.total_cost)).unwrap_or("—".into())
                                                    ),
                                                    ("Duration",
                                                        o_agg.map(|a| format!("{:.1}s", a.duration_secs)).unwrap_or("—".into()),
                                                        r_agg.map(|a| format!("{:.1}s", a.duration_secs)).unwrap_or("—".into())
                                                    ),
                                                    ("Total PRs",
                                                        format!("{}", orig_results_len),
                                                        format!("{}", repl_results_len)
                                                    ),
                                                ];

                                                rows.into_iter().map(|(metric, orig_val, replay_val)| {
                                                    view! {
                                                        <tr>
                                                            <td style=td_style>{metric}</td>
                                                            <td style=td_style>{orig_val}</td>
                                                            <td style=td_style>{replay_val}</td>
                                                        </tr>
                                                    }
                                                }).collect::<Vec<_>>()
                                            }}
                                        </tbody>
                                    </table>

                                    <h3 style="color: #e2e8f0; margin: 1rem 0 0.75rem;">"Per-PR Results"</h3>
                                    <table style=tbl_style>
                                        <thead>
                                            <tr>
                                                <th style=th_style>"PR"</th>
                                                <th style=th_style>"Original F1"</th>
                                                <th style=th_style>"Replay F1"</th>
                                                <th style=th_style>"Original Cost"</th>
                                                <th style=th_style>"Replay Cost"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {move || {
                                                let max_len = orig_results_len.max(repl_results_len);
                                                (0..max_len).map(|i| {
                                                    let orig_pr = orig_for_table.results.get(i);
                                                    let repl_pr = repl_for_table.results.get(i);
                                                    let pr_label = orig_pr.map(|p| format!("#{}", p.pr_number))
                                                        .or_else(|| repl_pr.map(|p| format!("#{}", p.pr_number)))
                                                        .unwrap_or_else(|| format!("#{}", i));

                                                    let orig_f1 = orig_pr.and_then(|p| p.f1).map(|v| format!("{:.3}", v)).unwrap_or("—".into());
                                                    let repl_f1 = repl_pr.and_then(|p| p.f1).map(|v| format!("{:.3}", v)).unwrap_or("—".into());
                                                    let orig_cost = orig_pr.and_then(|p| p.cost).map(|v| format!("${:.4}", v)).unwrap_or("—".into());
                                                    let repl_cost = repl_pr.and_then(|p| p.cost).map(|v| format!("${:.4}", v)).unwrap_or("—".into());

                                                    view! {
                                                        <tr>
                                                            <td style=td_style>{pr_label}</td>
                                                            <td style=td_style>{orig_f1}</td>
                                                            <td style=td_style>{repl_f1}</td>
                                                            <td style=td_style>{orig_cost}</td>
                                                            <td style=td_style>{repl_cost}</td>
                                                        </tr>
                                                    }
                                                }).collect::<Vec<_>>()
                                            }}
                                        </tbody>
                                    </table>
                                </div>
                            }.into_view()
                        }
                        _ => {
                            view! {
                                <p style="color: #94a3b8; font-style: italic;">
                                    "Loading comparison data..."
                                </p>
                            }.into_view()
                        }
                    }
                }}

                // Action buttons
                <div style="display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 1.5rem;">
                    {move || {
                        let state = replay_state.get();
                        let start_replay_idle = start_replay_idle.clone();
                        let start_replay_failed = start_replay_failed.clone();
                        match state {
                            ReplayState::Idle => {
                                let on_close = on_close_cancel.clone();
                                view! {
                                    <>
                                        <button style=btn_secondary.clone() on:click=move |_| (on_close)()>
                                            "Cancel"
                                        </button>
                                        <button style=btn_primary.clone() on:click=move |_| start_replay_idle()>
                                            "▶ Replay Run"
                                        </button>
                                    </>
                                }.into_view()
                            }
                            ReplayState::Starting | ReplayState::Running => {
                                view! {
                                    <button style=btn_danger.clone() disabled=true>
                                        "Running..."
                                    </button>
                                }.into_view()
                            }
                            ReplayState::Completed => {
                                let on_close = on_close_completed.clone();
                                view! {
                                    <button style=btn_primary.clone() on:click=move |_| (on_close)()>
                                        "Close"
                                    </button>
                                }.into_view()
                            }
                            ReplayState::Failed(_) => {
                                let on_close = on_close_failed.clone();
                                view! {
                                    <>
                                        <button style=btn_secondary.clone() on:click=move |_| (on_close)()>
                                            "Close"
                                        </button>
                                        <button style=btn_primary.clone() on:click=move |_| start_replay_failed()>
                                            "Retry Replay"
                                        </button>
                                    </>
                                }.into_view()
                            }
                        }
                    }}
                </div>
            </div>
        </div>
    }.into_view()
}

#[derive(Debug, Clone, PartialEq)]
enum ReplayState {
    Idle,
    Starting,
    Running,
    Completed,
    Failed(String),
}
