use std::{cell::Cell, rc::Rc};

use dioxus::{core::spawn_forever, prelude::*};
use gtl_models::viewer::{ViewerTabId, ViewerVersion};
use gtl_wire::viewer::{ViewerFeedback, ViewerShell, ViewerTabRequest, ViewerTheme};

use crate::{
    app::{
        application_navigation::ApplicationNavigation,
        application_router::{Route, use_viewer_routes},
    },
    entities::diffs::viewer_server,
    shared::{
        browser,
        retry_delay::RetryDelay,
        ui::{
            Button, ButtonSize, ButtonVariant, OVERLAY_SCROLLBAR_CLASSES, ToastHandle, ToastHost,
            use_toast,
        },
        viewer_client::{ViewerClientError, discard_viewer_connection},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewerShellLoad {
    Loading,
    Ready(ViewerShell),
    Error(ViewerClientError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewerConnection {
    Connecting,
    Connected,
    Retrying(ViewerClientError),
}

impl ViewerConnection {
    const fn is_connected(self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ViewerShellOrder {
    request_generation: ViewerShellRequestGeneration,
    version_watermark: Option<ViewerVersion>,
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ViewerDiffRowsLoading {
    tab_id: Option<ViewerTabId>,
}

impl ViewerDiffRowsLoading {
    const fn tab_id(self) -> Option<ViewerTabId> {
        self.tab_id
    }

    fn set(&mut self, tab_id: ViewerTabId, loading: bool) {
        if loading {
            self.tab_id = Some(tab_id);
        } else if self.tab_id == Some(tab_id) {
            self.tab_id = None;
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct ViewerRenderState {
    commands: ViewerRenderCommandScheduler,
    diff_rows_loading: ViewerDiffRowsLoading,
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

    fn observe_event(self, version: ViewerVersion) -> Self {
        self.observe_version(version)
    }

    fn accept_command_response(self, version: ViewerVersion) -> Option<Self> {
        if self.version_is_stale(version) {
            return None;
        }
        Some(self.observe_version(version).advance_request_generation())
    }

    fn accept_query_response(
        self,
        request_generation: ViewerShellRequestGeneration,
        version: ViewerVersion,
    ) -> Option<Self> {
        if !self.request_is_current(request_generation) || self.version_is_stale(version) {
            return None;
        }
        Some(self.observe_version(version))
    }

    fn request_is_current(self, request_generation: ViewerShellRequestGeneration) -> bool {
        self.request_generation == request_generation
    }

    fn version_is_stale(self, version: ViewerVersion) -> bool {
        self.version_watermark
            .is_some_and(|watermark| version < watermark)
    }

    fn observe_version(mut self, version: ViewerVersion) -> Self {
        self.version_watermark = Some(
            self.version_watermark
                .map_or(version, |watermark| watermark.max(version)),
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
    shell_reader: ReadSignal<ViewerShellLoad>,
    connection: Signal<ViewerConnection>,
    shell_order: Signal<ViewerShellOrder>,
    reconnect_generation: Signal<u64>,
    server_instance_id: Signal<Option<String>>,
    render_state: Signal<ViewerRenderState>,
    toast: ToastHandle,
}

impl ViewerContext {
    pub(crate) fn shell(self) -> ReadSignal<ViewerShellLoad> {
        self.shell_reader
    }

    pub(crate) fn server_instance_id(self) -> Option<String> {
        (self.server_instance_id)()
    }

    pub(crate) fn replace_shell(self, shell: ViewerShell) {
        self.try_replace_shell(shell);
    }

    pub(crate) fn connection(self) -> ViewerConnection {
        (self.connection)()
    }

    pub(crate) fn actions_enabled(self) -> bool {
        self.connection().is_connected()
    }

    pub(crate) fn try_replace_shell(mut self, shell: ViewerShell) -> ViewerShellReplacement {
        let Some(order) = (self.shell_order)().accept_command_response(shell.version) else {
            return ViewerShellReplacement::Stale;
        };
        self.shell_order.set(order);
        if let Some(notification) = viewer_feedback_toast(shell.feedback.as_ref()) {
            notification.enqueue(self.toast);
        }
        self.shell.set(ViewerShellLoad::Ready(shell));
        ViewerShellReplacement::Accepted
    }

    pub(crate) fn refresh_tab(self, tab_id: ViewerTabId) {
        self.schedule_render_command(ViewerRenderCommand::RefreshTab(ViewerTabRequest { tab_id }));
    }

    fn schedule_render_command(mut self, command: ViewerRenderCommand) {
        if !self.actions_enabled() {
            self.toast.error(ViewerClientError::Unavailable.message());
            return;
        }
        let submission = self.render_state.write().commands.submit(command);
        if let ViewerRenderCommandSubmission::Started(ticket) = submission {
            self.start_render_command(ticket, command);
        }
    }

    fn start_render_command(self, ticket: ViewerRenderCommandTicket, command: ViewerRenderCommand) {
        spawn_forever(run_render_command(self, ticket, command));
    }

    fn complete_render_command(
        mut self,
        ticket: ViewerRenderCommandTicket,
        result: Result<ViewerShell, ViewerClientError>,
    ) {
        let next = match self.render_state.write().commands.complete(ticket) {
            ViewerRenderCommandCompletion::Stale => return,
            ViewerRenderCommandCompletion::Finished => None,
            ViewerRenderCommandCompletion::Continue { ticket, command } => Some((ticket, command)),
        };
        match result {
            Ok(shell) => self.replace_shell(shell),
            Err(error) => self.toast.error(error.message()),
        }
        if let Some((ticket, command)) = next {
            self.start_render_command(ticket, command);
        }
    }

    pub(crate) fn render_command_pending(self) -> bool {
        (self.render_state)().commands.is_pending()
    }

    pub(crate) fn diff_rows_loading_tab_id(self) -> Option<ViewerTabId> {
        (self.render_state)().diff_rows_loading.tab_id()
    }

    pub(crate) fn set_diff_rows_loading(mut self, tab_id: ViewerTabId, loading: bool) {
        self.render_state
            .write()
            .diff_rows_loading
            .set(tab_id, loading);
    }

    fn connected_to(mut self, server_instance_id: String) -> bool {
        let server_changed = self
            .server_instance_id
            .peek()
            .as_ref()
            .is_some_and(|current| current != &server_instance_id);
        if server_changed {
            self.shell_order.set(ViewerShellOrder::default());
            self.shell.set(ViewerShellLoad::Loading);
            self.render_state.set(ViewerRenderState::default());
        }
        self.server_instance_id.set(Some(server_instance_id));
        self.connection.set(ViewerConnection::Connected);
        server_changed
    }

    fn disconnected(mut self, error: ViewerClientError) {
        self.connection.set(ViewerConnection::Retrying(error));
    }

    pub(crate) fn reconnect(mut self) {
        discard_viewer_connection();
        if !matches!((self.shell)(), ViewerShellLoad::Ready(_)) {
            self.shell.set(ViewerShellLoad::Loading);
        }
        self.connection.set(ViewerConnection::Connecting);
        *self.reconnect_generation.write() += 1;
    }

    pub(crate) fn refresh(mut self, show_loading: bool) {
        let (order, request_generation) = (self.shell_order)().start_request();
        self.shell_order.set(order);
        if show_loading {
            self.shell.set(ViewerShellLoad::Loading);
        }

        spawn(refresh_shell(self, request_generation));
    }

    fn invalidate(mut self, version: ViewerVersion) {
        let order = (self.shell_order)().observe_event(version);
        self.shell_order.set(order);
        let is_current = matches!(
            (self.shell)(),
            ViewerShellLoad::Ready(ref shell) if !order.version_is_stale(shell.version)
        );
        if !is_current {
            self.refresh(false);
        }
    }
}

async fn run_render_command(
    context: ViewerContext,
    ticket: ViewerRenderCommandTicket,
    command: ViewerRenderCommand,
) {
    let ViewerRenderCommand::RefreshTab(request) = command;
    let result = viewer_server::refresh_tab(request).await;
    context.complete_render_command(ticket, result);
}

async fn refresh_shell(
    mut context: ViewerContext,
    request_generation: ViewerShellRequestGeneration,
) {
    let result = viewer_server::get_shell().await;
    let order = (context.shell_order)();
    if !order.request_is_current(request_generation) {
        return;
    }
    match result {
        Ok(shell) => {
            let Some(order) = order.accept_query_response(request_generation, shell.version) else {
                return;
            };
            context.shell_order.set(order);
            if let Some(notification) = viewer_feedback_toast(shell.feedback.as_ref()) {
                notification.enqueue(context.toast);
            }
            context.shell.set(ViewerShellLoad::Ready(shell));
        }
        Err(error) => {
            if matches!((context.shell)(), ViewerShellLoad::Ready(_)) {
                context.toast.error(error.message());
            } else {
                context.shell.set(ViewerShellLoad::Error(error));
            }
        }
    }
}

#[component]
pub(crate) fn ApplicationLayout() -> Element {
    rsx! {
        ToastHost { ApplicationLayoutContent {} }
    }
}

#[component]
fn ApplicationLayoutContent() -> Element {
    let shell = use_signal(|| ViewerShellLoad::Loading);
    let shell_reader = use_hook(move || shell.into());
    let connection = use_signal(|| ViewerConnection::Connecting);
    let shell_order = use_signal(ViewerShellOrder::default);
    let reconnect_generation = use_signal(|| 0_u64);
    let server_instance_id = use_signal(|| None::<String>);
    let render_state = use_signal(ViewerRenderState::default);
    let state_change_version = use_signal(|| None::<ViewerVersion>);
    let toast = use_toast();
    let context = ViewerContext {
        shell,
        shell_reader,
        connection,
        shell_order,
        reconnect_generation,
        server_instance_id,
        render_state,
        toast,
    };
    use_context_provider(|| context);
    use_viewer_routes(context);

    let mut state_changes = use_future(move || async move {
        let mut retry_delay = RetryDelay::default();
        loop {
            let received_event = Rc::new(Cell::new(false));
            let event_received = Rc::clone(&received_event);
            let result = viewer_server::listen_for_state_changes(
                move |server_instance_id| {
                    if context.connected_to(server_instance_id) {
                        let mut version = state_change_version;
                        version.set(None);
                    }
                },
                move |event| {
                    event_received.set(true);
                    let mut version = state_change_version;
                    version.with_mut(|version| {
                        *version = Some(
                            version.map_or(event.version, |current| current.max(event.version)),
                        );
                    });
                },
            )
            .await;
            let error = result.err().unwrap_or(ViewerClientError::Unavailable);
            discard_viewer_connection();
            context.disconnected(error);
            if error == ViewerClientError::ProtocolMismatch {
                return;
            }
            if received_event.get() {
                retry_delay.reset();
            }
            dioxus_sdk_time::sleep(retry_delay.take_and_advance()).await;
        }
    });
    use_effect(move || {
        let reconnect_generation = reconnect_generation();
        if reconnect_generation > 0 {
            state_changes.restart();
        }
    });
    use_effect(move || {
        if let Some(version) = state_change_version() {
            spawn(async move {
                context.invalidate(version);
            });
        }
    });

    let shell = context.shell();
    let state = shell.read();
    let connection = context.connection();
    let theme = match &*state {
        ViewerShellLoad::Ready(shell) => shell.preferences.theme,
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => ViewerTheme::Dark,
    };
    use_effect(use_reactive((&theme,), move |(theme,)| {
        browser::apply_theme(theme.as_str());
    }));

    rsx! {
        div {
            class: "flex h-screen min-h-128 flex-col overflow-hidden bg-bg text-ink antialiased {OVERLAY_SCROLLBAR_CLASSES}",
            "data-theme": theme.as_str(),
            div {
                class: if connection.is_connected() { "flex min-h-0 flex-1 flex-col" } else { "flex min-h-0 flex-1 flex-col opacity-70 saturate-50" },
                "inert": (!connection.is_connected()).then_some(""),
                aria_busy: (!connection.is_connected()).to_string(),
                ApplicationNavigation {}
                div { class: "min-h-0 flex-1 overflow-hidden", Outlet::<Route> {} }
            }
            if !connection.is_connected() {
                ViewerConnectionNotice { connection, onretry: move |()| context.reconnect() }
            }
        }
    }
}

#[component]
fn ViewerConnectionNotice(connection: ViewerConnection, onretry: EventHandler<()>) -> Element {
    let (message, can_retry) = match connection {
        ViewerConnection::Connecting => ("Connecting to the viewer server…", false),
        ViewerConnection::Connected => return rsx! {},
        ViewerConnection::Retrying(error) => (
            error.message(),
            error != ViewerClientError::ProtocolMismatch,
        ),
    };

    rsx! {
        div {
            class: "fixed inset-x-4 bottom-6 z-80 mx-auto flex w-fit max-w-3xl items-center gap-3 rounded-panel border border-del-line bg-surface px-4 py-2 text-del shadow-floating",
            role: if can_retry { "alert" } else { "status" },
            p {
                if can_retry {
                    "{message} Retrying automatically."
                } else {
                    "{message}"
                }
            }
            if can_retry {
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| onretry.call(()),
                    "Try now"
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ViewerFeedbackToast {
    Ok(String),
    Warn(String),
}

impl ViewerFeedbackToast {
    fn enqueue(self, toast: ToastHandle) {
        match self {
            Self::Ok(message) => toast.ok(message),
            Self::Warn(message) => toast.warn(message),
        }
    }
}

fn viewer_feedback_toast(feedback: Option<&ViewerFeedback>) -> Option<ViewerFeedbackToast> {
    match feedback? {
        ViewerFeedback::TabClosed => None,
        ViewerFeedback::LiveViewDeleted => {
            Some(ViewerFeedbackToast::Ok("Live view deleted.".to_owned()))
        }
        ViewerFeedback::SnapshotRecipesSkipped { labels } => {
            let message = if labels.is_empty() {
                "Skipped snapshot diffs with no commits or changed files.".to_owned()
            } else {
                let noun = diff_noun(labels.len());
                format!(
                    "Skipped {} {noun} with no commits or changed files: {}.",
                    labels.len(),
                    labels.join(", ")
                )
            };
            Some(ViewerFeedbackToast::Warn(message))
        }
    }
}

const fn diff_noun(count: usize) -> &'static str {
    if count == 1 { "diff" } else { "diffs" }
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::ViewerVersion;
    use gtl_wire::viewer::{ViewerFeedback, ViewerTabRequest};

    use super::{
        ViewerDiffRowsLoading, ViewerFeedbackToast, ViewerRenderCommand,
        ViewerRenderCommandCompletion, ViewerRenderCommandScheduler, ViewerRenderCommandSubmission,
        ViewerRenderCommandTicket, ViewerShellOrder, viewer_feedback_toast,
    };
    use crate::test_support::{TestResult, viewer_tab_id};

    fn version(value: u64) -> ViewerVersion {
        ViewerVersion::new(value)
    }

    #[test]
    fn render_commands_run_the_queued_refresh_after_the_active_refresh() -> TestResult {
        let mut scheduler = ViewerRenderCommandScheduler::default();
        let active_ticket = ViewerRenderCommandTicket(1);
        let queued_ticket = ViewerRenderCommandTicket(2);
        let active = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(7)?,
        });
        let queued = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(11)?,
        });

        assert_eq!(
            scheduler.submit(active),
            ViewerRenderCommandSubmission::Started(active_ticket)
        );
        assert_eq!(
            scheduler.submit(queued),
            ViewerRenderCommandSubmission::Queued
        );
        assert_eq!(
            scheduler.complete(active_ticket),
            ViewerRenderCommandCompletion::Continue {
                ticket: queued_ticket,
                command: queued,
            }
        );
        assert!(scheduler.is_pending());
        assert_eq!(
            scheduler.complete(queued_ticket),
            ViewerRenderCommandCompletion::Finished
        );
        assert!(!scheduler.is_pending());
        Ok(())
    }

    #[test]
    fn refresh_commands_coalesce_to_the_latest_tab_after_the_active_refresh() -> TestResult {
        let mut scheduler = ViewerRenderCommandScheduler::default();
        let active_ticket = ViewerRenderCommandTicket(1);
        let refresh_ticket = ViewerRenderCommandTicket(2);
        let active_refresh = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(3)?,
        });
        let first_refresh = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(7)?,
        });
        let latest_refresh = ViewerRenderCommand::RefreshTab(ViewerTabRequest {
            tab_id: viewer_tab_id(11)?,
        });

        assert_eq!(
            scheduler.submit(active_refresh),
            ViewerRenderCommandSubmission::Started(active_ticket)
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
            scheduler.complete(active_ticket),
            ViewerRenderCommandCompletion::Continue {
                ticket: refresh_ticket,
                command: latest_refresh,
            }
        );
        assert_eq!(
            scheduler.complete(active_ticket),
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
            version_watermark: Some(version(7)),
        };
        assert_eq!(
            order.accept_query_response(initial_request_generation, version(7)),
            Some(order_initial)
        );
        let order = order_initial;
        let (order, event_request_generation) = order.observe_event(version(8)).start_request();

        assert!(order.accept_command_response(version(7)).is_none());
        assert_eq!(order.request_generation, event_request_generation);

        assert_eq!(
            order.accept_query_response(event_request_generation, version(8)),
            Some(ViewerShellOrder {
                request_generation: event_request_generation,
                version_watermark: Some(version(8)),
            })
        );
    }

    #[test]
    fn command_at_event_version_supersedes_the_event_refresh() {
        let (order, event_request_generation) = ViewerShellOrder::default()
            .observe_event(version(12))
            .start_request();

        let order_command = ViewerShellOrder {
            request_generation: event_request_generation.next(),
            version_watermark: Some(version(12)),
        };
        assert_eq!(
            order.accept_command_response(version(12)),
            Some(order_command)
        );
        let order = order_command;

        assert_ne!(order.request_generation, event_request_generation);
        assert!(
            order
                .accept_query_response(event_request_generation, version(12))
                .is_none()
        );
    }

    #[test]
    fn out_of_order_events_keep_the_highest_version() {
        let order = ViewerShellOrder::default()
            .observe_event(version(15))
            .observe_event(version(13));

        assert_eq!(order.version_watermark, Some(version(15)));
        assert!(order.accept_command_response(version(14)).is_none());
    }

    #[test]
    fn accepted_shell_version_rejects_a_later_older_command() {
        let (order, request_generation) = ViewerShellOrder::default().start_request();
        let order_accepted = ViewerShellOrder {
            request_generation,
            version_watermark: Some(version(21)),
        };
        assert_eq!(
            order.accept_query_response(request_generation, version(21)),
            Some(order_accepted)
        );
        let order = order_accepted;

        assert!(order.accept_command_response(version(20)).is_none());
        assert_eq!(order.request_generation, request_generation);
        assert_eq!(order.version_watermark, Some(version(21)));
    }

    #[test]
    fn tab_close_feedback_never_becomes_a_toast() {
        assert_eq!(
            viewer_feedback_toast(Some(&ViewerFeedback::TabClosed)),
            None
        );
    }

    #[test]
    fn stale_row_stream_cleanup_preserves_the_current_loading_tab() -> TestResult {
        let mut loading = ViewerDiffRowsLoading::default();
        let first = viewer_tab_id(4)?;
        let current = viewer_tab_id(8)?;

        loading.set(first, true);
        loading.set(current, true);
        loading.set(first, false);

        assert_eq!(loading.tab_id(), Some(current));
        Ok(())
    }

    #[test]
    fn skipped_snapshot_feedback_keeps_every_label() {
        let feedback = ViewerFeedback::SnapshotRecipesSkipped {
            labels: vec!["api".to_owned(), "web".to_owned()],
        };

        assert_eq!(
            viewer_feedback_toast(Some(&feedback)),
            Some(ViewerFeedbackToast::Warn(
                "Skipped 2 diffs with no commits or changed files: api, web.".to_owned()
            ))
        );
    }
}
