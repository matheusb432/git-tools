use dioxus::{core::spawn_forever, prelude::*};
use gtl_models::viewer::{ViewerShellRevision, ViewerTabId};
use gtl_wire::viewer::{
    SetViewerPreference, ViewerFeedback, ViewerShell, ViewerTabRequest, ViewerTheme,
};

use crate::{
    app::{application_navigation::ApplicationNavigation, application_router::Route},
    entities::diffs::{DiffViewerApi, theme_value},
    shared::{bridge::ClientApiError, browser, ui::FloatingNotice},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewerShellLoad {
    Loading,
    Ready(ViewerShell),
    Error(ClientApiError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ViewerShellOrder {
    request_generation: ViewerShellRequestGeneration,
    revision_watermark: Option<ViewerShellRevision>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ViewerShellRequestGeneration(u64);

impl ViewerShellRequestGeneration {
    #[must_use]
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ViewerRenderCommandTicket(u64);

impl ViewerRenderCommandTicket {
    #[must_use]
    const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerRenderCommand {
    SetPreference(SetViewerPreference),
    RefreshTab(ViewerTabRequest),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerRenderCommandSubmission {
    Started(ViewerRenderCommandTicket),
    Queued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewerRenderCommandCompletion {
    Stale,
    Finished,
    Continue {
        ticket: ViewerRenderCommandTicket,
        command: ViewerRenderCommand,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ViewerRenderCommandScheduler {
    generation: ViewerRenderCommandTicket,
    active: Option<ViewerRenderCommandTicket>,
    pending: Option<ViewerRenderCommand>,
}

impl ViewerRenderCommandScheduler {
    fn submit(&mut self, command: ViewerRenderCommand) -> ViewerRenderCommandSubmission {
        if self.active.is_some() {
            self.pending = Some(command);
            return ViewerRenderCommandSubmission::Queued;
        }
        let ticket = self.next_ticket();
        self.active = Some(ticket);
        ViewerRenderCommandSubmission::Started(ticket)
    }

    fn complete(&mut self, ticket: ViewerRenderCommandTicket) -> ViewerRenderCommandCompletion {
        if self.active != Some(ticket) {
            return ViewerRenderCommandCompletion::Stale;
        }
        let Some(command) = self.pending.take() else {
            self.active = None;
            return ViewerRenderCommandCompletion::Finished;
        };
        let ticket = self.next_ticket();
        self.active = Some(ticket);
        ViewerRenderCommandCompletion::Continue { ticket, command }
    }

    fn next_ticket(&mut self) -> ViewerRenderCommandTicket {
        self.generation = self.generation.next();
        self.generation
    }

    const fn is_pending(self) -> bool {
        self.active.is_some()
    }
}

impl ViewerShellOrder {
    fn start_request(self) -> (Self, ViewerShellRequestGeneration) {
        let order = self.advance_request_generation();
        (order, order.request_generation)
    }

    fn advance_request_generation(mut self) -> Self {
        self.request_generation = self.request_generation.next();
        self
    }

    fn observe_event(self, revision: ViewerShellRevision) -> Self {
        self.observe_revision(revision)
    }

    fn accept_command_response(self, revision: ViewerShellRevision) -> Option<Self> {
        if self.revision_is_stale(revision) {
            return None;
        }
        Some(self.observe_revision(revision).advance_request_generation())
    }

    fn accept_query_response(
        self,
        request_generation: ViewerShellRequestGeneration,
        revision: ViewerShellRevision,
    ) -> Option<Self> {
        if !self.request_is_current(request_generation) || self.revision_is_stale(revision) {
            return None;
        }
        Some(self.observe_revision(revision))
    }

    fn request_is_current(self, request_generation: ViewerShellRequestGeneration) -> bool {
        self.request_generation == request_generation
    }

    fn revision_is_stale(self, revision: ViewerShellRevision) -> bool {
        self.revision_watermark
            .is_some_and(|watermark| revision < watermark)
    }

    fn observe_revision(mut self, revision: ViewerShellRevision) -> Self {
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
    render_command_scheduler: Signal<ViewerRenderCommandScheduler>,
    render_command_error: Signal<Option<ClientApiError>>,
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
        self.render_command_error.set(None);
        self.shell.set(ViewerShellLoad::Ready(shell));
        ViewerShellReplacement::Accepted
    }

    pub(crate) fn set_preference(self, preference: SetViewerPreference) {
        self.schedule_render_command(ViewerRenderCommand::SetPreference(preference));
    }

    pub(crate) fn refresh_tab(self, tab_id: ViewerTabId) {
        self.schedule_render_command(ViewerRenderCommand::RefreshTab(ViewerTabRequest { tab_id }));
    }

    fn schedule_render_command(mut self, command: ViewerRenderCommand) {
        self.render_command_error.set(None);
        let submission = self.render_command_scheduler.write().submit(command);
        if let ViewerRenderCommandSubmission::Started(ticket) = submission {
            self.start_render_command(ticket, command);
        }
    }

    fn start_render_command(self, ticket: ViewerRenderCommandTicket, command: ViewerRenderCommand) {
        spawn_forever(async move {
            let result = match command {
                ViewerRenderCommand::SetPreference(preference) => {
                    DiffViewerApi::set_preference(preference).await
                }
                ViewerRenderCommand::RefreshTab(request) => {
                    DiffViewerApi::refresh_tab(request).await
                }
            };
            self.complete_render_command(ticket, result);
        });
    }

    fn complete_render_command(
        mut self,
        ticket: ViewerRenderCommandTicket,
        result: Result<ViewerShell, ClientApiError>,
    ) {
        let next = match self.render_command_scheduler.write().complete(ticket) {
            ViewerRenderCommandCompletion::Stale => return,
            ViewerRenderCommandCompletion::Finished => None,
            ViewerRenderCommandCompletion::Continue { ticket, command } => Some((ticket, command)),
        };
        match result {
            Ok(shell) => self.replace_shell(shell),
            Err(error) => self.render_command_error.set(Some(error)),
        }
        if let Some((ticket, command)) = next {
            self.start_render_command(ticket, command);
        }
    }

    pub(crate) fn render_command_pending(self) -> bool {
        (self.render_command_scheduler)().is_pending()
    }

    pub(crate) fn render_command_error(self) -> Option<ClientApiError> {
        (self.render_command_error)()
    }

    fn report_error(mut self, error: ClientApiError) {
        self.shell.set(ViewerShellLoad::Error(error));
    }

    pub(crate) fn reconnect(mut self) {
        self.render_command_error.set(None);
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
            let result = DiffViewerApi::get_shell().await;
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
                    self.render_command_error.set(None);
                    self.shell.set(ViewerShellLoad::Ready(shell));
                }
                Err(error) => self.shell.set(ViewerShellLoad::Error(error)),
            }
        });
    }

    fn invalidate(mut self, revision: ViewerShellRevision) {
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
    let render_command_scheduler = use_signal(ViewerRenderCommandScheduler::default);
    let render_command_error = use_signal(|| None::<ClientApiError>);
    let state_change_revision = use_signal(|| None::<ViewerShellRevision>);
    let context = ViewerContext {
        shell,
        shell_order,
        reconnect_generation,
        render_command_scheduler,
        render_command_error,
    };
    use_context_provider(|| context);

    let mut state_changes = use_future(move || async move {
        if let Err(error) = DiffViewerApi::listen_for_state_changes(
            move || context.refresh(true),
            move |event| {
                let mut revision = state_change_revision;
                revision.with_mut(|revision| {
                    *revision = Some(
                        revision.map_or(event.revision, |current| current.max(event.revision)),
                    );
                });
            },
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
    use_effect(move || {
        if let Some(revision) = state_change_revision() {
            spawn(async move {
                context.invalidate(revision);
            });
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
            class: "flex h-screen min-h-128 flex-col overflow-hidden bg-bg text-ink antialiased",
            "data-theme": theme_value(theme),
            ApplicationNavigation {}
            if let ViewerShellLoad::Ready(shell) = &state {
                if let Some(feedback) = &shell.feedback {
                    ViewerFeedbackNotice { feedback: feedback.clone() }
                }
            }
            div { class: "min-h-0 flex-1 overflow-hidden", Outlet::<Route> {} }
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
        FloatingNotice { role: "status", "{message}" }
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerShellRevision;
    use gtl_wire::viewer::{
        SetViewerPreference, ViewerDiffDensity, ViewerDiffLayout, ViewerTabRequest,
    };

    use super::{
        ViewerRenderCommand, ViewerRenderCommandCompletion, ViewerRenderCommandScheduler,
        ViewerRenderCommandSubmission, ViewerRenderCommandTicket, ViewerShellOrder,
    };
    use crate::test_support::{TestResult, viewer_tab_id};

    fn revision(value: u64) -> ViewerShellRevision {
        ViewerShellRevision::new(value)
    }

    #[test]
    fn render_commands_run_the_latest_rapid_preference_after_the_active_preference() {
        let mut scheduler = ViewerRenderCommandScheduler::default();
        let layout_ticket = ViewerRenderCommandTicket(1);
        let density_ticket = ViewerRenderCommandTicket(2);
        let layout = ViewerRenderCommand::SetPreference(SetViewerPreference::Layout(
            ViewerDiffLayout::Split,
        ));
        let density = ViewerRenderCommand::SetPreference(SetViewerPreference::Density(
            ViewerDiffDensity::Full,
        ));

        assert_eq!(
            scheduler.submit(layout),
            ViewerRenderCommandSubmission::Started(layout_ticket)
        );
        assert_eq!(
            scheduler.submit(density),
            ViewerRenderCommandSubmission::Queued
        );
        assert_eq!(
            scheduler.complete(layout_ticket),
            ViewerRenderCommandCompletion::Continue {
                ticket: density_ticket,
                command: density,
            }
        );
        assert!(scheduler.is_pending());
        assert_eq!(
            scheduler.complete(density_ticket),
            ViewerRenderCommandCompletion::Finished
        );
        assert!(!scheduler.is_pending());
    }

    #[test]
    fn refresh_commands_coalesce_to_the_latest_tab_after_the_active_preference() -> TestResult {
        let mut scheduler = ViewerRenderCommandScheduler::default();
        let layout_ticket = ViewerRenderCommandTicket(1);
        let refresh_ticket = ViewerRenderCommandTicket(2);
        let layout = ViewerRenderCommand::SetPreference(SetViewerPreference::Layout(
            ViewerDiffLayout::Unified,
        ));
        let density = ViewerRenderCommand::SetPreference(SetViewerPreference::Density(
            ViewerDiffDensity::Compact,
        ));
        let first_refresh = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(7)?,
        });
        let latest_refresh = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(11)?,
        });

        assert_eq!(
            scheduler.submit(layout),
            ViewerRenderCommandSubmission::Started(layout_ticket)
        );
        assert_eq!(
            scheduler.submit(density),
            ViewerRenderCommandSubmission::Queued
        );
        assert_eq!(
            scheduler.submit(first_refresh),
            ViewerRenderCommandSubmission::Queued
        );
        assert_eq!(
            scheduler.submit(latest_refresh),
            ViewerRenderCommandSubmission::Queued
        );
        assert_eq!(
            scheduler.complete(layout_ticket),
            ViewerRenderCommandCompletion::Continue {
                ticket: refresh_ticket,
                command: latest_refresh,
            }
        );
        assert_eq!(
            scheduler.complete(layout_ticket),
            ViewerRenderCommandCompletion::Stale
        );
        assert!(scheduler.is_pending());
        assert_eq!(
            scheduler.complete(refresh_ticket),
            ViewerRenderCommandCompletion::Finished
        );
        assert!(!scheduler.is_pending());
        Ok(())
    }

    #[test]
    fn stale_command_response_preserves_event_refresh_generation() {
        let (order, initial_request_generation) = ViewerShellOrder::default().start_request();
        let order_initial = ViewerShellOrder {
            request_generation: initial_request_generation,
            revision_watermark: Some(revision(7)),
        };
        assert_eq!(
            order.accept_query_response(initial_request_generation, revision(7)),
            Some(order_initial)
        );
        let order = order_initial;
        let (order, event_request_generation) = order.observe_event(revision(8)).start_request();

        assert!(order.accept_command_response(revision(7)).is_none());
        assert_eq!(order.request_generation, event_request_generation);

        assert_eq!(
            order.accept_query_response(event_request_generation, revision(8)),
            Some(ViewerShellOrder {
                request_generation: event_request_generation,
                revision_watermark: Some(revision(8)),
            })
        );
    }

    #[test]
    fn command_at_event_revision_supersedes_the_event_refresh() {
        let (order, event_request_generation) = ViewerShellOrder::default()
            .observe_event(revision(12))
            .start_request();

        let order_command = ViewerShellOrder {
            request_generation: event_request_generation.next(),
            revision_watermark: Some(revision(12)),
        };
        assert_eq!(
            order.accept_command_response(revision(12)),
            Some(order_command)
        );
        let order = order_command;

        assert_ne!(order.request_generation, event_request_generation);
        assert!(
            order
                .accept_query_response(event_request_generation, revision(12))
                .is_none()
        );
    }

    #[test]
    fn out_of_order_events_keep_the_highest_revision_watermark() {
        let order = ViewerShellOrder::default()
            .observe_event(revision(15))
            .observe_event(revision(13));

        assert_eq!(order.revision_watermark, Some(revision(15)));
        assert!(order.accept_command_response(revision(14)).is_none());
    }

    #[test]
    fn accepted_shell_revision_rejects_a_later_older_command() {
        let (order, request_generation) = ViewerShellOrder::default().start_request();
        let order_accepted = ViewerShellOrder {
            request_generation,
            revision_watermark: Some(revision(21)),
        };
        assert_eq!(
            order.accept_query_response(request_generation, revision(21)),
            Some(order_accepted)
        );
        let order = order_accepted;

        assert!(order.accept_command_response(revision(20)).is_none());
        assert_eq!(order.request_generation, request_generation);
        assert_eq!(order.revision_watermark, Some(revision(21)));
    }
}
