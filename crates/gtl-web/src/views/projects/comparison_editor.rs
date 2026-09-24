use std::fmt::Write as _;

use dioxus::prelude::*;
use gtl_models::{paths::RepositoryRoot, projects::comparison::ComparisonBranch};
use gtl_wire::viewer::{
    FieldUpdate,
    projects::{UpdateViewerProject, ViewerProject},
};
use lucide_dioxus::{GitCompare, Settings2};

use super::loading::Projects;
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        field_errors::{FieldErrors, FormField},
        ui::{
            Button, ButtonSize, ButtonState, ButtonType, ButtonVariant, IconPopover, TextInput,
            popover::{PopoverPlacement, PopoverSurface},
        },
        viewer_client::{ViewerClientError, captured_client_error},
    },
};

pub(super) fn comparison_popover_id(path: &RepositoryRoot) -> String {
    let mut id = String::from("project-comparison");
    for character in path.to_string().chars() {
        if character.is_ascii_alphanumeric() {
            id.push(character);
        } else {
            let _ = write!(id, "-{}-", u32::from(character));
        }
    }
    id
}

/// The comparison editor's single input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparisonField {
    Branch,
}

impl FormField for ComparisonField {
    const ALL: &'static [Self] = &[Self::Branch];

    fn request_field(self) -> &'static str {
        "comparison_branch"
    }

    fn correction(self) -> &'static str {
        "Enter a local branch name, such as main or release/next."
    }
}

/// Opens the comparison branch editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ComparisonEditorTrigger {
    /// A settings icon among a project's actions.
    #[default]
    Icon,
    /// A labeled action beside a failure that a different branch resolves.
    Labeled,
}

/// Edits the comparison branch of the catalogued project at `project`.
///
/// While `active` is false the editor stays closed and discards its draft. `onsaved` runs after
/// the server stores the new branch.
#[component]
pub(crate) fn ComparisonBranchEditor(
    project: RepositoryRoot,
    branch: ComparisonBranch,
    #[props(default)] trigger: ComparisonEditorTrigger,
    #[props(default)] disabled: bool,
    #[props(default = true)] active: bool,
    onsaved: EventHandler<()>,
) -> Element {
    let popover_id = match trigger {
        ComparisonEditorTrigger::Icon => comparison_popover_id(&project),
        ComparisonEditorTrigger::Labeled => format!("{}-action", comparison_popover_id(&project)),
    };
    let label = format!("Comparison branch: {branch}");
    let form = rsx! {
        ComparisonBranchForm {
            project,
            branch,
            popover_id: popover_id.clone(),
            disabled,
            active,
            onsaved,
        }
    };
    match trigger {
        ComparisonEditorTrigger::Icon => rsx! {
            IconPopover {
                id: popover_id,
                aria_label: label,
                placement: PopoverPlacement::TriggerEnd,
                icon: rsx! {
                    Settings2 { size: 15 }
                },
                {form}
            }
        },
        ComparisonEditorTrigger::Labeled => rsx! {
            span { class: "inline-flex",
                Button {
                    id: "{popover_id}-trigger",
                    variant: ButtonVariant::Outline,
                    popovertarget: popover_id.clone(),
                    popovertargetaction: "toggle",
                    aria_controls: popover_id.clone(),
                    disabled,
                    icon: rsx! {
                        GitCompare { size: 15 }
                    },
                    "Change comparison branch"
                }
                PopoverSurface {
                    id: popover_id,
                    placement: PopoverPlacement::TriggerEnd,
                    role: "group",
                    aria_label: label,
                    {form}
                }
            }
        },
    }
}

/// The comparison editor for a project row, closed while Projects is inactive.
#[component]
pub(super) fn ProjectComparisonEditor(project: ViewerProject, disabled: bool) -> Element {
    let projects = use_context::<Projects>();
    rsx! {
        ComparisonBranchEditor {
            project: project.path,
            branch: project.comparison_branch,
            disabled,
            active: (projects.active)(),
            onsaved: move |()| (projects.refresh)(()),
        }
    }
}

