use std::collections::HashSet;

use dioxus::{core::spawn_forever, prelude::*};
use gtl_models::viewer::ViewerKeybindingAction;
use gtl_wire::diff_review::{DiffFileReview, DiffFileReviewReference, SetDiffFileReviewed};
use lucide_dioxus::Check;

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{
        i18n::{t, use_language},
        keyboard::shortcut_title,
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, ToastHandle, use_toast},
    },
};

#[derive(Clone, Copy)]
pub(crate) struct FileReviewController {
    pending: ReadSignal<HashSet<DiffFileReviewReference>>,
    submit: Callback<DiffFileReview>,
}

pub(crate) fn use_file_reviews_provider() {
    let mut pending = use_signal(HashSet::new);
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let submit = use_callback(move |review: DiffFileReview| {
        if !pending.write().insert(review.reference.clone()) {
            return;
        }
        spawn_forever(submit_review(review, pending, viewer, toast));
    });
    use_context_provider(|| FileReviewController {
        pending: pending.into(),
        submit,
    });
}

async fn submit_review(
    review: DiffFileReview,
    mut pending: Signal<HashSet<DiffFileReviewReference>>,
    viewer: ViewerContext,
    toast: ToastHandle,
) {
    let request = SetDiffFileReviewed {
        file: review.reference.clone(),
        reviewed: !review.reviewed,
    };
    let result = async {
        viewer_server::set_diff_file_reviewed(request).await?;
        viewer_server::get_shell().await
    }
    .await;
    match result {
        Ok(shell) => viewer.replace_shell(shell),
        Err(error) => {
            viewer.refresh(false);
            toast.client_error(&error);
        }
    }
    pending.write().remove(&review.reference);
}

impl FileReviewController {
    pub(crate) fn toggle(self, review: DiffFileReview) {
        self.submit.call(review);
    }
}

#[component]
pub(crate) fn FileReviewAction(review: DiffFileReview) -> Element {
    let viewer = use_context::<ViewerContext>();
    let controller = try_use_context::<FileReviewController>();
    let pending =
        controller.is_some_and(|controller| controller.pending.read().contains(&review.reference));
    let language = use_language();
    let keybindings = crate::app::user_settings::use_viewer_keybindings();
    let reviewed = review.reviewed;
    let enabled = viewer.actions_enabled() && controller.is_some();
    let label = if review.reviewed {
        t!(language, "review-mark-unreviewed")
    } else {
        t!(language, "review-mark-reviewed")
    };
    rsx! {
        Button {
            class: "diff-file-review-action mobile:size-11",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            state: if pending { ButtonState::Loading } else if enabled { ButtonState::Enabled } else { ButtonState::Disabled },
            aria_label: t!(language, "review-file-reviewed"),
            title: shortcut_title(&label, keybindings, ViewerKeybindingAction::ToggleFileReviewed),
            aria_keyshortcuts: keybindings.aria_keyshortcuts(ViewerKeybindingAction::ToggleFileReviewed),
            aria_pressed: reviewed.to_string(),
            "data-reviewed": reviewed.to_string(),
            onclick: move |event: MouseEvent| {
                event.stop_propagation();
                event.prevent_default();
                if let Some(controller) = controller {
                    controller.toggle(review.clone());
                }
            },
            icon: rsx! {
                span { aria_hidden: "true",
                    if reviewed {
                        Check { size: 16 }
                    } else {
                        span { class: "diff-file-review-circle" }
                    }
                }
            },
        }
    }
}
