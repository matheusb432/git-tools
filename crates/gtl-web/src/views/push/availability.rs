use dioxus::prelude::*;
use gtl_wire::viewer::{
    ViewerActiveState, ViewerViewIdentity,
    push::{CreateViewerPush, ViewerPushAvailability, ViewerPushState},
};

use super::{PushButton, PushController};
use crate::{
    app::application_layout::ViewerShellLoad,
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_message::failure_message,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone, PartialEq, Eq)]
struct ViewPushStatusKey {
    identity: ViewerViewIdentity,
    refresh_epoch: u64,
    server_instance_id: String,
}

#[derive(Clone)]
struct ViewPushStatus {
    key: ViewPushStatusKey,
    result: Result<ViewerPushState, ViewerClientError>,
}

impl ViewPushStatus {
    fn current(
        &self,
        key: &ViewPushStatusKey,
        identity: ViewerViewIdentity,
    ) -> Option<&Result<ViewerPushState, ViewerClientError>> {
        (self.key == *key && key.identity == identity).then_some(&self.result)
    }
}

#[derive(Default)]
enum ViewSnapshotPushStatus {
    #[default]
    Unknown,
    Known {
        key: ViewPushStatusKey,
        unpushed: bool,
    },
}

impl ViewSnapshotPushStatus {
    fn observe(&mut self, status: &ViewPushStatus) {
        let Some(unpushed) = status
            .result
            .as_ref()
            .ok()
            .and_then(|state| state.snapshot_has_unpushed_commits)
        else {
            return;
        };
        *self = Self::Known {
            key: status.key.clone(),
            unpushed,
        };
    }

    fn current(&self, identity: ViewerViewIdentity, server_instance_id: &str) -> Option<bool> {
        let Self::Known { key, unpushed } = self else {
            return None;
        };
        (key.server_instance_id == server_instance_id
            && key.identity.tab_id == identity.tab_id
            && key.identity.range_generation == identity.range_generation)
            .then_some(*unpushed)
    }
}

#[derive(Clone, Copy)]
struct ViewPushStatusContext {
    key: Memo<Option<ViewPushStatusKey>>,
    status: Resource<Option<ViewPushStatus>>,
    snapshot: ReadSignal<ViewSnapshotPushStatus>,
}

pub(super) fn use_view_push_status_provider() {
    let controller = use_context::<PushController>();
    let key = use_memo(move || {
        if !controller.viewer.actions_enabled() {
            return None;
        }
        let server_instance_id = controller.viewer.server_instance_id()?;
        let identity = controller.viewer.shell().with(|shell| match shell {
            ViewerShellLoad::Ready(shell) => match &shell.active {
                ViewerActiveState::Ready { view } => Some(view.identity),
                _ => None,
            },
            _ => None,
        })?;
        Some(ViewPushStatusKey {
            identity,
            refresh_epoch: (controller.refresh_epoch)(),
            server_instance_id,
        })
    });
    let mut snapshot = use_signal(ViewSnapshotPushStatus::default);
    let status = use_resource(move || async move {
        let key = key()?;
        // Commit the tab's UI before starting optional Git work. Rows load independently.
        browser::yield_to_paint().await;
        let result = viewer_server::get_push_availability(key.identity).await;
        let status = ViewPushStatus { key, result };
        snapshot.write().observe(&status);
        Some(status)
    });
    use_context_provider(|| ViewPushStatusContext {
        key,
        status,
        snapshot: snapshot.into(),
    });
}

#[component]
pub(crate) fn ViewPushButton(
    id: String,
    identity: ReadSignal<ViewerViewIdentity>,
    disabled: ReadSignal<bool>,
) -> Element {
    let (available, title, _) = use_view_push_availability(identity, disabled);
    rsx! {
        PushButton {
            id,
            icon_only: true,
            source: CreateViewerPush::View {
                identity: identity(),
            },
            disabled: !available,
            title: Some(title),
        }
    }
}

pub(crate) fn use_view_push_availability(
    identity: ReadSignal<ViewerViewIdentity>,
    disabled: ReadSignal<bool>,
) -> (bool, String, Option<bool>) {
    let language = use_language();
    let controller = use_context::<PushController>();
    let status = use_context::<ViewPushStatusContext>();
    let current_key = (status.key)();
    let loaded = status.status.read();
    let loaded = loaded.as_ref().and_then(Option::as_ref);
    let current = current_key
        .as_ref()
        .zip(loaded)
        .and_then(|(key, status)| status.current(key, identity()));
    let snapshot_has_unpushed_commits = controller
        .viewer
        .server_instance_id()
        .and_then(|server| status.snapshot.read().current(identity(), &server));
    let title = match current.map(|result| result.as_ref().map(|state| &state.availability)) {
        Some(Ok(ViewerPushAvailability::Available)) | None => t!(language, "push-button"),
        Some(Ok(ViewerPushAvailability::NothingToPush)) => {
            t!(language, "push-availability-nothing")
        }
        Some(Ok(ViewerPushAvailability::Blocked { failure })) => failure_message(failure, language),
        Some(Err(error)) => client_error_message(error, language),
    };
    (
        !disabled()
            && controller.viewer.actions_enabled()
            && current
                .is_none_or(|result| matches!(result, Ok(state) if state.availability == ViewerPushAvailability::Available)),
        title,
        snapshot_has_unpushed_commits,
    )
}

