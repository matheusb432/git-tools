use dioxus::prelude::*;
use gtl_contracts::viewer::{ViewerFeedback, ViewerShell, ViewerTheme};

use crate::{
    app::{application_navigation::ApplicationNavigation, application_router::Route},
    entities::diffs::{ViewerApi, theme_value},
    shared::{bridge::ClientApiError, browser},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewerShellLoad {
    Loading,
    Ready(ViewerShell),
    Error(ClientApiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ViewerShellOrder {
    request_generation: u64,
    revision_watermark: Option<u64>,
}

impl ViewerShellOrder {
    fn start_request(self) -> (Self, u64) {
        let order = self.advance_request_generation();
        (order, order.request_generation)
    }

    fn advance_request_generation(mut self) -> Self {
        self.request_generation = self.request_generation.wrapping_add(1);
        self
    }

    fn observe_event(self, revision: u64) -> Self {
        self.observe_revision(revision)
    }

    fn accept_command_response(self, revision: u64) -> Option<Self> {
        if self.revision_is_stale(revision) {
            return None;
        }
        Some(self.observe_revision(revision).advance_request_generation())
    }

    fn accept_query_response(self, request_generation: u64, revision: u64) -> Option<Self> {
        if !self.request_is_current(request_generation) || self.revision_is_stale(revision) {
            return None;
        }
        Some(self.observe_revision(revision))
    }

    const fn request_is_current(self, request_generation: u64) -> bool {
        self.request_generation == request_generation
    }

    fn revision_is_stale(self, revision: u64) -> bool {
        self.revision_watermark
            .is_some_and(|watermark| revision < watermark)
    }

    fn observe_revision(mut self, revision: u64) -> Self {
        self.revision_watermark = Some(
            self.revision_watermark
                .map_or(revision, |watermark| watermark.max(revision)),
        );
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewerShellReplacement {
    Accepted,
    Stale,
}

#[derive(Clone, Copy)]
pub(crate) struct ViewerContext {
    shell: Signal<ViewerShellLoad>,
    shell_order: Signal<ViewerShellOrder>,
    reconnect_generation: Signal<u64>,
}

impl ViewerContext {
    pub(crate) fn read(self) -> ViewerShellLoad {
        (self.shell)()
    }

    pub(crate) fn replace_shell(self, shell: ViewerShell) {
        self.try_replace_shell(shell);
    }

    pub(crate) fn try_replace_shell(mut self, shell: ViewerShell) -> ViewerShellReplacement {
        let Some(order) = (self.shell_order)().accept_command_response(shell.revision) else {
            return ViewerShellReplacement::Stale;
        };
        self.shell_order.set(order);
        self.shell.set(ViewerShellLoad::Ready(shell));
        ViewerShellReplacement::Accepted
    }

    fn report_error(mut self, error: ClientApiError) {
        self.shell.set(ViewerShellLoad::Error(error));
    }

    pub(crate) fn reconnect(mut self) {
        self.shell.set(ViewerShellLoad::Loading);
        *self.reconnect_generation.write() += 1;
    }

    pub(crate) fn refresh(mut self, show_loading: bool) {
        let (order, request_generation) = (self.shell_order)().start_request();
        self.shell_order.set(order);
        if show_loading {
            self.shell.set(ViewerShellLoad::Loading);
        }

        spawn(async move {
            let result = ViewerApi::get_shell().await;
            let order = (self.shell_order)();
            if !order.request_is_current(request_generation) {
                return;
            }
            match result {
                Ok(shell) => {
                    let Some(order) =
                        order.accept_query_response(request_generation, shell.revision)
                    else {
                        return;
                    };
                    self.shell_order.set(order);
                    self.shell.set(ViewerShellLoad::Ready(shell));
                }
                Err(error) => self.shell.set(ViewerShellLoad::Error(error)),
            }
        });
    }

    fn invalidate(mut self, revision: u64) {
        let order = (self.shell_order)().observe_event(revision);
        self.shell_order.set(order);
        let is_current = matches!(
            (self.shell)(),
            ViewerShellLoad::Ready(ref shell) if !order.revision_is_stale(shell.revision)
        );
        if !is_current {
            self.refresh(false);
        }
    }
}

#[component]
pub(crate) fn ApplicationLayout() -> Element {
    let shell = use_signal(|| ViewerShellLoad::Loading);
    let shell_order = use_signal(ViewerShellOrder::default);
    let reconnect_generation = use_signal(|| 0_u64);
    let context = ViewerContext {
        shell,
        shell_order,
        reconnect_generation,
    };
    use_context_provider(|| context);

    let mut state_changes = use_future(move || async move {
        if let Err(error) = ViewerApi::listen_for_state_changes(
            move || context.refresh(true),
            move |event| context.invalidate(event.revision),
        )
        .await
        {
            context.report_error(error);
        }
    });
    use_effect(move || {
        let reconnect_generation = reconnect_generation();
        if reconnect_generation > 0 {
            state_changes.restart();
        }
    });

    let state = context.read();
    let theme = match &state {
        ViewerShellLoad::Ready(shell) => shell.preferences.theme,
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => ViewerTheme::Dark,
    };
    use_effect(use_reactive((&theme,), move |(theme,)| {
        browser::apply_theme(theme_value(theme));
    }));

    rsx! {
        div {
            class: "flex h-screen min-h-[32rem] flex-col overflow-hidden bg-bg text-ink antialiased",
            "data-theme": theme_value(theme),
            ApplicationNavigation {}
            if let ViewerShellLoad::Ready(shell) = &state {
                if let Some(feedback) = &shell.feedback {
                    ViewerFeedbackNotice { feedback: feedback.clone() }
                }
            }
            div { class: "min-h-0 flex-1 overflow-hidden",
                Outlet::<Route> {}
            }
        }
    }
}

#[component]
fn ViewerFeedbackNotice(feedback: ViewerFeedback) -> Element {
    let message = match feedback {
        ViewerFeedback::TabClosed => "Tab closed.",
        ViewerFeedback::LiveViewDeleted => "Live view deleted.",
        ViewerFeedback::SnapshotRecipesSkipped { .. } => {
            "Some snapshot recipes were skipped because they were already open."
        }
    };

    rsx! {
        div { class: "shrink-0 border-b border-acc-line bg-acc-soft px-4 py-2 text-xs text-acc", role: "status",
            "{message}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ViewerShellOrder;

    #[test]
    fn stale_command_response_preserves_event_refresh_generation() {
        let (order, initial_request_generation) = ViewerShellOrder::default().start_request();
        let order_initial = ViewerShellOrder {
            request_generation: initial_request_generation,
            revision_watermark: Some(7),
        };
        assert_eq!(
            order.accept_query_response(initial_request_generation, 7),
            Some(order_initial)
        );
        let order = order_initial;
        let (order, event_request_generation) = order.observe_event(8).start_request();

        assert!(order.accept_command_response(7).is_none());
        assert_eq!(order.request_generation, event_request_generation);

        assert_eq!(
            order.accept_query_response(event_request_generation, 8),
            Some(ViewerShellOrder {
                request_generation: event_request_generation,
                revision_watermark: Some(8),
            })
        );
    }

    #[test]
    fn command_at_event_revision_supersedes_the_event_refresh() {
        let (order, event_request_generation) = ViewerShellOrder::default()
            .observe_event(12)
            .start_request();

        let order_command = ViewerShellOrder {
            request_generation: event_request_generation.wrapping_add(1),
            revision_watermark: Some(12),
        };
        assert_eq!(order.accept_command_response(12), Some(order_command));
        let order = order_command;

        assert_ne!(order.request_generation, event_request_generation);
        assert!(
            order
                .accept_query_response(event_request_generation, 12)
                .is_none()
        );
    }

    #[test]
    fn out_of_order_events_keep_the_highest_revision_watermark() {
        let order = ViewerShellOrder::default()
            .observe_event(15)
            .observe_event(13);

        assert_eq!(order.revision_watermark, Some(15));
        assert!(order.accept_command_response(14).is_none());
    }

    #[test]
    fn accepted_shell_revision_rejects_a_later_older_command() {
        let (order, request_generation) = ViewerShellOrder::default().start_request();
        let order_accepted = ViewerShellOrder {
            request_generation,
            revision_watermark: Some(21),
        };
        assert_eq!(
            order.accept_query_response(request_generation, 21),
            Some(order_accepted)
        );
        let order = order_accepted;

        assert!(order.accept_command_response(20).is_none());
        assert_eq!(order.request_generation, request_generation);
        assert_eq!(order.revision_watermark, Some(21));
    }
}
