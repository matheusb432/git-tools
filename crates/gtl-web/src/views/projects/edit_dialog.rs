use dioxus::prelude::*;
use gtl_models::{
    paths::ProjectName, projects::comparison::ComparisonBranch, settings::ViewerLanguage,
};
use gtl_wire::viewer::{
    EditSettingsRequest, FieldUpdate, ViewerUserSettings,
    projects::{UpdateViewerProject, ViewerProject},
};
use lucide_dioxus::History;

use crate::{
    entities::diffs::viewer_server,
    shared::{
        failure_notice::client_error_message,
        field_errors::{FieldErrors, FormField},
        i18n::{t, use_language},
        ui::{
            AlertDialog, Button, ButtonSize, ButtonState, ButtonType, ButtonVariant, Checkbox,
            PanelDialog, TextInput, panel_dialog::PanelDialogVariant,
        },
        unsaved_changes_confirmation::use_unsaved_changes_confirmation,
        viewer_client::{ViewerClientError, captured_client_error},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectEditField {
    ComparisonBranch,
}

impl FormField for ProjectEditField {
    const ALL: &'static [Self] = &[Self::ComparisonBranch];

    fn request_field(self) -> &'static str {
        "comparison_branch"
    }

    fn correction(self, language: ViewerLanguage) -> String {
        t!(language, "projects-comparison-branch-correction")
    }
}

/// Values the server last confirmed; `None` while the push preference is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectEditBaseline {
    comparison_branch: ComparisonBranch,
    push_without_confirmation: Option<bool>,
}

/// Unsaved form values; `None` keeps the saved push preference.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProjectEditDraft {
    comparison_branch: String,
    push_without_confirmation: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ProjectEditPlan {
    comparison_branch: Option<ComparisonBranch>,
    push_without_confirmation: Option<bool>,
}

impl ProjectEditDraft {
    fn differs_from(&self, baseline: &ProjectEditBaseline) -> bool {
        self.comparison_branch != baseline.comparison_branch.as_ref()
            || self
                .push_without_confirmation
                .is_some_and(|value| Some(value) != baseline.push_without_confirmation)
    }

    fn plan(
        &self,
        baseline: &ProjectEditBaseline,
    ) -> Result<ProjectEditPlan, FieldErrors<ProjectEditField>> {
        let mut errors = FieldErrors::default();
        let branch = errors.parse(
            ProjectEditField::ComparisonBranch,
            ComparisonBranch::try_new(self.comparison_branch.clone()),
        );
        let Some(branch) = branch else {
            return Err(errors);
        };
        Ok(ProjectEditPlan {
            comparison_branch: (branch != baseline.comparison_branch).then_some(branch),
            push_without_confirmation: self
                .push_without_confirmation
                .filter(|value| Some(*value) != baseline.push_without_confirmation),
        })
    }
}

fn push_confirmation_request(
    settings: &ViewerUserSettings,
    project: &ProjectName,
    push_without_confirmation: bool,
) -> EditSettingsRequest {
    EditSettingsRequest {
        expected_revision: Some(settings.revision),
        viewer_push_no_confirmation_projects: FieldUpdate::Update(push_confirmation_projects(
            &settings.viewer_push_no_confirmation_projects,
            project,
            push_without_confirmation,
        )),
        ..EditSettingsRequest::default()
    }
}

fn push_confirmation_projects(
    projects: &[ProjectName],
    project: &ProjectName,
    push_without_confirmation: bool,
) -> Vec<ProjectName> {
    projects
        .iter()
        .filter(|name| *name != project)
        .chain(push_without_confirmation.then_some(project))
        .cloned()
        .collect()
}

/// The writes one submission sends, in order.
#[derive(Clone)]
struct ProjectEditRequests {
    comparison_branch: Option<ComparisonBranchChange>,
    settings: Option<EditSettingsRequest>,
}

#[derive(Clone)]
struct ComparisonBranchChange {
    branch: ComparisonBranch,
    expected: ComparisonBranch,
}

impl ProjectEditRequests {
    const fn is_empty(&self) -> bool {
        self.comparison_branch.is_none() && self.settings.is_none()
    }
}

