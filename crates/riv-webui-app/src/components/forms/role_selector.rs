use std::sync::Arc;

use leptos::prelude::*;
use riv_webui_shared::config::AgentInfo;

use super::checkbox_group::{CheckboxGroup, CheckboxOption};
use super::checkbox_select_bar::CheckboxSelectBar;

/// Role checkbox group with incompatibility-matrix policy.
///
/// Thin wrapper over [`CheckboxGroup`].
/// Derives reactive [`CheckboxOption`] vec from the static `available_roles` + reactive `selected_roles`,
/// applying the incompatibility disable + tooltip logic.
#[component]
pub fn RoleSelector(
    available_roles: Vec<AgentInfo>,
    selected_roles: ReadSignal<Vec<String>>,
    set_selected_roles: WriteSignal<Vec<String>>,
) -> impl IntoView {
    let role_infos = available_roles.clone();
    let roles_for_select_all = role_infos.clone();
    let select_all = Arc::new(move || maximal_roles(&roles_for_select_all));
    let options = Signal::derive(move || {
        let selected = selected_roles.get();
        role_infos
            .iter()
            .map(|role_info| {
                let abbr = &role_info.abbreviation;
                let disabled = is_role_disabled(abbr, &role_infos, &selected);
                let tooltip = if disabled {
                    Some(incompatibility_tooltip(abbr, &role_infos, &selected))
                } else {
                    None
                };
                CheckboxOption {
                    key: abbr.clone(),
                    label: role_info.display_name(),
                    disabled,
                    tooltip,
                }
            })
            .collect()
    });

    view! {
        <div class="role-selector">
            <CheckboxSelectBar
                options=options
                selected=selected_roles
                set_selected=set_selected_roles
                count_label=|checked: usize, total: usize| {
                    format!("{} / {} roles selected", checked, total)
                }
                select_all
            />
            <CheckboxGroup
                options=options
                selected=selected_roles
                set_selected=set_selected_roles
            />
        </div>
    }
}

/// Greedily build a maximal compatible selection.
/// Takes every role that can be enabled alongside the roles already chosen.
///
/// Used for Select All so that mutually-incompatible roles are never all selected at once.
fn maximal_roles(role_infos: &[AgentInfo]) -> Vec<String> {
    let mut selected: Vec<String> = Vec::new();
    for info in role_infos {
        if !is_role_disabled(&info.abbreviation, role_infos, &selected) {
            selected.push(info.abbreviation.clone());
        }
    }
    selected
}

fn is_role_disabled(role_abbr: &str, role_infos: &[AgentInfo], selected: &[String]) -> bool {
    if selected.contains(&role_abbr.to_string()) {
        return false;
    }

    for s in selected {
        if let Some(info) = role_infos.iter().find(|r| r.abbreviation == *s)
            && info
                .incompatible_with_roles
                .contains(&role_abbr.to_string())
        {
            return true;
        }

        if let Some(info) = role_infos.iter().find(|r| r.abbreviation == role_abbr)
            && info.incompatible_with_roles.contains(s)
        {
            return true;
        }
    }

    false
}

fn incompatibility_tooltip(
    role_abbr: &str,
    role_infos: &[AgentInfo],
    selected: &[String],
) -> String {
    let abbr_str = role_abbr.to_string();
    let incompatible: Vec<String> = role_infos
        .iter()
        .filter(|ri| {
            selected.contains(&ri.abbreviation) && ri.incompatible_with_roles.contains(&abbr_str)
        })
        .map(|ri| ri.abbreviation.clone())
        .chain(
            role_infos
                .iter()
                .filter(|ri| ri.abbreviation == role_abbr)
                .flat_map(|ri| {
                    ri.incompatible_with_roles
                        .iter()
                        .filter(|ir| selected.contains(ir))
                        .cloned()
                        .collect::<Vec<_>>()
                }),
        )
        .collect();

    format!("Incompatible with: {}", incompatible.join(", "))
}