struct ComparisonBranchEdit {
    draft: ReadSignal<Option<String>>,
    pending: Memo<bool>,
    field_errors: Memo<FieldErrors<ComparisonField>>,
    form_error: Memo<Option<String>>,
    change: Callback<String>,
    save: Callback<()>,
}

fn use_comparison_branch_edit(
    project: &RepositoryRoot,
    branch: &ComparisonBranch,
    popover_id: &str,
    active: bool,
    onsaved: EventHandler<()>,
) -> ComparisonBranchEdit {
    let mut draft = use_signal(|| None::<String>);
    let mut parse_errors = use_signal(FieldErrors::<ComparisonField>::default);
    let popover_id = popover_id.to_owned();
    use_effect(use_reactive(
        (&popover_id, &active),
        move |(popover_id, active)| {
            if !active {
                browser::hide_popover(&popover_id);
                draft.set(None);
                parse_errors.set(FieldErrors::default());
            }
        },
    ));
    let mut action = use_action(move |request: UpdateViewerProject| {
        let popover_id = popover_id.clone();
        async move {
            viewer_server::update_project(request).await?;
            draft.set(None);
            browser::hide_popover(&popover_id);
            onsaved.call(());
            Ok::<(), ViewerClientError>(())
        }
    });
    let change = use_callback(move |value| {
        draft.set(Some(value));
        parse_errors.set(FieldErrors::default());
        action.reset();
    });
    let path = project.clone();
    let previous = branch.clone();
    let save = use_callback(move |()| {
        let Some(value) = draft.peek().clone() else {
            return;
        };
        let mut errors = FieldErrors::default();
        let branch = errors.parse(ComparisonField::Branch, ComparisonBranch::try_new(value));
        parse_errors.set(errors);
        if let Some(branch) = branch {
            action.call(UpdateViewerProject {
                path: path.clone(),
                comparison_branch: FieldUpdate::Update(branch),
                expected_comparison_branch: previous.clone(),
            });
        }
    });
    let server_error = use_memo(move || action.value().and_then(Result::err));
    ComparisonBranchEdit {
        draft: draft.into(),
        pending: use_memo(move || action.pending()),
        field_errors: use_memo(move || {
            let parsed = parse_errors();
            if !parsed.is_empty() {
                return parsed;
            }
            server_error()
                .as_ref()
                .and_then(captured_client_error)
                .and_then(ViewerClientError::failure)
                .and_then(FieldErrors::from_failure)
                .unwrap_or_default()
        }),
        form_error: use_memo(move || {
            server_error()
                .filter(|error| {
                    captured_client_error(error)
                        .and_then(ViewerClientError::failure)
                        .and_then(FieldErrors::<ComparisonField>::from_failure)
                        .is_none()
                })
                .map(|error| error.to_string())
        }),
        change,
        save,
    }
}

#[component]
fn ComparisonBranchForm(
    project: RepositoryRoot,
    branch: ComparisonBranch,
    popover_id: String,
    disabled: bool,
    active: bool,
    onsaved: EventHandler<()>,
) -> Element {
    let edit = use_comparison_branch_edit(&project, &branch, &popover_id, active, onsaved);
    let value = (edit.draft)().unwrap_or_else(|| branch.to_string());
    let pending = (edit.pending)();
    let form_error = (edit.form_error)();
    rsx! {
        form {
            class: "grid gap-3 p-3",
            onsubmit: move |event| {
                event.prevent_default();
                (edit.save)(());
            },
            TextInput {
                id: "{popover_id}-branch",
                label: "Comparison branch",
                value,
                disabled: disabled || pending,
                maxlength: "1024",
                error: (edit.field_errors)().message(ComparisonField::Branch),
                oninput: move |event: FormEvent| (edit.change)(event.value()),
                supporting_content: rsx! { "Used when the current branch has no upstream." },
            }
            if let Some(error) = form_error {
                p { class: "text-xs break-words text-del", role: "alert", "{error}" }
            }
            div { class: "flex justify-end",
                Button {
                    button_type: ButtonType::Submit,
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    state: if pending { ButtonState::Loading } else if disabled || (edit.draft)().is_none() { ButtonState::Disabled } else { ButtonState::Enabled },
                    "Save comparison"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