impl ProjectEditPlan {
    /// Returns `None` while a push preference change waits for the loaded settings.
    fn requests(
        self,
        expected_branch: &ComparisonBranch,
        settings: Option<&ViewerUserSettings>,
        project: &ProjectName,
    ) -> Option<ProjectEditRequests> {
        let settings = match self.push_without_confirmation {
            Some(value) => Some(push_confirmation_request(settings?, project, value)),
            None => None,
        };
        Some(ProjectEditRequests {
            comparison_branch: self.comparison_branch.map(|branch| ComparisonBranchChange {
                branch,
                expected: expected_branch.clone(),
            }),
            settings,
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
struct ProjectEdit {
    draft: ReadSignal<ProjectEditDraft>,
    baseline: Memo<ProjectEditBaseline>,
    dirty: Memo<bool>,
    settings_error: Memo<Option<ViewerClientError>>,
    pending: Memo<bool>,
    field_errors: Memo<FieldErrors<ProjectEditField>>,
    form_error: Memo<Option<ViewerClientError>>,
    change_branch: Callback<String>,
    reveal_errors: Callback<()>,
    change_push: Callback<bool>,
    submit: Callback<()>,
    reload_settings: Callback<()>,
}

struct PushPreference {
    settings: Resource<Result<ViewerUserSettings, ViewerClientError>>,
    loaded: Memo<Option<ViewerUserSettings>>,
    enabled: Memo<Option<bool>>,
    error: Memo<Option<ViewerClientError>>,
}

fn use_push_preference(project: &ProjectName) -> PushPreference {
    let project = project.clone();
    let settings = use_resource(viewer_server::get_settings);
    let loaded = use_memo(move || {
        settings
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned()
    });
    PushPreference {
        settings,
        loaded,
        enabled: use_memo(move || {
            loaded().map(|settings| {
                settings
                    .viewer_push_no_confirmation_projects
                    .contains(&project)
            })
        }),
        error: use_memo(move || {
            settings
                .read()
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .cloned()
        }),
    }
}

fn use_project_edit(project: &ViewerProject, onsaved: EventHandler<()>) -> ProjectEdit {
    let path = project.path.clone();
    let name = project.name.clone();
    let initial_branch = project.comparison_branch.clone();
    let push_preference = use_push_preference(&name);
    let mut settings = push_preference.settings;
    let loaded_settings = push_preference.loaded;
    let saved_branch = use_signal(|| initial_branch.clone());
    let mut draft = use_signal(|| ProjectEditDraft {
        comparison_branch: initial_branch.to_string(),
        push_without_confirmation: None,
    });
    let mut errors_revealed = use_signal(|| false);
    let baseline = use_memo(move || ProjectEditBaseline {
        comparison_branch: saved_branch(),
        push_without_confirmation: (push_preference.enabled)(),
    });
    let mut action = use_action(move |requests: ProjectEditRequests| {
        let path = path.clone();
        async move {
            save_project_edit(path, requests, saved_branch, settings).await?;
            onsaved.call(());
            Ok::<(), ViewerClientError>(())
        }
    });
    let server_error = use_memo(move || {
        action
            .value()
            .and_then(Result::err)
            .as_ref()
            .and_then(captured_client_error)
            .cloned()
    });
    let submit = use_callback(move |()| {
        if action.pending() {
            return;
        }
        errors_revealed.set(true);
        let Some(requests) = draft.peek().plan(&baseline.peek()).ok().and_then(|plan| {
            plan.requests(&saved_branch.peek(), loaded_settings.peek().as_ref(), &name)
        }) else {
            return;
        };
        if requests.is_empty() {
            onsaved.call(());
        } else {
            action.call(requests);
        }
    });
    ProjectEdit {
        draft: draft.into(),
        baseline,
        dirty: use_memo(move || draft().differs_from(&baseline())),
        settings_error: push_preference.error,
        pending: use_memo(move || action.pending()),
        field_errors: use_memo(move || {
            if errors_revealed()
                && let Err(errors) = draft().plan(&baseline())
            {
                return errors;
            }
            server_error()
                .as_ref()
                .and_then(ViewerClientError::failure)
                .and_then(FieldErrors::from_failure)
                .unwrap_or_default()
        }),
        form_error: use_memo(move || {
            server_error().filter(|error| {
                error
                    .failure()
                    .and_then(FieldErrors::<ProjectEditField>::from_failure)
                    .is_none()
            })
        }),
        change_branch: use_callback(move |value| {
            draft.write().comparison_branch = value;
            action.reset();
        }),
        reveal_errors: use_callback(move |()| errors_revealed.set(true)),
        change_push: use_callback(move |value| {
            draft.write().push_without_confirmation = Some(value);
            action.reset();
        }),
        submit,
        reload_settings: use_callback(move |()| {
            action.reset();
            settings.restart();
        }),
    }
}

async fn save_project_edit(
    path: gtl_models::paths::RepositoryRoot,
    requests: ProjectEditRequests,
    mut saved_branch: Signal<ComparisonBranch>,
    mut settings: Resource<Result<ViewerUserSettings, ViewerClientError>>,
) -> Result<(), ViewerClientError> {
    if let Some(change) = requests.comparison_branch {
        viewer_server::update_project(UpdateViewerProject {
            path,
            comparison_branch: FieldUpdate::Update(change.branch.clone()),
            expected_comparison_branch: change.expected,
        })
        .await?;
        saved_branch.set(change.branch);
    }
    if let Some(request) = requests.settings {
        let result = viewer_server::edit_settings(request).await;
        settings.restart();
        result?;
    }
    Ok(())
}

/// Edits `project` and asks before discarding unsaved changes on any exit except a save.
///
/// While `open` is false the dialog hides and keeps its draft.
#[component]
pub(super) fn ProjectEditDialog(
    project: ViewerProject,
    open: bool,
    onclose: EventHandler<()>,
    onclosed: EventHandler<()>,
    onsnapshots: EventHandler<()>,
) -> Element {
    let language = use_language();
    let edit = use_project_edit(&project, onclose);
    let confirmation = use_unsaved_changes_confirmation(edit.dirty.into());
    let close = use_callback(move |()| (confirmation.request_confirmation)(onclose));
    let open_snapshots = use_callback(move |()| (confirmation.request_confirmation)(onsnapshots));
    let name = project.name.to_string();
    rsx! {
        PanelDialog {
            id: "project-edit-dialog",
            trigger_id: super::project_edit_trigger_id(&project),
            title: t!(language, "projects-edit-title", project = name.clone()),
            variant: PanelDialogVariant::Form,
            open,
            onclose: close,
            onclosed,
            ProjectEditForm { edit, oncancel: close, onsnapshots: open_snapshots }
        }
        AlertDialog {
            id: "project-edit-discard-dialog",
            trigger_id: "project-edit-branch",
            open: open && (confirmation.open)(),
            title: t!(language, "projects-edit-discard-title"),
            description: t!(language, "projects-edit-discard-description", project = name),
            confirm_label: t!(language, "projects-edit-discard"),
            onconfirm: confirmation.confirm_leave,
            oncancel: confirmation.cancel_leave,
        }
    }
}

#[component]
fn ProjectEditForm(
    edit: ProjectEdit,
    oncancel: EventHandler<()>,
    onsnapshots: EventHandler<()>,
) -> Element {
    let language = use_language();
    let draft = (edit.draft)();
    let baseline = (edit.baseline)();
    let pending = (edit.pending)();
    let settings_error = (edit.settings_error)();
    let push_known = baseline.push_without_confirmation.is_some();
    let push_checked = draft
        .push_without_confirmation
        .or(baseline.push_without_confirmation)
        .unwrap_or(false);
    let save_state = if pending {
        ButtonState::Loading
    } else if (edit.dirty)() {
        ButtonState::Enabled
    } else {
        ButtonState::Disabled
    };
    rsx! {
        form {
            class: "grid gap-5",
            novalidate: true,
            aria_busy: pending.then_some("true"),
            onsubmit: move |event| {
                event.prevent_default();
                (edit.submit)(());
            },
            TextInput {
                id: "project-edit-branch",
                label: t!(language, "projects-comparison-branch"),
                value: draft.comparison_branch,
                readonly: pending,
                required: true,
                maxlength: "1024",
                autocomplete: "off",
                "data-dialog-content-initial-focus": "true",
                error: (edit.field_errors)().message(ProjectEditField::ComparisonBranch, language),
                oninput: move |event: FormEvent| (edit.change_branch)(event.value()),
                onchange: move |_| (edit.reveal_errors)(()),
                supporting_content: rsx! {
                    {t!(language, "projects-comparison-branch-hint")}
                },
            }
            div { class: "grid gap-2",
                Checkbox {
                    id: "project-edit-push-no-confirmation",
                    label: t!(language, "projects-viewer-push-no-confirmation"),
                    hint: t!(language, "projects-viewer-push-no-confirmation-hint"),
                    checked: push_checked,
                    disabled: pending || !push_known,
                    error: settings_error.as_ref().map(|error| client_error_message(error, language)),
                    onchange: move |value| (edit.change_push)(value),
                }
                if settings_error.is_some() {
                    Button {
                        class: "justify-self-start",
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Bare,
                        onclick: move |_| (edit.reload_settings)(()),
                        {t!(language, "settings-reload")}
                    }
                }
            }
            if let Some(error) = (edit.form_error)() {
                p { class: "text-sm break-words text-del", role: "alert",
                    {client_error_message(&error, language)}
                }
            }
            div { class: "flex flex-wrap items-center justify-between gap-3 border-t border-line pt-4",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Ghost,
                    aria_haspopup: "dialog",
                    icon: rsx! {
                        History { size: 15 }
                    },
                    onclick: move |_| onsnapshots.call(()),
                    {t!(language, "projects-snapshot-history")}
                }
                div { class: "ml-auto flex items-center gap-2",
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        onclick: move |_| oncancel.call(()),
                        {t!(language, "dialog-cancel")}
                    }
                    Button {
                        button_type: ButtonType::Submit,
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Primary,
                        state: save_state,
                        {t!(language, "projects-edit-save")}
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
