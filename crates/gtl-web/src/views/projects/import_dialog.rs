use std::collections::BTreeSet;

use dioxus::prelude::*;
use gtl_models::{
    failure::{Failure, ProjectFailure},
    projects::catalogue::{ProjectId, ProjectTitle},
    settings::ViewerLanguage,
};
use gtl_wire::viewer::projects::{
    DiscoverProjectRepositories, DiscoveredProjectRepository, ImportProjectRepositories,
    ProjectDiscoveryState, ProjectImportOutcome, ProjectImportResult, ProjectImportSelection,
};

use super::loading::Projects;
use crate::{
    entities::diffs::viewer_server,
    shared::{
        failure_message::failure_message,
        failure_notice::captured_error_message,
        field_errors::{FieldErrors, FormField},
        i18n::{t, use_language},
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, ScrollArea, TextInput,
            TextInputLabelVisibility,
        },
        viewer_client::{ViewerClientError, captured_client_error},
    },
};

/// The folder input that scans for repositories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScanField {
    Root,
}

impl FormField for ScanField {
    const ALL: &'static [Self] = &[Self::Root];

    fn request_field(self) -> &'static str {
        "root"
    }

    fn correction(self, language: ViewerLanguage) -> String {
        t!(language, "projects-import-folder-correction")
    }
}

/// Places a scan failure on the folder input when the folder caused it.
fn scan_field_errors(error: &ViewerClientError) -> Option<FieldErrors<ScanField>> {
    let failure = error.failure()?;
    if let Some(errors) = FieldErrors::from_failure(failure) {
        return Some(errors);
    }
    match failure {
        Failure::Project(
            ProjectFailure::ScanFolderInvalid { .. }
            | ProjectFailure::ScanFailed { .. }
            | ProjectFailure::HomeUnavailable
            | ProjectFailure::TooManyRepositories { .. },
        ) => {
            let mut errors = FieldErrors::default();
            errors.reject_with(ScanField::Root, failure.clone());
            Some(errors)
        }
        _ => None,
    }
}

/// The editable inputs of one new repository row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportRowField {
    ProjectId,
    Title,
}

impl FormField for ImportRowField {
    const ALL: &'static [Self] = &[Self::ProjectId, Self::Title];

    fn request_field(self) -> &'static str {
        match self {
            Self::ProjectId => "project_id",
            Self::Title => "title",
        }
    }

    fn correction(self, language: ViewerLanguage) -> String {
        match self {
            Self::ProjectId => t!(language, "projects-import-id-correction"),
            Self::Title => t!(language, "projects-import-title-correction"),
        }
    }
}

#[derive(Clone, PartialEq)]
struct ImportRow {
    path: String,
    label: String,
    state: ProjectDiscoveryState,
    selected: bool,
    project_id: String,
    title: String,
    outcome: Option<ProjectImportOutcome>,
    field_errors: FieldErrors<ImportRowField>,
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

    /// Parses the row into a selection; restored rows keep their stored ID and title.
    fn selection(&self) -> Result<ProjectImportSelection, FieldErrors<ImportRowField>> {
        if !matches!(self.state, ProjectDiscoveryState::New) {
            return Ok(ProjectImportSelection {
                path: self.path.clone(),
                project_id: self.project_id.clone(),
                title: self.title.clone(),
            });
        }
        let mut errors = FieldErrors::default();
        let project_id = errors.parse(
            ImportRowField::ProjectId,
            ProjectId::try_new(self.project_id.trim().to_owned()),
        );
        let title = errors.parse(
            ImportRowField::Title,
            ProjectTitle::try_new(self.title.trim().to_owned()),
        );
        match (project_id, title) {
            (Some(project_id), Some(title)) => Ok(ProjectImportSelection {
                path: self.path.clone(),
                project_id: project_id.to_string(),
                title: title.to_string(),
            }),
            _ => Err(errors),
        }
    }
}