#[component]
pub(crate) fn ViewPushMenuAction(
    identity: ReadSignal<ViewerViewIdentity>,
    disabled: ReadSignal<bool>,
    menu_id: String,
    trigger_id: String,
) -> Element {
    let (available, title, _) = use_view_push_availability(identity, disabled);
    rsx! {
        super::controller::PushMenuAction {
            source: CreateViewerPush::View {
                identity: identity(),
            },
            disabled: !available,
            title,
            menu_id,
            trigger_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration};

    use super::*;
    use crate::test_support::{TestResult, viewer_active_view, viewer_tab_id};

    fn status(identity: ViewerViewIdentity) -> ViewPushStatus {
        ViewPushStatus {
            key: ViewPushStatusKey {
                identity,
                refresh_epoch: 0,
                server_instance_id: "server".to_owned(),
            },
            result: Ok(ViewerPushState {
                availability: ViewerPushAvailability::Available,
                snapshot_has_unpushed_commits: Some(true),
            }),
        }
    }

    #[test]
    fn background_results_cannot_update_another_tab_or_generation() -> TestResult {
        let identity = viewer_active_view(viewer_tab_id(1)?).identity;
        let loaded = status(identity);
        let mut snapshot = ViewSnapshotPushStatus::default();
        snapshot.observe(&loaded);
        assert!(loaded.current(&loaded.key, identity).is_some());
        for next_identity in [
            ViewerViewIdentity {
                tab_id: viewer_tab_id(2)?,
                ..identity
            },
            ViewerViewIdentity {
                range_generation: ViewerRangeGeneration::new(2),
                ..identity
            },
        ] {
            let key = ViewPushStatusKey {
                identity: next_identity,
                ..loaded.key.clone()
            };
            assert!(loaded.current(&key, next_identity).is_none());
            assert_eq!(
                snapshot.current(next_identity, &key.server_instance_id),
                None
            );
        }
        let key = ViewPushStatusKey {
            server_instance_id: "replacement".to_owned(),
            ..loaded.key.clone()
        };
        assert!(loaded.current(&key, identity).is_none());
        assert_eq!(snapshot.current(identity, &key.server_instance_id), None);
        Ok(())
    }

    #[test]
    fn snapshot_status_survives_commit_selection_while_eligibility_loads() -> TestResult {
        let identity = viewer_active_view(viewer_tab_id(1)?).identity;
        let loaded = status(identity);
        let mut snapshot = ViewSnapshotPushStatus::default();
        snapshot.observe(&loaded);
        let selected = ViewerViewIdentity {
            selection_generation: ViewerSelectionGeneration::new(2),
            ..identity
        };
        let key = ViewPushStatusKey {
            identity: selected,
            ..loaded.key.clone()
        };
        assert!(loaded.current(&key, selected).is_none());
        assert_eq!(
            snapshot.current(selected, &key.server_instance_id),
            Some(true)
        );
        Ok(())
    }

    #[test]
    fn unknown_results_retain_snapshot_status_until_a_confirmed_change() -> TestResult {
        let identity = viewer_active_view(viewer_tab_id(1)?).identity;
        let mut snapshot = ViewSnapshotPushStatus::default();
        assert_eq!(snapshot.current(identity, "server"), None);
        let mut loaded = status(identity);
        snapshot.observe(&loaded);
        loaded.key.refresh_epoch += 1;
        loaded.result = Err(ViewerClientError::Disconnected);
        snapshot.observe(&loaded);
        assert_eq!(snapshot.current(identity, "server"), Some(true));
        loaded.result = Ok(ViewerPushState {
            availability: ViewerPushAvailability::Available,
            snapshot_has_unpushed_commits: None,
        });
        snapshot.observe(&loaded);
        assert_eq!(snapshot.current(identity, "server"), Some(true));
        loaded.result = Ok(ViewerPushState {
            availability: ViewerPushAvailability::NothingToPush,
            snapshot_has_unpushed_commits: Some(false),
        });
        snapshot.observe(&loaded);
        assert_eq!(snapshot.current(identity, "server"), Some(false));
        Ok(())
    }
}
