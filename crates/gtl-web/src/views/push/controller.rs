use std::{collections::HashMap, time::Duration};

use dioxus::{core::spawn_forever, prelude::*};
use gtl_models::{
    failure::Failure, paths::RepositoryRoot, settings::ViewerLanguage, viewer::ViewerTabId,
};
use gtl_wire::viewer::push::{
    CreateViewerPush, ViewerPushPreview, ViewerPushRequest, ViewerPushStatus,
};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{
            AlertDialog, AlertDialogSize, Button, ButtonSize, ButtonState, ButtonVariant,
            ToastHandle, ToastText, use_toast,
        },
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone)]
enum PushPhase {
    Preparing,
    Review {
        request: ViewerPushRequest,
        preview: Box<ViewerPushPreview>,
    },
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum PushSourceKey {
    Project(RepositoryRoot),
    View(ViewerTabId),
}

impl From<&CreateViewerPush> for PushSourceKey {
    fn from(source: &CreateViewerPush) -> Self {
        match source {
            CreateViewerPush::Project { path } => Self::Project(path.clone()),
            CreateViewerPush::View { identity } => Self::View(identity.tab_id),
        }
    }
}

/// How a push attempt ended; reported once as a toast.
enum PushReport {
    Completed,
    Failed(Failure),
    Client(ViewerClientError),
    /// The server returned the push to review instead of running it.
    NotStarted,
}

#[derive(Clone)]
enum PushOperationPhase {
    Starting,
    Queued,
    Running,
    Unresolved(PushUnresolved),
}

/// Why the viewer cannot tell how a confirmed push ended.
#[derive(Clone)]
enum PushUnresolved {
    /// Reading the push status failed.
    StatusUnreadable(ViewerClientError),
    /// The push outlasted the viewer's status checks.
    StatusPending,
}

impl PushUnresolved {
    fn message(&self, language: ViewerLanguage) -> String {
        match self {
            Self::StatusUnreadable(error) => t!(
                language,
                "push-status-unreadable",
                error = client_error_message(error, language)
            ),
            Self::StatusPending => t!(language, "push-status-pending"),
        }
    }
}

#[derive(Clone)]
struct PushOperation {
    ticket: u64,
    instance: Option<String>,
    request: ViewerPushRequest,
    phase: PushOperationPhase,
}

#[derive(Clone)]
struct PushDialog {
    ticket: u64,
    instance: Option<String>,
    trigger: String,
    source: PushSourceKey,
    phase: PushPhase,
}

#[derive(Clone, Copy)]
pub(crate) struct PushController {
    dialog: Signal<Option<PushDialog>>,
    operations: Signal<HashMap<PushSourceKey, PushOperation>>,
    ticket: Signal<u64>,
    pub(crate) refresh_epoch: ReadSignal<u64>,
    refresh: Signal<u64>,
    pub(super) viewer: ViewerContext,
    toast: ToastHandle,
}

pub(crate) fn use_push_provider() {
    let dialog = use_signal(|| None);
    let operations = use_signal(HashMap::new);
    let ticket = use_signal(|| 0_u64);
    let refresh = use_signal(|| 0_u64);
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    use_context_provider(|| PushController {
        dialog,
        operations,
        ticket,
        refresh_epoch: refresh.into(),
        refresh,
        viewer,
        toast,
    });
}

impl PushController {
    fn current_dialog(self, ticket: u64) -> bool {
        self.dialog.peek().as_ref().is_some_and(|dialog| {
            dialog.ticket == ticket && dialog.instance == self.viewer.server_instance_id()
        })
    }

    fn current_operation(self, source: &PushSourceKey, ticket: u64) -> bool {
        self.operations.peek().get(source).is_some_and(|operation| {
            operation.ticket == ticket && operation.instance == self.viewer.server_instance_id()
        })
    }

    fn set_dialog_phase(mut self, ticket: u64, phase: PushPhase) {
        if self.current_dialog(ticket)
            && let Some(dialog) = self.dialog.write().as_mut()
        {
            dialog.phase = phase;
        }
    }

    fn activate(mut self, source: CreateViewerPush, trigger: String) {
        if !self.viewer.actions_enabled() || self.dialog.peek().is_some() {
            return;
        }
        let source_key = PushSourceKey::from(&source);
        let operation = self.operations.peek().get(&source_key).cloned();
        if let Some(operation) = operation {
            self.recheck_operation(source_key, &operation);
            return;
        }
        let ticket = *self.ticket.peek() + 1;
        self.ticket.set(ticket);
        self.dialog.set(Some(PushDialog {
            ticket,
            instance: self.viewer.server_instance_id(),
            trigger,
            source: source_key,
            phase: PushPhase::Preparing,
        }));
        spawn_forever(async move {
            prepare(&self, ticket, source).await;
        });
    }

