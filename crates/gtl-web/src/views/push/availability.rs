use dioxus::prelude::*;
use gtl_wire::viewer::{
    ViewerViewIdentity,
    push::{CreateViewerPush, ViewerPushAvailability},
};

use super::{PushButton, PushController};
use crate::entities::diffs::viewer_server;

#[component]
pub(crate) fn ViewPushButton(
    identity: ReadSignal<ViewerViewIdentity>,
    disabled: ReadSignal<bool>,
) -> Element {
    let controller = use_context::<PushController>();
    let key = use_memo(move || {
        (
            identity(),
            (controller.refresh_epoch)(),
            controller.viewer.server_instance_id(),
            !disabled() && controller.viewer.actions_enabled(),
        )
    });
    let availability = use_resource(move || async move {
        let key = key();
        let result = if key.3 {
            Some(viewer_server::get_push_availability(key.0).await)
        } else {
            None
        };
        (key, result)
    });
    let current = use_memo(move || {
        let current_key = key();
        availability
            .read()
            .as_ref()
            .filter(|(completed_key, _)| *completed_key == current_key)
            .and_then(|(_, result)| result.clone())
    });
    let title = match current() {
        Some(Ok(ViewerPushAvailability::Available)) => "Push".to_owned(),
        Some(Ok(ViewerPushAvailability::NothingToPush)) => {
            "No unpushed commits through this SHA".to_owned()
        }
        Some(Ok(ViewerPushAvailability::Blocked { failure })) => failure.to_string(),
        Some(Err(error)) => error.to_string(),
        None => "Checking for unpushed commits".to_owned(),
    };
    rsx! {
        PushButton {
            id: "viewer-push-trigger",
            source: CreateViewerPush::View {
                identity: identity(),
            },
            disabled: !matches!(current(), Some(Ok(ViewerPushAvailability::Available))),
            title,
        }
    }
}
