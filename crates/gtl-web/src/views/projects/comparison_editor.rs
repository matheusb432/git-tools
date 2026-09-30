use std::fmt::Write as _;

use dioxus::prelude::*;
use gtl_models::{paths::RepositoryRoot, projects::comparison::ComparisonBranch};
use gtl_wire::viewer::{FieldUpdate, projects::UpdateViewerProject};
use lucide_dioxus::GitCompare;

use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::client_error_message,
        field_errors::{FieldErrors, FormField},
        i18n::{t, use_language},
        ui::{
            Button, ButtonSize, ButtonState, ButtonType, ButtonVariant, TextInput,
            popover::{PopoverPlacement, PopoverSurface},
        },
        viewer_client::{ViewerClientError, captured_client_error},
    },
};

fn comparison_popover_id(path: &RepositoryRoot) -> String {
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

    fn correction(self, language: gtl_models::settings::ViewerLanguage) -> String {
        t!(language, "projects-comparison-branch-correction")
    }
}

/// Edits the comparison branch of the catalogued project at `project` from a labeled action.
///
/// While `active` is false the editor stays closed and discards its draft. `onsaved` runs after
/// the server stores the new branch.
#[component]
pub(crate) fn ComparisonBranchEditor(
    project: RepositoryRoot,
    branch: ComparisonBranch,
    #[props(default)] disabled: bool,
    #[props(default = true)] active: bool,
    onsaved: EventHandler<()>,
) -> Element {
    let popover_id = format!("{}-action", comparison_popover_id(&project));
    let language = use_language();
    let label = t!(
        language,
        "projects-comparison-branch-value",
        branch = branch.to_string()
    );
    rsx! {
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
                {t!(language, "projects-change-comparison-branch")}
            }
            PopoverSurface {
                id: popover_id.clone(),
                placement: PopoverPlacement::TriggerEnd,
                role: "group",
                aria_label: label,
                ComparisonBranchForm {
                    project,
                    branch,
                    popover_id,
                    disabled,
                    active,
                    onsaved,
                }
            }
        }
    }
}

struct ComparisonBranchEdit {
    draft: ReadSignal<Option<String>>,
    pending: Memo<bool>,
    field_errors: Memo<FieldErrors<ComparisonField>>,
    form_error: Memo<Option<ViewerClientError>>,
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
                .as_ref()
                .and_then(captured_client_error)
                .filter(|error| {
                    error
                        .failure()
                        .and_then(FieldErrors::<ComparisonField>::from_failure)
                        .is_none()
                })
                .cloned()
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
    let language = use_language();
    let edit = use_comparison_branch_edit(&project, &branch, &popover_id, active, onsaved);
    let value = (edit.draft)().unwrap_or_else(|| branch.to_string());
    let pending = (edit.pending)();
    let form_error = (edit.form_error)();
    rsx! {
        form {
            class: "grid w-full min-w-0 gap-3 p-3",
            novalidate: true,
            onsubmit: move |event| {
                event.prevent_default();
                (edit.save)(());
            },
            TextInput {
                id: "{popover_id}-branch",
                label: t!(language, "projects-comparison-branch"),
                value,
                disabled: disabled || pending,
                maxlength: "1024",
                error: (edit.field_errors)().message(ComparisonField::Branch, language),
                oninput: move |event: FormEvent| (edit.change)(event.value()),
                supporting_content: rsx! {
                    {t!(language, "projects-comparison-branch-hint")}
                },
            }
            if let Some(error) = form_error {
                p { class: "text-xs break-words text-del", role: "alert",
                    {client_error_message(&error, language)}
                }
            }
            div { class: "flex justify-end",
                Button {
                    button_type: ButtonType::Submit,
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    state: if pending { ButtonState::Loading } else if disabled || (edit.draft)().is_none() { ButtonState::Disabled } else { ButtonState::Enabled },
                    {t!(language, "projects-comparison-save")}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
