use crb_webui_shared::runs::RunSummary;
use leptos::{IntoView, SignalGet, SignalSet, SignalUpdate, component, create_signal, view};

#[component]
pub fn RunTable(runs: Vec<RunSummary>) -> impl IntoView {
    let (sort_column, set_sort_column) = create_signal::<SortColumn>(SortColumn::Date);
    let (sort_asc, set_sort_asc) = create_signal(true);

    let toggle_sort = move |col: SortColumn| {
        if sort_column.get() == col {
            set_sort_asc.update(|v| *v = !*v);
        } else {
            set_sort_column.set(col);
            set_sort_asc.set(true);
        }
    };

    let sorted_runs = move || {
        let mut runs = runs.clone();
        let asc = sort_asc.get();
        match sort_column.get() {
            SortColumn::Name => {
                runs.sort_by(|a, b| {
                    if asc {
                        a.name.cmp(&b.name)
                    } else {
                        b.name.cmp(&a.name)
                    }
                });
            }
            SortColumn::Status => {
                runs.sort_by(|a, b| {
                    if asc {
                        a.status.cmp(&b.status)
                    } else {
                        b.status.cmp(&a.status)
                    }
                });
            }
            SortColumn::Model => {
                runs.sort_by(|a, b| {
                    let a_m = a.model.as_deref().unwrap_or("");
                    let b_m = b.model.as_deref().unwrap_or("");
                    if asc { a_m.cmp(b_m) } else { b_m.cmp(a_m) }
                });
            }
            SortColumn::F1 => {
                runs.sort_by(|a, b| {
                    let a_v = a.avg_f1.unwrap_or(-1.0);
                    let b_v = b.avg_f1.unwrap_or(-1.0);
                    if asc {
                        a_v.partial_cmp(&b_v).unwrap()
                    } else {
                        b_v.partial_cmp(&a_v).unwrap()
                    }
                });
            }
            SortColumn::PrCount => {
                runs.sort_by(|a, b| {
                    if asc {
                        a.pr_count.cmp(&b.pr_count)
                    } else {
                        b.pr_count.cmp(&a.pr_count)
                    }
                });
            }
            SortColumn::Cost => {
                runs.sort_by(|a, b| {
                    let a_v = a.total_cost.unwrap_or(0.0);
                    let b_v = b.total_cost.unwrap_or(0.0);
                    if asc {
                        a_v.partial_cmp(&b_v).unwrap()
                    } else {
                        b_v.partial_cmp(&a_v).unwrap()
                    }
                });
            }
            SortColumn::Date => {
                runs.sort_by(|a, b| {
                    if asc {
                        a.id.cmp(&b.id)
                    } else {
                        b.id.cmp(&a.id)
                    }
                });
            }
        }
        runs
    };

    let _sort_indicator = move |col| {
        if sort_column.get() == col {
            if sort_asc.get() { " ^" } else { " v" }
        } else {
            ""
        }
    };

    let sort_arrow = move |col| {
        if sort_column.get() == col {
            if sort_asc.get() { "^" } else { "v" }
        } else {
            ""
        }
    };

    view! {
        <div class="table-wrapper">
            <table class="table">
                <thead>
                    <tr>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Name)>
                            {move || format!("Name {}", sort_arrow(SortColumn::Name))}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Status)>
                            {move || format!("Status {}", sort_arrow(SortColumn::Status))}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Model)>
                            {move || format!("Model {}", sort_arrow(SortColumn::Model))}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::PrCount)>
                            {move || format!("PRs {}", sort_arrow(SortColumn::PrCount))}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::F1)>
                            {move || format!("F1 {}", sort_arrow(SortColumn::F1))}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Cost)>
                            {move || format!("Cost {}", sort_arrow(SortColumn::Cost))}
                        </th>
                        <th class="table__th">"Details"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || sorted_runs().into_iter().map(|run| {
                        let badge_variant = match run.status.as_str() {
                            "done" => "badge--success",
                            "failed" => "badge--danger",
                            "running" => "badge--warning",
                            _ => "badge--neutral",
                        };
                        let f1_str = run.avg_f1.map(|v| format!("{:.3}", v)).unwrap_or_else(|| "-".into());
                        let cost_str = run.total_cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "-".into());
                        let model_str = run.model.unwrap_or_else(|| "-".to_string());
                        let detail_path = format!("/runs/{}", run.id);
                        let live_path = format!("/runs/{}/live", run.id);

                        view! {
                            <tr class="table__row table__row--clickable" data-href=&detail_path>
                                <td class="table__td" style="font-weight: var(--weight-medium, 500);"><a href=&detail_path style="color: var(--text-link, #58a6ff);">{&run.name}</a></td>
                                <td class="table__td">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{&run.status}</span>
                                    </span>
                                </td>
                                <td class="table__td">{model_str}</td>
                                <td class="table__td">{run.pr_count}</td>
                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{f1_str}</td>
                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{cost_str}</td>
                                <td class="table__td">
                                    <div style="display: flex; gap: 0.5rem;">
                                        <a href=&detail_path class="btn btn--sm btn--secondary">"View"</a>
                                        {if run.status == "running" || run.status == "pending" {
                                            view! {
                                                <a href=&live_path class="btn btn--sm btn--secondary">"Live"</a>
                                            }.into_view()
                                        } else {
                                            view! { <span></span> }.into_view()
                                        }}
                                    </div>
                                </td>
                            </tr>
                        }
                    }).collect::<Vec<_>>()}
                </tbody>
            </table>
        </div>
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Name,
    Status,
    Model,
    PrCount,
    F1,
    Cost,
    Date,
}