    fn recheck_operation(mut self, source: PushSourceKey, operation: &PushOperation) {
        if operation.instance != self.viewer.server_instance_id()
            || !matches!(operation.phase, PushOperationPhase::Unresolved(_))
        {
            return;
        }
        if let Some(active) = self.operations.write().get_mut(&source) {
            active.phase = PushOperationPhase::Starting;
        }
        self.observe_operation(source, operation.ticket, operation.request, false);
    }

    fn confirm(mut self) {
        let Some(dialog) = self.dialog.peek().clone() else {
            return;
        };
        let PushPhase::Review { request, .. } = dialog.phase else {
            return;
        };
        if !self.current_dialog(dialog.ticket) {
            return;
        }
        self.operations.write().insert(
            dialog.source.clone(),
            PushOperation {
                ticket: dialog.ticket,
                instance: dialog.instance,
                request,
                phase: PushOperationPhase::Starting,
            },
        );
        self.dismiss();
        self.observe_operation(dialog.source, dialog.ticket, request, true);
    }

    fn observe_operation(
        self,
        source: PushSourceKey,
        ticket: u64,
        request: ViewerPushRequest,
        start: bool,
    ) {
        spawn_forever(async move {
            run_operation(&self, source, ticket, request, start).await;
        });
    }

    fn finish_dialog(self, ticket: u64, report: PushReport) {
        if !self.current_dialog(ticket) {
            return;
        }
        self.dismiss();
        self.report(report);
    }

    fn finish_operation(mut self, source: &PushSourceKey, ticket: u64, report: PushReport) {
        if !self.current_operation(source, ticket) {
            return;
        }
        self.operations.write().remove(source);
        self.report(report);
    }

    fn report(mut self, report: PushReport) {
        self.refresh.with_mut(|epoch| *epoch += 1);
        match report {
            PushReport::Completed => self.toast.ok(ToastText::localized(|language| {
                t!(language, "push-completed")
            })),
            PushReport::Failed(failure) => self.toast.failure(&failure),
            PushReport::Client(error) => self.toast.client_error(&error),
            PushReport::NotStarted => self.toast.warn(ToastText::localized(|language| {
                t!(language, "push-not-started")
            })),
        }
        self.viewer.refresh(false);
    }

    fn close(self) {
        self.dismiss();
    }

    fn unresolved(mut self, source: &PushSourceKey, ticket: u64, reason: PushUnresolved) {
        if !self.current_operation(source, ticket) {
            return;
        }
        if let Some(operation) = self.operations.write().get_mut(source) {
            operation.phase = PushOperationPhase::Unresolved(reason.clone());
        }
        self.toast.warn(ToastText::localized(move |language| {
            reason.message(language)
        }));
    }

    fn set_operation_phase(
        mut self,
        source: &PushSourceKey,
        ticket: u64,
        phase: PushOperationPhase,
    ) {
        if !self.current_operation(source, ticket) {
            return;
        }
        if let Some(operation) = self.operations.write().get_mut(source) {
            operation.phase = phase;
        }
    }

