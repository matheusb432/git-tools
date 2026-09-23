use std::collections::BTreeSet;

use dioxus::prelude::*;
use gtl_wire::viewer::projects::{
    DiscoverProjectRepositories, DiscoveredProjectRepository, ImportProjectRepositories,
    ProjectDiscoveryState, ProjectImportOutcome, ProjectImportResult, ProjectImportSelection,
};

use super::loading::Projects;
use crate::{
    entities::diffs::viewer_server,
    shared::{
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, ScrollArea, TextInput,
            TextInputLabelVisibility,
        },
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone, PartialEq)]
struct ImportRow {
    path: String,
    label: String,
    state: ProjectDiscoveryState,
    selected: bool,
    project_id: String,
    title: String,
    outcome: Option<ProjectImportOutcome>,
}

impl ImportRow {
    fn available(&self) -> bool {
        !matches!(
            self.outcome,
            Some(ProjectImportOutcome::Created | ProjectImportOutcome::Restored)
        ) && matches!(
            self.state,
            ProjectDiscoveryState::New | ProjectDiscoveryState::Unmanaged(_)
        )
    }

    fn selection(&self) -> ProjectImportSelection {
        ProjectImportSelection {
            path: self.path.clone(),
            project_id: self.project_id.clone(),
            title: self.title.clone(),
        }
    }
}

fn import_rows(repositories: Vec<DiscoveredProjectRepository>) -> Vec<ImportRow> {
    let mut used = repositories
        .iter()
        .filter_map(|repository| match &repository.state {
            ProjectDiscoveryState::New => None,
            ProjectDiscoveryState::Active(id)
            | ProjectDiscoveryState::Paused(id)
            | ProjectDiscoveryState::Unmanaged(id) => Some(id.to_string()),
        })
        .collect::<BTreeSet<_>>();
    repositories
        .into_iter()
        .map(|repository| {
            let label = repository.label.to_string();
            let name = label.rsplit('/').next().unwrap_or(&label).to_owned();
            let project_id = match &repository.state {
                ProjectDiscoveryState::New => suggested_id(&name, &mut used),
                ProjectDiscoveryState::Active(id)
                | ProjectDiscoveryState::Paused(id)
                | ProjectDiscoveryState::Unmanaged(id) => id.to_string(),
            };
            ImportRow {
                path: repository.path.to_string(),
                label,
                state: repository.state,
                selected: false,
                project_id,
                title: name,
                outcome: None,
            }
        })
        .collect()
}