/// Parses every selected row, recording corrections on the rows that cannot be imported.
fn import_selections(rows: &mut [ImportRow]) -> Option<Vec<ProjectImportSelection>> {
    let mut selections = Vec::new();
    let mut valid = true;
    for row in rows
        .iter_mut()
        .filter(|row| row.selected && row.available())
    {
        match row.selection() {
            Ok(selection) => {
                row.field_errors = FieldErrors::default();
                selections.push(selection);
            }
            Err(errors) => {
                row.field_errors = errors;
                valid = false;
            }
        }
    }
    valid.then_some(selections)
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
                field_errors: FieldErrors::default(),
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
    let language = use_language();
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
        let picked = pick_folder(t!(language, "projects-import-picker-title")).await?;
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
    let scan_field_errors = scan_error
        .as_ref()
        .and_then(captured_client_error)
        .and_then(scan_field_errors)
        .unwrap_or_default();
    let picker_error = picker.value().and_then(Result::err);
    let import_error = import.value().and_then(Result::err);
    let error = scan_error
        .filter(|_| scan_field_errors.is_empty())
        .or(picker_error)
        .or(import_error)
        .map(|error| captured_error_message(&error, language));

    rsx! {
        div { class: "flex h-full min-h-0 flex-col gap-4",
            div {
                class: "grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-end",
                "data-tour": super::tours::IMPORT_SCAN.value(),
                TextInput {
                    id: "project-import-root",
                    label: t!(language, "projects-import-folder"),
                    value: root(),
                    error: scan_field_errors.message(ScanField::Root, language),
                    placeholder: t!(language, "projects-import-folder-placeholder"),
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
                    {t!(language, "projects-import-choose-folder")}
                }
                Button {
                    state: if busy || root().trim().is_empty() { ButtonState::Disabled } else { ButtonState::Enabled },
                    onclick: move |_| {
                        let path = root().trim().to_owned();
                        rows.set(Vec::new());
                        scanned.set(false);
                        scan.call(path);
                    },
                    {t!(language, "projects-import-scan")}
                }
            }
            if let Some(error) = error {
                p { class: "text-sm text-del", role: "alert", "{error}" }
            }
            if scan.pending() {
                p { class: "text-sm text-ink-2", role: "status",
                    {t!(language, "projects-import-scanning")}
                }
            }
            if !rows().is_empty() {
                div { class: "flex items-center justify-between gap-3 border-b border-line pb-2",
                    p { class: "text-sm text-ink-2",
                        {t!(language, "projects-import-found", found = rows().len(), selected = selected)}
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
                            {t!(language, "projects-import-deselect-all")}
                        } else {
                            {t!(language, "projects-import-select-all")}
                        }
                    }
                }
            }
            ScrollArea {
                class: "min-h-0 flex-1 overflow-auto",
                "data-tour": super::tours::IMPORT_ROWS.value(),
                if rows().is_empty() && !scan.pending() {
                    p { class: "py-8 text-center text-sm text-ink-2",
                        if scanned() {
                            {t!(language, "projects-import-none-found")}
                        } else {
                            {t!(language, "projects-import-empty")}
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
            div {
                class: "flex items-center justify-between gap-3 border-t border-line pt-3",
                "data-tour": super::tours::IMPORT_SUBMIT.value(),
                p { class: "text-xs text-ink-2", {t!(language, "projects-import-footer")} }
                Button {
                    state: if busy || selected == 0 { ButtonState::Disabled } else { ButtonState::Enabled },
                    onclick: move |_| {
                        if let Some(selections) = rows.with_mut(|rows| import_selections(rows)) {
                            import
                                .call(ImportProjectRepositories {
                                    selections,
                                });
                        }
                    },
                    {t!(language, "projects-import-add", selected = selected)}
                }
            }
        }
    }
}

fn apply_results(rows: &mut [ImportRow], results: Vec<ProjectImportResult>) {
    for result in results {
        if let Some(row) = rows.iter_mut().find(|row| row.path == result.path) {
            row.selected = matches!(result.outcome, ProjectImportOutcome::Failed(_));
            row.field_errors = rejected_fields(&result.outcome);
            row.outcome = Some(result.outcome);
        }
    }
}

/// The row inputs a failed import names; other failures stay on the row.
fn rejected_fields(outcome: &ProjectImportOutcome) -> FieldErrors<ImportRowField> {
    match outcome {
        ProjectImportOutcome::Failed(failure) => {
            FieldErrors::from_failure(failure).unwrap_or_default()
        }
        ProjectImportOutcome::Created | ProjectImportOutcome::Restored => FieldErrors::default(),
    }
}

#[component]
fn ProjectImportRow(
    index: usize,
    row: ImportRow,
    mut rows: Signal<Vec<ImportRow>>,
    disabled: bool,
) -> Element {
    let language = use_language();
    let available = row.available();
    let state = match &row.outcome {
        Some(ProjectImportOutcome::Created) => t!(language, "projects-import-created"),
        Some(ProjectImportOutcome::Restored) => t!(language, "projects-import-restored"),
        Some(ProjectImportOutcome::Failed(_)) => t!(language, "projects-import-failed"),
        None => match row.state {
            ProjectDiscoveryState::New => t!(language, "projects-import-new"),
            ProjectDiscoveryState::Active(_) => t!(language, "projects-import-active"),
            ProjectDiscoveryState::Paused(_) => t!(language, "projects-import-paused"),
            ProjectDiscoveryState::Unmanaged(_) => t!(language, "projects-import-unmanaged"),
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
                        aria_label: t!(language, "projects-import-select-row", path = row.path.as_str()),
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
                        id: "project-import-{index}-id",
                        label: t!(language, "projects-import-id-label", project = row.label.as_str()),
                        label_visibility: TextInputLabelVisibility::Hidden,
                        value: row.project_id,
                        maxlength: "4",
                        disabled: disabled || !editable,
                        error: row.field_errors.message(ImportRowField::ProjectId, language),
                        oninput: move |event: FormEvent| {
                            rows.with_mut(|rows| {
                                rows[index].project_id = event.value().to_ascii_uppercase();
                                rows[index].field_errors.clear(ImportRowField::ProjectId);
                                rows[index].outcome = None;
                            });
                        },
                    }
                    TextInput {
                        id: "project-import-{index}-title",
                        label: t!(language, "projects-import-title-label", project = row.label.as_str()),
                        label_visibility: TextInputLabelVisibility::Hidden,
                        value: row.title,
                        disabled: disabled || !editable,
                        error: row.field_errors.message(ImportRowField::Title, language),
                        oninput: move |event: FormEvent| {
                            rows.with_mut(|rows| {
                                rows[index].title = event.value();
                                rows[index].field_errors.clear(ImportRowField::Title);
                                rows[index].outcome = None;
                            });
                        },
                    }
                }
            }
            if let Some(ProjectImportOutcome::Failed(failure)) = row
                .outcome
                .filter(|_| row.field_errors.is_empty())
            {
                p { class: "mt-2 pl-7 text-xs text-del", role: "alert",
                    {failure_message(&failure, language)}
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn pick_folder(title: String) -> Result<Option<String>, ViewerClientError> {
    gtl_client::pick_project_folder(title).await
}

#[cfg(not(target_arch = "wasm32"))]
fn pick_folder(
    _title: String,
) -> impl std::future::Future<Output = Result<Option<String>, ViewerClientError>> {
    std::future::ready(Err(ViewerClientError::Disconnected))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_row(project_id: &str, title: &str) -> ImportRow {
        ImportRow {
            path: "/repos/example".into(),
            label: "example".into(),
            state: ProjectDiscoveryState::New,
            selected: true,
            project_id: project_id.into(),
            title: title.into(),
            outcome: None,
            field_errors: FieldErrors::default(),
        }
    }

    #[test]
    fn invalid_rows_keep_a_correction_on_each_rejected_input() {
        let mut rows = [new_row("x", "  "), new_row("EXM", " Example ")];

        assert_eq!(import_selections(&mut rows), None);
        assert!(
            rows[0]
                .field_errors
                .message(ImportRowField::ProjectId, ViewerLanguage::EnUs)
                .is_some()
        );
        assert!(
            rows[0]
                .field_errors
                .message(ImportRowField::Title, ViewerLanguage::EnUs)
                .is_some()
        );
        assert!(rows[1].field_errors.is_empty());
    }

    #[test]
    fn valid_rows_import_trimmed_values() {
        let mut rows = [new_row("EXM", " Example ")];

        let selections = import_selections(&mut rows).unwrap();

        assert_eq!(selections[0].project_id, "EXM");
        assert_eq!(selections[0].title, "Example");
    }

    #[test]
    fn server_field_rejections_mark_the_row_input() {
        let mut rows = [new_row("EXM", "Example")];

        apply_results(
            &mut rows,
            vec![ProjectImportResult {
                path: "/repos/example".into(),
                project_id: "EXM".into(),
                outcome: ProjectImportOutcome::Failed(Failure::InvalidRequest {
                    field: "project_id".into(),
                }),
            }],
        );

        assert!(
            rows[0]
                .field_errors
                .message(ImportRowField::ProjectId, ViewerLanguage::EnUs)
                .is_some()
        );
    }

    #[test]
    fn suggestions_use_unique_uppercase_ids() {
        let mut used = ["GITT".to_owned()].into_iter().collect();
        assert_eq!(suggested_id("git-tools", &mut used), "GITA");
        assert_eq!(suggested_id("git-tools", &mut used), "GITB");
        assert_eq!(suggested_id("a", &mut used), "AX");
    }
}