    fn dismiss(mut self) {
        let dialog = self.dialog.write().take();
        if let Some(dialog) = dialog {
            browser::focus_element(dialog.trigger);
        }
    }
}

async fn run_operation(
    controller: &PushController,
    source: PushSourceKey,
    ticket: u64,
    request: ViewerPushRequest,
    start: bool,
) {
    // Always reconcile the operation, even if the start acknowledgement is lost.
    let start_error = if start {
        viewer_server::start_push(request).await.err()
    } else {
        None
    };
    observe(controller, source, ticket, request, start_error).await;
}

#[derive(Debug, PartialEq)]
struct PushButtonPresentation {
    label: String,
    title: String,
    state: ButtonState,
    unresolved: bool,
}

/// Only a queued, running, or unresolved push replaces the caller's `title`.
fn push_button_presentation(
    operation: Option<&PushOperationPhase>,
    preparing: bool,
    unavailable: bool,
    title: Option<String>,
    language: ViewerLanguage,
) -> PushButtonPresentation {
    let idle_state = if preparing {
        ButtonState::Loading
    } else if unavailable {
        ButtonState::Disabled
    } else {
        ButtonState::Enabled
    };
    match operation {
        Some(PushOperationPhase::Queued) => PushButtonPresentation {
            label: t!(language, "push-button"),
            title: t!(language, "push-waiting"),
            state: ButtonState::Waiting,
            unresolved: false,
        },
        Some(PushOperationPhase::Starting | PushOperationPhase::Running) => {
            PushButtonPresentation {
                label: t!(language, "push-button"),
                title: t!(language, "push-running"),
                state: ButtonState::Loading,
                unresolved: false,
            }
        }
        Some(PushOperationPhase::Unresolved(reason)) => PushButtonPresentation {
            label: t!(language, "push-check-result"),
            title: t!(
                language,
                "push-check-result-title",
                message = reason.message(language)
            ),
            state: idle_state,
            unresolved: true,
        },
        None => PushButtonPresentation {
            label: t!(language, "push-button"),
            title: title.unwrap_or_else(|| t!(language, "push-button")),
            state: idle_state,
            unresolved: false,
        },
    }
}

#[component]
pub(crate) fn PushButton(
    id: String,
    source: CreateViewerPush,
    #[props(default)] disabled: bool,
    #[props(default)] icon_only: bool,
    title: Option<String>,
) -> Element {
    let (presentation, activate) = use_push_trigger(source, disabled, title);
    let PushButtonPresentation {
        label,
        title,
        state,
        unresolved,
    } = presentation;
    let trigger = id.clone();
    rsx! {
        Button {
            id,
            size: if icon_only { ButtonSize::IconSmall } else { ButtonSize::Small },
            variant: ButtonVariant::Accent,
            class: "mobile:size-11 mobile:p-0",
            state,
            aria_label: label.clone(),
            title,
            icon: rsx! {
                if unresolved {
                    lucide_dioxus::RefreshCw { size: 14 }
                } else {
                    lucide_dioxus::Upload { size: 14 }
                }
            },
            onclick: move |_| activate.call(trigger.clone()),
            if !icon_only {
                span { class: "mobile:hidden", "{label}" }
            }
        }
    }
}

fn use_push_trigger(
    source: CreateViewerPush,
    disabled: bool,
    title: Option<String>,
) -> (PushButtonPresentation, Callback<String>) {
    let language = use_language();
    let controller = use_context::<PushController>();
    let source_key = PushSourceKey::from(&source);
    let dialog = controller.dialog.read();
    let preparing = dialog.as_ref().is_some_and(|dialog| {
        dialog.source == source_key && matches!(dialog.phase, PushPhase::Preparing)
    });
    let operation = controller.operations.read().get(&source_key).cloned();
    let unavailable = !controller.viewer.actions_enabled()
        || dialog.is_some()
        || (disabled && operation.is_none());
    let presentation = push_button_presentation(
        operation.as_ref().map(|operation| &operation.phase),
        preparing,
        unavailable,
        title,
        language,
    );
    let activate = use_callback(move |trigger| controller.activate(source.clone(), trigger));
    (presentation, activate)
}

#[component]
pub(super) fn PushMenuAction(
    source: CreateViewerPush,
    disabled: bool,
    title: String,
    menu_id: String,
    trigger_id: String,
) -> Element {
    let (presentation, activate) = use_push_trigger(source, disabled, Some(title));
    let PushButtonPresentation {
        label,
        title,
        state,
        unresolved,
    } = presentation;
    rsx! {
        Button {
            class: "control-menu-action viewer-tab-context-action",
            size: ButtonSize::Content,
            variant: ButtonVariant::Bare,
            role: "menuitem",
            tabindex: "-1",
            state,
            title,
            icon: rsx! {
                span { class: "inline-flex text-acc", aria_hidden: "true",
                    if unresolved {
                        lucide_dioxus::RefreshCw { size: 14 }
                    } else {
                        lucide_dioxus::Upload { size: 14 }
                    }
                }
            },
            onclick: move |_| {
                browser::hide_popover(&menu_id);
                activate.call(trigger_id.clone());
            },
            "{label}"
        }
    }
}

#[component]
pub(crate) fn PushDialogHost() -> Element {
    let language = use_language();
    let mut controller = use_context::<PushController>();
    use_effect(move || {
        let instance = controller.viewer.server_instance_id();
        let dialog_changed = controller
            .dialog
            .peek()
            .as_ref()
            .is_some_and(|dialog| dialog.instance != instance);
        if dialog_changed {
            controller.dialog.set(None);
        }
        let operations_changed = controller
            .operations
            .peek()
            .values()
            .any(|operation| operation.instance != instance);
        if operations_changed {
            controller
                .operations
                .write()
                .retain(|_, operation| operation.instance == instance);
        }
    });
    let Some(dialog) = controller.dialog.read().clone() else {
        return rsx! {};
    };
    match dialog.phase {
        PushPhase::Preparing => rsx! {},
        PushPhase::Review { preview, .. } => {
            rsx! {
                AlertDialog {
                    id: "viewer-push-confirmation",
                    trigger_id: dialog.trigger,
                    open: true,
                    size: AlertDialogSize::Wide,
                    title: t!(language, "push-dialog-title", count = preview.count),
                    description: t!(language, "push-dialog-description"),
                    confirm_label: t!(language, "push-dialog-confirm", count = preview.count),
                    onconfirm: move |()| controller.confirm(),
                    oncancel: move |()| controller.close(),
                    super::PushConfirmationDetails { preview: *preview }
                }
            }
        }
    }
}

async fn prepare(controller: &PushController, ticket: u64, source: CreateViewerPush) {
    let result = async {
        let request = viewer_server::create_push(source).await?;
        let status = viewer_server::get_push(request).await?;
        Ok::<_, ViewerClientError>((request, status))
    }
    .await;
    match result {
        Ok((request, ViewerPushStatus::Review(preview))) => {
            controller.set_dialog_phase(
                ticket,
                PushPhase::Review {
                    request,
                    preview: Box::new(preview),
                },
            );
        }
        Ok((_, ViewerPushStatus::Failed { failure })) => {
            controller.finish_dialog(ticket, PushReport::Failed(failure));
        }
        Ok(_) => controller.finish_dialog(ticket, PushReport::NotStarted),
        Err(error) => controller.finish_dialog(ticket, PushReport::Client(error)),
    }
}

async fn observe(
    controller: &PushController,
    source: PushSourceKey,
    ticket: u64,
    request: ViewerPushRequest,
    start_error: Option<ViewerClientError>,
) {
    // Thirty-two confirmed operations can each occupy a two-minute push worker.
    for _ in 0..2400 {
        if !controller.current_operation(&source, ticket) {
            return;
        }
        match viewer_server::get_push(request).await {
            Ok(ViewerPushStatus::Succeeded) => {
                controller.finish_operation(&source, ticket, PushReport::Completed);
                return;
            }
            Ok(ViewerPushStatus::Failed { failure }) => {
                controller.finish_operation(&source, ticket, PushReport::Failed(failure));
                return;
            }
            Ok(ViewerPushStatus::Review(_)) => {
                controller.finish_operation(
                    &source,
                    ticket,
                    start_error.map_or(PushReport::NotStarted, PushReport::Client),
                );
                return;
            }
            Ok(ViewerPushStatus::Queued) => {
                controller.set_operation_phase(&source, ticket, PushOperationPhase::Queued);
                dioxus_sdk_time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            Ok(ViewerPushStatus::Running) => {
                controller.set_operation_phase(&source, ticket, PushOperationPhase::Running);
            }
            Err(error) => {
                controller.unresolved(&source, ticket, PushUnresolved::StatusUnreadable(error));
                return;
            }
        }
        dioxus_sdk_time::sleep(Duration::from_millis(500)).await;
    }
    controller.unresolved(&source, ticket, PushUnresolved::StatusPending);
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDLE_TITLE: &str = "Push to origin";

    fn presentation(
        operation: Option<&PushOperationPhase>,
        preparing: bool,
        unavailable: bool,
    ) -> PushButtonPresentation {
        push_button_presentation(
            operation,
            preparing,
            unavailable,
            Some(IDLE_TITLE.to_owned()),
            ViewerLanguage::EnUs,
        )
    }

    #[test]
    fn a_queued_push_waits_behind_other_pushes() {
        assert_eq!(
            presentation(Some(&PushOperationPhase::Queued), false, true),
            PushButtonPresentation {
                label: "Push".to_owned(),
                title: "Waiting to push commits...".to_owned(),
                state: ButtonState::Waiting,
                unresolved: false,
            }
        );
    }

    #[test]
    fn a_starting_or_running_push_is_busy() {
        for phase in [PushOperationPhase::Starting, PushOperationPhase::Running] {
            assert_eq!(
                presentation(Some(&phase), false, true),
                PushButtonPresentation {
                    label: "Push".to_owned(),
                    title: "Pushing commits...".to_owned(),
                    state: ButtonState::Loading,
                    unresolved: false,
                }
            );
        }
    }

    #[test]
    fn preparing_a_push_is_busy_and_keeps_the_idle_title() {
        assert_eq!(
            presentation(None, true, true),
            PushButtonPresentation {
                label: "Push".to_owned(),
                title: IDLE_TITLE.to_owned(),
                state: ButtonState::Loading,
                unresolved: false,
            }
        );
    }

    #[test]
    fn an_unresolved_push_offers_to_check_its_result() {
        let phase = PushOperationPhase::Unresolved(PushUnresolved::StatusPending);

        assert_eq!(
            presentation(Some(&phase), false, false),
            PushButtonPresentation {
                label: "Check result".to_owned(),
                title: "Check push result: The push result is not available yet. Use the push button to check its result.".to_owned(),
                state: ButtonState::Enabled,
                unresolved: true,
            }
        );
    }

    #[test]
    fn an_idle_push_follows_availability() {
        assert_eq!(presentation(None, false, false).state, ButtonState::Enabled);
        assert_eq!(presentation(None, false, true).state, ButtonState::Disabled);
    }
}