fn suggested_id(name: &str, used: &mut BTreeSet<String>) -> String {
    let mut base = name
        .chars()
        .filter(char::is_ascii_alphabetic)
        .take(4)
        .map(|character| character.to_ascii_uppercase())
        .collect::<String>();
    while base.len() < 2 {
        base.push('X');
    }
    if used.insert(base.clone()) {
        return base;
    }
    let prefix = base.chars().take(3).collect::<String>();
    for suffix in b'A'..=b'Z' {
        let candidate = format!("{prefix}{}", char::from(suffix));
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    base
}

#[component]
pub(super) fn ImportProjectsDialog() -> Element {
    let projects = use_context::<Projects>();
    let mut root = use_signal(String::new);
    let mut rows = use_signal(Vec::<ImportRow>::new);
    let mut scanned = use_signal(|| false);
    let mut scan = use_action(move |path: String| async move {
        let discovery = viewer_server::discover_project_repositories(DiscoverProjectRepositories {
            root: path,
        })
        .await?;
        root.set(discovery.root);
        rows.set(import_rows(discovery.repositories));
        scanned.set(true);
        Ok::<(), ViewerClientError>(())
    });
    let mut picker = use_action(move |()| async move {
        let picked = pick_folder().await?;
        if let Some(path) = picked {
            root.set(path.clone());
            rows.set(Vec::new());
            scanned.set(false);
            scan.call(path);
        }
        Ok::<(), ViewerClientError>(())
    });
    let mut import = use_action(move |request: ImportProjectRepositories| async move {
        let results = viewer_server::import_project_repositories(request).await?;
        let changed = results.iter().any(|result| {
            matches!(
                result.outcome,
                ProjectImportOutcome::Created | ProjectImportOutcome::Restored
            )
        });
        rows.with_mut(|rows| apply_results(rows, results));
        if changed {
            (projects.refresh)(());
        }
        Ok::<(), ViewerClientError>(())
    });
    let busy = scan.pending() || picker.pending() || import.pending();
    let selected = rows().iter().filter(|row| row.selected).count();
    let available = rows().iter().filter(|row| row.available()).count();
    let scan_error = scan.value().and_then(Result::err);
    let picker_error = picker.value().and_then(Result::err);
    let import_error = import.value().and_then(Result::err);
    let error = scan_error
        .or(picker_error)
        .or(import_error)
        .map(|error| error.to_string());

    rsx! {
        div { class: "flex h-full min-h-0 flex-col gap-4",
            div { class: "grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-end",
                TextInput {
                    label: "Folder to scan",
                    value: root(),
                    placeholder: "~/my-projects or /path/to/projects",
                    disabled: busy,
                    oninput: move |event: FormEvent| {
                        root.set(event.value());
                        rows.set(Vec::new());
                        scanned.set(false);
                        scan.reset();
                        picker.reset();
                        import.reset();
                    },
                }
                Button {
                    variant: ButtonVariant::Outline,
                    state: if busy { ButtonState::Disabled } else { ButtonState::Enabled },
                    onclick: move |_| picker.call(()),
                    "Choose folder"
                }
                Button {
                    state: if busy || root().trim().is_empty() { ButtonState::Disabled } else { ButtonState::Enabled },
                    onclick: move |_| {
                        let path = root().trim().to_owned();
                        rows.set(Vec::new());
                        scanned.set(false);
                        scan.call(path);
                    },
                    "Scan"
                }
            }
            if let Some(error) = error {
                p { class: "text-sm text-del", role: "alert", "{error}" }
            }
            if scan.pending() {
                p { class: "text-sm text-ink-2", role: "status", "Scanning folders…" }
            }
            if !rows().is_empty() {
                div { class: "flex items-center justify-between gap-3 border-b border-line pb-2",
                    p { class: "text-sm text-ink-2",
                        "{rows().len()} repositories found · {selected} selected"
                    }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Small,
                        state: if busy || available == 0 { ButtonState::Disabled } else { ButtonState::Enabled },
                        onclick: move |_| {
                            let all_selected = rows()
                                .iter()
                                .filter(|row| row.available())
                                .all(|row| row.selected);
                            rows.with_mut(|rows| {
                                for row in rows.iter_mut().filter(|row| row.available()) {
                                    row.selected = !all_selected;
                                }
                            });
                        },
                        if selected == available && available > 0 {
                            "Deselect all"
                        } else {
                            "Select all"
                        }
                    }
                }
            }
            ScrollArea { class: "min-h-0 flex-1 overflow-auto",
                if rows().is_empty() && !scan.pending() {
                    p { class: "py-8 text-center text-sm text-ink-2",
                        if scanned() {
                            "No Git repositories found in this folder."
                        } else {
                            "Choose a folder to find Git repositories. Results start unchecked."
                        }
                    }
                }
                div { class: "grid gap-3",
                    for (index, row) in rows().into_iter().enumerate() {
                        ProjectImportRow {
                            key: "{row.path}",
                            index,
                            row,
                            rows,
                            disabled: busy,
                        }
                    }
                }
            }
            div { class: "flex items-center justify-between gap-3 border-t border-line pt-3",
                p { class: "text-xs text-ink-2",
                    "Only selected repositories are added. Each result is reported separately."
                }
                Button {
                    state: if busy || selected == 0 { ButtonState::Disabled } else { ButtonState::Enabled },
                    onclick: move |_| {
                        import
                            .call(ImportProjectRepositories {
                                selections: rows()
                                    .into_iter()
                                    .filter(|row| row.selected && row.available())
                                    .map(|row| row.selection())
                                    .collect(),
                            });
                    },
                    "Add {selected} selected"
                }
            }
        }
    }
}

