use std::fmt::Write as _;

use dioxus::prelude::*;
use gtl_models::paths::RepositoryRoot;
use gtl_wire::viewer::{
    FieldUpdate,
    projects::{UpdateViewerProject, ViewerProject},
};
use lucide_dioxus::Settings2;

use super::loading::Projects;
use crate::{
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{
            Button, ButtonSize, ButtonState, ButtonType, ButtonVariant, IconPopover, TextInput,
            popover::PopoverPlacement,
        },
        viewer_client::ViewerClientError,
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

struct ProjectComparisonEdit {
    draft: ReadSignal<Option<String>>,
    pending: Memo<bool>,
    error: Memo<Option<String>>,
    change: Callback<String>,
    save: Callback<()>,
}

fn use_project_comparison_edit(project: &ViewerProject, popover_id: &str) -> ProjectComparisonEdit {
    let projects = use_context::<Projects>();
    let mut draft = use_signal(|| None::<String>);
    let mut validation_error = use_signal(|| None::<String>);
    let popover_id = popover_id.to_owned();
    use_effect(use_reactive((&popover_id,), move |(popover_id,)| {
        if !(projects.active)() {
            browser::hide_popover(&popover_id);
            draft.set(None);
            validation_error.set(None);
        }
    }));
    let mut action = use_action(move |request: UpdateViewerProject| {
        let popover_id = popover_id.clone();
        async move {
            viewer_server::update_project(request).await?;
            draft.set(None);
            browser::hide_popover(&popover_id);
            (projects.refresh)(());
            Ok::<(), ViewerClientError>(())
        }
    });
    let change = use_callback(move |value| {
        draft.set(Some(value));
        validation_error.set(None);
    });
    let path = project.path.clone();
    let previous = project.comparison_branch.clone();
    let save = use_callback(move |()| {
        let Some(value) = draft.peek().clone() else {
            return;
        };
        match gtl_models::projects::comparison::ComparisonBranch::try_new(value) {
            Ok(branch) => {
                validation_error.set(None);
                action.call(UpdateViewerProject {
                    path: path.clone(),
                    comparison_branch: FieldUpdate::Update(branch),
                    expected_comparison_branch: previous.clone(),
                });
            }
            Err(_) => validation_error.set(Some(
                "Enter a local branch name, such as main or release/next.".to_owned(),
            )),
        }
    });
    ProjectComparisonEdit {
        draft: draft.into(),
        pending: use_memo(move || action.pending()),
        error: use_memo(move || {
            validation_error().or_else(|| {
                action
                    .value()
                    .and_then(Result::err)
                    .map(|error| error.to_string())
            })
        }),
        change,
        save,
    }
}

#[component]
pub(super) fn ProjectComparisonEditor(project: ViewerProject, disabled: bool) -> Element {
    let popover_id = comparison_popover_id(&project.path);
    let edit = use_project_comparison_edit(&project, &popover_id);
    let value = (edit.draft)().unwrap_or_else(|| project.comparison_branch.to_string());
    let pending = (edit.pending)();
    let error = (edit.error)();
    rsx! {
        IconPopover {
            id: popover_id,
            aria_label: "Comparison branch: {project.comparison_branch}",
            placement: PopoverPlacement::TriggerEnd,
            icon: rsx! {
                Settings2 { size: 15 }
            },
            form {
                class: "grid gap-3 p-3",
                onsubmit: move |event| {
                    event.prevent_default();
                    (edit.save)(());
                },
                TextInput {
                    label: "Comparison branch",
                    value,
                    disabled: disabled || pending,
                    maxlength: "1024",
                    aria_invalid: error.is_some().to_string(),
                    oninput: move |event: FormEvent| (edit.change)(event.value()),
                    supporting_content: rsx! { "Used when the current branch has no upstream." },
                }
                if let Some(error) = error {
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
}

#[cfg(test)]
mod tests;
