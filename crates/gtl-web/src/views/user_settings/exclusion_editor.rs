use dioxus::prelude::*;
use gtl_models::diffs::ExcludedExtensions;
use gtl_wire::viewer::{FieldUpdate, file_filters::UpdateDiffExclusions};

use crate::{
    entities::diffs::viewer_server,
    shared::{
        ui::{
            Button, ButtonSize, ButtonState, ButtonVariant, ExtensionExclusionsAction,
            ExtensionExclusionsInput, use_toast,
        },
        viewer_client::ViewerClientError,
    },
};

#[component]
pub(super) fn ExclusionEditor(
    configured: ExcludedExtensions,
    onchanged: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(|| None::<ExcludedExtensions>);
    let mut failure = use_signal(|| None::<ViewerClientError>);
    let toast = use_toast();
    let mut save = use_action(move |request: UpdateDiffExclusions| async move {
        match viewer_server::update_diff_exclusions(request).await {
            Ok(()) => {
                failure.set(None);
                onchanged.call(());
                toast.ok("Exclusions saved");
            }
            Err(error) => {
                failure.set(Some(error));
                toast.error(super::settings_edit_error_message(error));
            }
        }
        Ok::<(), std::convert::Infallible>(())
    });
    let selected = draft().unwrap_or_else(|| configured.clone());
    let pending = save.pending();
    let changed = selected != configured;
    let save_failure = failure();
    let save_error = save_failure.map(super::settings_edit_error_message);
    let selection_for_change = selected.clone();
    let selection_for_save = selected.clone();
    rsx! {
        div { class: "grid gap-3 px-4 py-4",
            ExtensionExclusionsInput {
                id: "settings-default-exclusion",
                excluded: selected,
                available: Vec::new(),
                disabled: pending,
                onchange: move |action: ExtensionExclusionsAction| {
                    failure.set(None);
                    draft.set(Some(action.apply(&selection_for_change)));
                },
            }
            if let Some(error) = save_error {
                div { class: "flex flex-wrap items-center gap-2",
                    p {
                        class: "min-w-0 flex-1 text-sm text-del",
                        role: "alert",
                        "{error}"
                    }
                    if save_failure == Some(ViewerClientError::Conflict) {
                        Button {
                            size: ButtonSize::Small,
                            variant: ButtonVariant::Outline,
                            disabled: pending,
                            onclick: move |_| {
                                failure.set(None);
                                onchanged.call(());
                            },
                            "Reload exclusions"
                        }
                    }
                }
            }
            div { class: "flex justify-end",
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    state: if pending { ButtonState::Loading } else if changed { ButtonState::Enabled } else { ButtonState::Disabled },
                    onclick: move |_| {
                        failure.set(None);
                        save.call(UpdateDiffExclusions {
                            project: None,
                            extensions: FieldUpdate::Update(selection_for_save.clone()),
                            expected: Some(configured.clone()),
                        });
                    },
                    "Save"
                }
            }
        }
    }
}
