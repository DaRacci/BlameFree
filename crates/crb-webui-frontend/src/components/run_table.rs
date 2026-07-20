use std::cmp::Ordering;

use crb_types::review::{Review, ReviewStatus};
use leptos::either::{Either, EitherOf3};
use leptos::prelude::*;
use lucide_leptos::{ArrowDown, ArrowUp};

#[component]
pub fn RunTable(runs: Vec<Review>) -> impl IntoView {
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
            SortColumn::Name => sort_by(&a.id, &b.id, asc),
            SortColumn::Status => sort_by(&a.status, &b.status, asc),
            SortColumn::F1 => {
                Ordering::Equal // REMOVED: metrics was benchmark-specific
            }
            SortColumn::PrCount => {
                Ordering::Equal // REMOVED: results_len was benchmark-specific
            }
            SortColumn::Cost => {
                let a_v = a.analytics.as_ref().map(|a| a.total_cost());
                let b_v = b.analytics.as_ref().map(|a| a.total_cost());
                sort_by(a_v, b_v, asc)
            }
            SortColumn::Date => a.id.cmp(&b.id),
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
                        let badge_variant = match run.status {
                            ReviewStatus::Completed => "badge--success",
                            ReviewStatus::Failed => "badge--danger",
                            ReviewStatus::Running => "badge--warning",
                            ReviewStatus::Pending | ReviewStatus::Cancelled => "badge--neutral",
                        };
                        let f1_str = format!("{:.3}", 0.0); // REMOVED: metrics was benchmark-specific
                        let cost_str = run.analytics.as_ref().map(|a| format!("${:.4}", a.total_cost())).unwrap_or_else(|| "-".into());
                        let detail_path = format!("/runs/{}/", run.id);
                        let live_path = format!("/runs/{}/live", run.id);

                        let detail_path = detail_path;
                        let live_path = live_path;
                        view! {
                            <tr class="table__row table__row--clickable" data-href=detail_path.clone()>
                                <td class="table__td font-medium"><a href=detail_path.clone() style="color: var(--text-link, #58a6ff);">{run.id.to_string()}</a></td>
                                <td class="table__td">
                                    <span class=format!("badge {}", badge_variant)>
                                        <span class="badge__dot"></span>
                                        <span class="badge__label">{run.status.to_string()}</span>
                                    </span>
                                </td>
                                <td class="table__td">{0}</td>
                                <td class="table__td table__td--mono">{f1_str}</td>
                                <td class="table__td table__td--mono">{cost_str}</td>
                                <td class="table__td">
                                    <div class="flex-row gap-sm">
                                        <a href=detail_path.clone() class="btn btn--sm btn--secondary">"View"</a>
                                        {if run.status == ReviewStatus::Running || run.status == ReviewStatus::Pending {
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
