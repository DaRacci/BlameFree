use crb_webui_shared::config::RoleInfo;
use leptos::{
    IntoView, ReadSignal, SignalGet, SignalUpdate, SignalWith, WriteSignal, component, view,
};

/// A reusable checkbox group for selecting roles/agents.
///
/// Displays each role abbreviation as a checkbox. Roles that are incompatible
/// with currently selected roles are disabled with a tooltip explaining why.
#[component]
pub fn RoleSelector(
    /// All available roles (with their incompatibility info).
    available_roles: Vec<RoleInfo>,
    /// Write-signal for the currently selected role abbreviations.
    selected_roles: ReadSignal<Vec<String>>,
    /// Write-signal to update the selected role abbreviations.
    set_selected_roles: WriteSignal<Vec<String>>,
) -> impl IntoView {
    let is_role_disabled = move |role_abbr: &str, role_infos: &Vec<RoleInfo>| -> bool {
        let selected = selected_roles.get();
        if selected.contains(&role_abbr.to_string()) {
            return false;
        }
        for s in &selected {
            if let Some(info) = role_infos.iter().find(|r| r.abbreviation == *s) {
                if info
                    .incompatible_with_roles
                    .contains(&role_abbr.to_string())
                {
                    return true;
                }
            }
            if let Some(info) = role_infos.iter().find(|r| r.abbreviation == role_abbr) {
                if info.incompatible_with_roles.contains(s) {
                    return true;
                }
            }
        }
        false
    };

    let toggle_role = move |role: &str| {
        let role = role.to_string();
        set_selected_roles.update(|roles| {
            if let Some(pos) = roles.iter().position(|r| r == &role) {
                roles.remove(pos);
            } else {
                roles.push(role);
            }
        });
    };

    let is_role_selected = move |role: &str| -> bool {
        selected_roles.with(|roles| roles.contains(&role.to_string()))
    };

    let role_infos_cloned = available_roles.clone();

    view! {
        {available_roles
            .into_iter()
            .map(|role_info| {
                let abbr = role_info.abbreviation.clone();
                let abbr_display = abbr.clone();
                let checked = is_role_selected(&abbr);
                let disabled = is_role_disabled(&abbr, &role_infos_cloned);
                let title = if disabled {
                    let incompatible_with: Vec<String> =
                        role_infos_cloned
                            .iter()
                            .filter(|ri| {
                                let selected = selected_roles.get();
                                selected.contains(&ri.abbreviation)
                                    && ri.incompatible_with_roles.contains(&abbr)
                            })
                            .map(|ri| ri.abbreviation.clone())
                            .chain(
                                role_infos_cloned
                                    .iter()
                                    .filter(|ri| {
                                        let selected = selected_roles.get();
                                        ri.abbreviation == abbr
                                            && selected
                                                .iter()
                                                .any(|s| ri.incompatible_with_roles.contains(s))
                                    })
                                    .flat_map(|ri| {
                                        let selected = selected_roles.get();
                                        ri.incompatible_with_roles
                                            .iter()
                                            .filter(|ir| selected.contains(ir))
                                            .cloned()
                                            .collect::<Vec<_>>()
                                    }),
                            )
                            .collect::<Vec<_>>();
                    format!("Incompatible with: {}", incompatible_with.join(", "))
                } else {
                    String::new()
                };
                let label_class = if disabled {
                    "checkbox-label checkbox-label--disabled"
                } else {
                    "checkbox-label"
                };
                view! {
                    <label class=label_class>
                        <input
                            type="checkbox"
                            prop:checked=checked
                            disabled=disabled
                            on:click={
                                let abbr = abbr.clone();
                                move |_| toggle_role(&abbr)
                            }
                        />
                        <span title=title>{abbr_display}</span>
                    </label>
                }
            })
            .collect::<Vec<_>>()}
    }
}