fn apply_results(rows: &mut [ImportRow], results: Vec<ProjectImportResult>) {
    for result in results {
        if let Some(row) = rows.iter_mut().find(|row| row.path == result.path) {
            row.selected = matches!(result.outcome, ProjectImportOutcome::Failed(_));
            row.outcome = Some(result.outcome);
        }
    }
}

#[component]
fn ProjectImportRow(
    index: usize,
    row: ImportRow,
    mut rows: Signal<Vec<ImportRow>>,
    disabled: bool,
) -> Element {
    let available = row.available();
    let state = match &row.outcome {
        Some(ProjectImportOutcome::Created) => "Created",
        Some(ProjectImportOutcome::Restored) => "Restored",
        Some(ProjectImportOutcome::Failed(_)) => "Failed",
        None => match row.state {
            ProjectDiscoveryState::New => "New",
            ProjectDiscoveryState::Active(_) => "Active project",
            ProjectDiscoveryState::Paused(_) => "Paused project",
            ProjectDiscoveryState::Unmanaged(_) => "Unmanaged · Restore",
        },
    };
    let editable = matches!(row.state, ProjectDiscoveryState::New) && available;
    rsx! {
        div {
            class: "rounded-panel border border-line-2 bg-surface-2 p-3",
            "data-testid": "project-import-row",
            div { class: "flex items-start gap-3",
                label { class: "flex min-w-0 flex-1 items-start gap-3",
                    input {
                        r#type: "checkbox",
                        class: "mt-1 size-4 accent-acc",
                        checked: row.selected,
                        disabled: disabled || !available,
                        aria_label: "Select {row.path}",
                        onclick: move |_| rows.with_mut(|rows| rows[index].selected = !rows[index].selected),
                    }
                    span { class: "min-w-0",
                        span { class: "block break-words font-medium text-ink", "{row.label}" }
                        span { class: "block break-all text-xs text-ink-2", "{row.path}" }
                    }
                }
                span { class: "shrink-0 text-xs text-ink-2", "{state}" }
            }
            if matches!(row.state, ProjectDiscoveryState::New)
                || matches!(row.state, ProjectDiscoveryState::Unmanaged(_))
            {
                div { class: "mt-3 grid gap-3 pl-7 sm:grid-cols-[7rem_minmax(0,1fr)]",
                    TextInput {
                        label: "Project ID for {row.label}",
                        label_visibility: TextInputLabelVisibility::Hidden,
                        value: row.project_id,
                        maxlength: "4",
                        disabled: disabled || !editable,
                        oninput: move |event: FormEvent| {
                            rows.with_mut(|rows| {
                                rows[index].project_id = event.value().to_ascii_uppercase();
                                rows[index].outcome = None;
                            });
                        },
                    }
                    TextInput {
                        label: "Project title for {row.label}",
                        label_visibility: TextInputLabelVisibility::Hidden,
                        value: row.title,
                        disabled: disabled || !editable,
                        oninput: move |event: FormEvent| {
                            rows.with_mut(|rows| {
                                rows[index].title = event.value();
                                rows[index].outcome = None;
                            });
                        },
                    }
                }
            }
            if let Some(ProjectImportOutcome::Failed(message)) = row.outcome {
                p { class: "mt-2 pl-7 text-xs text-del", role: "alert", "{message}" }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn pick_folder() -> Result<Option<String>, ViewerClientError> {
    gtl_client::pick_project_folder().await
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_folder() -> impl std::future::Future<Output = Result<Option<String>, ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Unavailable))
}

#[cfg(test)]
mod tests {
    use super::suggested_id;

    #[test]
    fn suggestions_use_unique_uppercase_ids() {
        let mut used = ["GITT".to_owned()].into_iter().collect();
        assert_eq!(suggested_id("git-tools", &mut used), "GITA");
        assert_eq!(suggested_id("git-tools", &mut used), "GITB");
        assert_eq!(suggested_id("a", &mut used), "AX");
    }
}
