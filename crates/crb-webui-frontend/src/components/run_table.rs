use std::cmp::Ordering;

use crb_types::benchmark::metrics::MetricsProvider;
use crb_types::wrappers::WrappedData;
use crb_webui_shared::runs::{RunStatus, RunSummary};
use leptos::either::{Either, EitherOf3};
use leptos::prelude::*;
use lucide_leptos::{ArrowDown, ArrowUp};

#[component]
pub fn RunTable(runs: Vec<RunSummary>) -> impl IntoView {
    let (sort_column, set_sort_column) = signal::<SortColumn>(SortColumn::Date);
    let (sort_asc, set_sort_asc) = signal(true);

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
        runs.sort_by(|a, b| match sort_column.get() {
            SortColumn::Name => sort_by(&a.meta.name, &b.meta.name, asc),
            SortColumn::Status => sort_by(&a.meta.status, &b.meta.status, asc),
            SortColumn::Model => {
                let a_m = a.meta.model.as_ref().map(|m| m.get()).unwrap_or("");
                let b_m = b.meta.model.as_ref().map(|m| m.get()).unwrap_or("");
                sort_by(a_m, b_m, asc)
            }
            SortColumn::F1 => {
                let a_v = a.metrics.f1();
                let b_v = b.metrics.f1();
                sort_by(a_v, b_v, asc)
            }
            SortColumn::PrCount => a.meta.pr_count.cmp(&b.meta.pr_count),
            SortColumn::Cost => {
                let a_v = a.meta.total_cost;
                let b_v = b.meta.total_cost;
                sort_by(a_v, b_v, asc)
            }
            SortColumn::Date => a.meta.id.cmp(&b.meta.id),
        });
        runs
    };

    let sort_icon = move |col| -> EitherOf3<_, _, ()> {
        if sort_column.get() == col {
            if sort_asc.get() {
                EitherOf3::A(view! { <ArrowUp size=14 /> })
            } else {
                EitherOf3::B(view! { <ArrowDown size=14 /> })
            }
        } else {
            EitherOf3::C(())
        }
    };

    view! {
        <div class="table-wrapper">
            <table class="table">
                <thead>
                    <tr>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Name)>
                            {move || view! { "Name " {sort_icon(SortColumn::Name)} }}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Status)>
                            {move || view! { "Status " {sort_icon(SortColumn::Status)} }}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Model)>
                            {move || view! { "Model " {sort_icon(SortColumn::Model)} }}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::PrCount)>
                            {move || view! { "PRs " {sort_icon(SortColumn::PrCount)} }}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::F1)>
                            {move || view! { "F1 " {sort_icon(SortColumn::F1)} }}
                        </th>
                        <th class="table__th table__th--sortable" on:click=move |_| toggle_sort(SortColumn::Cost)>
                            {move || view! { "Cost " {sort_icon(SortColumn::Cost)} }}
                        </th>
                        <th class="table__th">"Details"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || sorted_runs().into_iter().map(|run| {
                        let badge_variant = match run.meta.status {
                            RunStatus::Completed => "badge--success",
                            RunStatus::Failed => "badge--danger",
                            RunStatus::Running => "badge--warning",
                            RunStatus::Pending | RunStatus::Cancelled => "badge--neutral",
                        };
                        let f1_str = format!("{:.3}", run.metrics.f1());
                        let cost_str = run.meta.total_cost.map(|v| format!("${:.4}", v)).unwrap_or_else(|| "-".into());
                        let model_str = run.meta.model.as_ref().map(|m| m.get()).unwrap_or_else(|| "-");
                        let detail_path = format!("/runs/{}/", run.meta.id);
                        let live_path = format!("/runs/{}/live", run.meta.id);

                        let detail_path = detail_path;
                        let live_path = live_path;
                        view! {
                            <tr class="table__row table__row--clickable" data-href=detail_path.clone()>
                                <td class="table__td" style="font-weight: var(--weight-medium, 500);"><a href=detail_path.clone() style="color: var(--text-link, #58a6ff);">{run.meta.name.clone()}</a></td>
                                <td class="table__td">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{run.meta.status.to_string()}</span>
                                    </span>
                                </td>
                                <td class="table__td">{model_str}</td>
                                <td class="table__td">{run.meta.pr_count}</td>
                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{f1_str}</td>
                                <td class="table__td" style="font-family: var(--font-mono, monospace);">{cost_str}</td>
                                <td class="table__td">
                                    <div style="display: flex; gap: 0.5rem;">
                                        <a href=detail_path.clone() class="btn btn--sm btn--secondary">"View"</a>
                                        {if run.meta.status == RunStatus::Running || run.meta.status == RunStatus::Pending {
                                            Either::Left(
                                                view! {
                                                    <a href=live_path class="btn btn--sm btn--secondary">"Live"</a>
                                                }
                                            )
                                        } else {
                                            Either::Right(
                                                view! { <span></span> }
                                            )
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

fn sort_by<T>(a: T, b: T, asc: bool) -> Ordering
where
    T: PartialOrd,
{
    if asc {
        a.partial_cmp(&b).unwrap_or(Ordering::Equal)
    } else {
        b.partial_cmp(&a).unwrap_or(Ordering::Equal)
    }
}
