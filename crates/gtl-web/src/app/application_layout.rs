use std::{cell::Cell, rc::Rc};

use dioxus::{
    core::{Task, spawn_forever},
    prelude::*,
};
use gtl_models::viewer::{ViewerTabId, ViewerVersion};
use gtl_wire::viewer::{
    ViewerActiveState, ViewerCommitSelection, ViewerFeedback, ViewerShell, ViewerTabRequest,
    ViewerTheme,
};

use crate::{
    app::{
        application_router::{Route, use_viewer_routes},
        displayed_language::DisplayedLanguage,
        window_header::WindowHeader,
    },
    entities::diffs::viewer_server,
    shared::{
        browser,
        failure_notice::{client_error_message, is_invalid_settings},
        i18n::{t, use_language},
        recipe_label::recipe_label_text,
        retry_delay::RetryDelay,
        ui::{Button, ButtonSize, ButtonVariant, ToastHandle, ToastHost, use_toast},
        viewer_client::{ViewerClientError, discard_viewer_connection},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewerShellLoad {
    Loading,
    Ready(ViewerShell),
    Error(ViewerClientError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewerConnection {
    Connecting,
    Connected,
    Retrying(ViewerClientError),
}

impl ViewerConnection {
    const fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ViewerShellOrder {
    request_generation: ViewerShellRequestGeneration,
    version_watermark: Option<ViewerVersion>,
}

#[derive(Clone, Copy, Default)]
struct ViewerShellRequests {
    order: ViewerShellOrder,
    task: Option<Task>,
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
    UpdateTab(ViewerTabRequest),
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

    #[cfg(test)]
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
    connection: Signal<ViewerConnection>,
    shell_requests: Signal<ViewerShellRequests>,
    reconnect_generation: Signal<u64>,
    server_instance_id: Signal<Option<String>>,
    render_state: Signal<ViewerRenderState>,
    toast: ToastHandle,
    live_errors: Signal<
        std::collections::HashMap<ViewerTabId, crate::entities::diffs::live_errors::LiveErrors>,
    >,
}

impl ViewerContext {
    pub(crate) fn close_tab(self, tab_id: ViewerTabId, focus: bool) {
        if !self.actions_enabled() {
            return;
        }
        spawn_forever(async move {
            self.close_tab_and_focus(tab_id, focus)
                .await
                .unwrap_or_else(|error| self.toast.client_error(&error));
        });
    }

    async fn close_tab_and_focus(
        self,
        tab_id: ViewerTabId,
        focus: bool,
    ) -> Result<(), ViewerClientError> {
        let shell = viewer_server::close_tab(gtl_wire::viewer::ViewerTabRequest { tab_id }).await?;
        let next = super::application_router::active_tab_id(&shell.active);
        self.replace_shell(shell);
        if focus {
            browser::focus_element(next.map_or_else(
                || "workspace-heading".to_owned(),
                crate::shared::ui::viewer_tab_element_id,
            ));
        }
        Ok(())
    }

    pub(crate) fn live_errors(
        self,
    ) -> ReadSignal<
        std::collections::HashMap<ViewerTabId, crate::entities::diffs::live_errors::LiveErrors>,
    > {
        self.live_errors.into()
    }

    fn observe_live_check(mut self, check: gtl_wire::viewer::ViewerLiveCheck) {
        self.live_errors
            .write()
            .entry(check.tab_id)
            .or_default()
            .observe(
                check.result.map_err(ViewerClientError::from),
                check.elapsed_ms,
            );
    }

    pub(crate) fn shell(self) -> ReadSignal<ViewerShellLoad> {
        self.shell.into()
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
        let Some(order) = self
            .shell_requests
            .peek()
            .order
            .accept_command_response(shell.version)
        else {
            return ViewerShellReplacement::Stale;
        };
        self.shell_requests.write().order = order;
        self.cancel_shell_request();
        if let Some(notification) =
            viewer_feedback_toast(shell.feedback.as_ref(), shell.preferences.language)
        {
            self.toast.warn(notification);
        }
        self.publish_shell(shell);
        ViewerShellReplacement::Accepted
    }

    fn cancel_shell_request(mut self) {
        let task = self.shell_requests.write().task.take();
        if let Some(task) = task {
            task.cancel();
        }
    }

    fn publish_shell(mut self, mut shell: ViewerShell) {
        if let ViewerShellLoad::Ready(previous) = &*self.shell.peek()
            && let ViewerActiveState::Ready { view: displayed } = &previous.active
            && let ViewerActiveState::Ready { view: incoming } = &mut shell.active
            && matches!(
                incoming.commit_selection,
                ViewerCommitSelection::Pending { .. }
            )
            && incoming.identity == displayed.identity
        {
            let mut retained = displayed.clone();
            retained.commit_selection = incoming.commit_selection.clone();
            *incoming = retained;
        }
        if try_consume_context::<crate::app::user_settings::UserSettings>()
            .is_none_or(|settings| settings.selection.peek().is_none())
        {
            DisplayedLanguage::show_configured(shell.preferences.language);
        }
        let next = ViewerShellLoad::Ready(shell);
        if *self.shell.peek() != next {
            self.shell.set(next);
        }
    }

    pub(crate) fn refresh_tab(self, tab_id: ViewerTabId) {
        self.schedule_render_command(ViewerRenderCommand::RefreshTab(ViewerTabRequest { tab_id }));
    }

    /// Recomputes the tab from its recipe's current revisions.
    pub(crate) fn update_tab(self, tab_id: ViewerTabId) {
        self.schedule_render_command(ViewerRenderCommand::UpdateTab(ViewerTabRequest { tab_id }));
    }

    fn schedule_render_command(mut self, command: ViewerRenderCommand) {
        if !self.actions_enabled() {
            self.toast.client_error(&ViewerClientError::Disconnected);
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
            Err(error) => self.toast.client_error(&error),
        }
        if let Some((ticket, command)) = next {
            self.start_render_command(ticket, command);
        }
    }

    pub(crate) fn diff_rows_loading_tab_id(self) -> Option<ViewerTabId> {
        (self.render_state)().diff_rows_loading.tab_id()
    }

    pub(crate) fn set_diff_rows_loading(mut self, tab_id: ViewerTabId, loading: bool) {
        let mut next = *self.render_state.peek();
        next.diff_rows_loading.set(tab_id, loading);
        if *self.render_state.peek() != next {
            self.render_state.set(next);
        }
    }

    fn connected_to(mut self, server_instance_id: String) -> bool {
        let server_changed = self
            .server_instance_id
            .peek()
            .as_ref()
            .is_some_and(|current| current != &server_instance_id);
        if server_changed {
            self.cancel_shell_request();
            self.shell_requests.write().order = ViewerShellOrder::default();
            self.shell.set(ViewerShellLoad::Loading);
            self.render_state.set(ViewerRenderState::default());
            self.live_errors.write().clear();
        }
        if self.server_instance_id.peek().as_ref() != Some(&server_instance_id) {
            self.server_instance_id.set(Some(server_instance_id));
        }
        if *self.connection.peek() != ViewerConnection::Connected {
            self.connection.set(ViewerConnection::Connected);
        }
        server_changed
    }

    fn disconnected(mut self, error: ViewerClientError) {
        DisplayedLanguage::show_default_when_unknown();
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
        if self.shell_requests.peek().task.is_some() {
            return;
        }
        let (order, request_generation) = self.shell_requests.peek().order.start_request();
        self.shell_requests.write().order = order;
        if show_loading {
            self.shell.set(ViewerShellLoad::Loading);
        }

        self.shell_requests.write().task = Some(spawn(refresh_shell(self, request_generation)));
    }

    fn invalidate(mut self, version: ViewerVersion) {
        let order = self.shell_requests.peek().order.observe_event(version);
        if self.shell_requests.peek().order != order {
            self.shell_requests.write().order = order;
        }
        let is_current = matches!(
            &*self.shell.peek(),
            ViewerShellLoad::Ready(shell) if !order.version_is_stale(shell.version)
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
    let result = match command {
        ViewerRenderCommand::RefreshTab(request) => viewer_server::refresh_tab(request).await,
        ViewerRenderCommand::UpdateTab(request) => viewer_server::update_tab(request).await,
    };
    context.complete_render_command(ticket, result);
}

async fn refresh_shell(
    mut context: ViewerContext,
    request_generation: ViewerShellRequestGeneration,
) {
    let result = viewer_server::get_shell().await;
    context.shell_requests.write().task = None;
    let order = context.shell_requests.peek().order;
    if !order.request_is_current(request_generation) {
        return;
    }
    match result {
        Ok(shell) => {
            let Some(order) = order.accept_query_response(request_generation, shell.version) else {
                context.refresh(false);
                return;
            };
            context.shell_requests.write().order = order;
            if let Some(notification) =
                viewer_feedback_toast(shell.feedback.as_ref(), shell.preferences.language)
            {
                context.toast.warn(notification);
            }
            context.publish_shell(shell);
        }
        Err(error) => {
            if !is_invalid_settings(&error)
                && matches!((context.shell)(), ViewerShellLoad::Ready(_))
            {
                context.toast.client_error(&error);
            } else {
                DisplayedLanguage::show_default_when_unknown();
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
    crate::entities::diffs::use_client_diff_cache_provider();
    crate::entities::diffs::use_commit_page_cache_provider();
    crate::views::diffs::project_diff::use_project_diff_destinations_provider();
    let shell = use_signal(|| ViewerShellLoad::Loading);
    let connection = use_signal(|| ViewerConnection::Connecting);
    let shell_requests = use_signal(ViewerShellRequests::default);
    let reconnect_generation = use_signal(|| 0_u64);
    let server_instance_id = use_signal(|| None::<String>);
    let render_state = use_signal(ViewerRenderState::default);
    let state_change_version = use_signal(|| None::<ViewerVersion>);
    let toast = use_toast();
    let mut live_errors = use_signal(std::collections::HashMap::new);
    let context = ViewerContext {
        shell,
        connection,
        shell_requests,
        reconnect_generation,
        server_instance_id,
        render_state,
        toast,
        live_errors,
    };
    let displayed_language = use_context::<DisplayedLanguage>();
    use_context_provider(|| context);
    let settings = crate::app::user_settings::use_user_settings_provider();
    crate::views::diffs::diff_workspace::sidebars::use_sidebar_controls_provider();
    let date_format = use_memo(move || {
        (settings.selection)().map_or_else(
            || match &*shell.read() {
                ViewerShellLoad::Ready(shell) => shell.preferences.date_format,
                _ => gtl_models::settings::ViewerDateFormat::default(),
            },
            |selection| selection.date_format,
        )
    });
    crate::shared::date_display::use_date_display_provider(date_format.into());
    crate::views::diffs::use_diff_presentation_provider();
    crate::views::diffs::file_filter_changes::use_file_filter_changes_provider();
    crate::views::projects::cache::use_status_cache_provider();
    crate::views::push::use_push_provider();
    use_viewer_routes(context);
    super::application_router::use_settings_navigation_provider();

    let visible = browser::use_document_visible();
    let route = use_route::<Route>();
    let request = use_memo(use_reactive((&route,), move |(route,)| {
        let live_tab_id = if visible() {
            match &*shell.read() {
                ViewerShellLoad::Ready(shell) => {
                    let active = crate::app::application_router::active_tab_id(&shell.active);
                    let displayed =
                        matches!(route, Route::CurrentDiff {}) || route.tab_id() == active;
                    active.filter(|id| {
                        displayed && shell.tabs.iter().any(|tab| tab.id == *id && tab.live)
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        gtl_wire::viewer::WatchViewer {
            live_tab_id,
            ..Default::default()
        }
    }));
    use_effect(move || {
        if let ViewerShellLoad::Ready(shell) = &*shell.read() {
            live_errors
                .write()
                .retain(|id, _| shell.tabs.iter().any(|tab| tab.id == *id));
        }
    });
    let mut state_changes = use_resource(move || {
        let request = request();
        for errors in live_errors.write().values_mut() {
            errors.interrupt();
        }
        async move {
            let mut retry_delay = RetryDelay::default();
            loop {
                let received_event = Rc::new(Cell::new(false));
                let event_received = Rc::clone(&received_event);
                let result = viewer_server::listen_for_state_changes(
                    request.clone(),
                    move |server_instance_id| {
                        if context.connected_to(server_instance_id) {
                            let mut version = state_change_version;
                            version.set(None);
                        }
                    },
                    move |event| {
                        event_received.set(true);
                        if let Some(check) = event.live_check {
                            context.observe_live_check(check);
                        }
                        let mut version = state_change_version;
                        let next = Some(
                            version
                                .peek()
                                .map_or(event.version, |current| current.max(event.version)),
                        );
                        if *version.peek() != next {
                            version.set(next);
                        }
                    },
                )
                .await;
                let error = result.err().unwrap_or(ViewerClientError::Disconnected);
                discard_viewer_connection();
                context.disconnected(error.clone());
                if let Some(tab_id) = request.live_tab_id {
                    live_errors
                        .write()
                        .entry(tab_id)
                        .or_default()
                        .observe(Err(error.clone()), 0);
                }
                if error == ViewerClientError::ProtocolMismatch {
                    return;
                }
                if received_event.get() {
                    retry_delay.reset();
                }
                dioxus_sdk_time::sleep(retry_delay.take_and_advance()).await;
            }
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
    let selected_theme =
        use_memo(move || (settings.selection)().map(|selected| selected.theme.unwrap_or_default()));
    let theme = selected_theme().unwrap_or_else(|| match &*state {
        ViewerShellLoad::Ready(shell) => shell.preferences.theme,
        _ => ViewerTheme::default(),
    });
    use_effect(use_reactive((&theme,), move |(theme,)| {
        browser::apply_theme(theme.as_str());
    }));
    // Only a loaded preference recolors the native icons, so startup does not flash the default.
    let icon_theme = selected_theme().or_else(|| match &*state {
        ViewerShellLoad::Ready(shell) => Some(shell.preferences.theme),
        ViewerShellLoad::Loading | ViewerShellLoad::Error(_) => None,
    });
    use_effect(use_reactive((&icon_theme,), move |(icon_theme,)| {
        let Some(theme) = icon_theme else {
            return;
        };
        spawn(async move {
            if let Err(error) = gtl_client::window::set_theme_icons(theme).await {
                context.toast.client_error(&error);
            }
        });
    }));

    let selected_accessibility =
        use_memo(move || (settings.selection)().map(|selected| selected.accessibility));
    let accessibility = selected_accessibility().unwrap_or_else(|| match &*state {
        ViewerShellLoad::Ready(shell) => shell.preferences.accessibility,
        _ => gtl_models::settings::ViewerAccessibility::default(),
    });
    let language = use_language();
    use_effect(use_reactive((&language,), move |(language,)| {
        let labels = gtl_wire::window::TrayLabels {
            show: t!(language, "tray-show"),
            quit: t!(language, "tray-quit"),
        };
        spawn(async move {
            if let Err(error) = gtl_client::window::set_tray_labels(labels).await {
                context.toast.client_error(&error);
            }
        });
    }));

    use_effect(use_reactive((&accessibility,), move |(accessibility,)| {
        browser::apply_reduced_motion(accessibility.reduce_motion);
        spawn(async move {
            if let Err(error) = gtl_client::window::set_scale(accessibility.ui_scale_percent).await
            {
                context.toast.client_error(&error);
            }
        });
    }));

    rsx! {
        div {
            class: "viewer-shell h-screen antialiased",
            class: if !displayed_language.is_known() { "invisible" },
            "data-theme": theme.as_str(),
            WindowHeader {}
            crate::views::push::PushDialogHost {}
            div {
                class: "viewer-connection-content",
                "inert": (!connection.is_connected()).then_some(""),
                aria_busy: (!connection.is_connected()).to_string(),
                div { class: "min-h-0 flex-1 overflow-hidden",
                    if matches!(&*state, ViewerShellLoad::Error(error) if is_invalid_settings(error)) {
                        crate::views::settings_recovery::SettingsRecovery { onretry: move |()| context.refresh(false) }
                    } else {
                        super::projects_host::ProjectsHost {}
                        Outlet::<Route> {}
                    }
                }
            }
            if !connection.is_connected() {
                ViewerConnectionNotice {
                    connection: connection.clone(),
                    onretry: move |()| context.reconnect(),
                }
            }
        }
    }
}

#[component]
fn ViewerConnectionNotice(connection: ViewerConnection, onretry: EventHandler<()>) -> Element {
    let language = use_language();
    let (message, can_retry) = match connection {
        ViewerConnection::Connecting => (t!(language, "connection-connecting"), false),
        ViewerConnection::Connected => return rsx! {},
        ViewerConnection::Retrying(error) => {
            let can_retry = error != ViewerClientError::ProtocolMismatch;
            let message = client_error_message(&error, language);
            let message = if can_retry {
                t!(language, "connection-retrying", message = message)
            } else {
                message
            };
            (message, can_retry)
        }
    };

    rsx! {
        div {
            class: "viewer-connection-notice mx-auto w-fit gap-3 px-4 py-2",
            role: if can_retry { "alert" } else { "status" },
            p { "{message}" }
            if can_retry {
                Button {
                    size: ButtonSize::Small,
                    variant: ButtonVariant::Outline,
                    onclick: move |_| onretry.call(()),
                    {t!(language, "connection-try-now")}
                }
            }
        }
    }
}

fn viewer_feedback_toast(
    feedback: Option<&ViewerFeedback>,
    language: gtl_models::settings::ViewerLanguage,
) -> Option<String> {
    match feedback? {
        ViewerFeedback::SnapshotRecipesSkipped { labels } => Some(t!(
            language,
            "feedback-snapshots-skipped-named",
            count = labels.len(),
            labels = labels
                .iter()
                .map(|label| recipe_label_text(label, language))
                .collect::<Vec<_>>()
                .join(", "),
        )),
    }
}

#[cfg(test)]
mod tests {
    use dioxus::{
        dioxus_core::{AttributeValue, Mutation, Mutations},
        prelude::*,
    };
    use gtl_models::viewer::ViewerVersion;
    use gtl_wire::viewer::{
        ViewerActiveState, ViewerFeedback, ViewerPreferences, ViewerRenderOptions, ViewerShell,
        ViewerTabRequest,
    };

    use super::{
        ViewerDiffRowsLoading, ViewerRenderCommand, ViewerRenderCommandCompletion,
        ViewerRenderCommandScheduler, ViewerRenderCommandSubmission, ViewerRenderCommandTicket,
        ViewerShellOrder, viewer_feedback_toast,
    };
    use crate::test_support::{TestResult, project_name, recipe_label, viewer_tab_id};

    #[component]
    fn DiffInteractionFixture(shell: Signal<super::ViewerShellLoad>) -> Element {
        rsx! {
            super::ToastHost {
                DiffInteractionContext { shell }
            }
        }
    }

    #[component]
    fn DiffInteractionContext(shell: Signal<super::ViewerShellLoad>) -> Element {
        let context = super::ViewerContext {
            shell,
            connection: use_signal(|| super::ViewerConnection::Connected),
            shell_requests: use_signal(super::ViewerShellRequests::default),
            reconnect_generation: use_signal(|| 0),
            server_instance_id: use_signal(|| Some("test-server".to_owned())),
            render_state: use_signal(super::ViewerRenderState::default),
            toast: super::use_toast(),
            live_errors: use_signal(std::collections::HashMap::new),
        };
        use_context_provider(|| context);
        rsx! {
            crate::views::diffs::DiffWorkspaceView { tab_id: Some(viewer_tab_id(1).unwrap()) }
        }
    }

    fn pending_shell(tab_id: u64) -> TestResult<super::ViewerShellLoad> {
        Ok(super::ViewerShellLoad::Ready(ViewerShell {
            version: ViewerVersion::default(),
            focus_request_version: None,
            tabs: [1, 2]
                .into_iter()
                .map(|id| {
                    Ok(gtl_wire::viewer::ViewerTab {
                        details: None,
                        custom_name: None,
                        pinned: false,
                        id: viewer_tab_id(id)?,
                        label: recipe_label(&format!("Diff {id}"))?,
                        live: false,
                        state: gtl_wire::viewer::ViewerTabState::Pending,
                    })
                })
                .collect::<TestResult<_>>()?,
            active: ViewerActiveState::Pending {
                tab_id: viewer_tab_id(tab_id)?,
            },
            preferences: ViewerPreferences {
                copy_with_line_context: true,
                accessibility: gtl_models::settings::ViewerAccessibility::default(),
                language: gtl_models::settings::ViewerLanguage::default(),
                date_format: gtl_models::settings::ViewerDateFormat::default(),
                sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
                theme: super::ViewerTheme::Dark,
                render_options: ViewerRenderOptions {
                    wrap_lines: false,
                    layout: gtl_wire::viewer::ViewerDiffLayout::Unified,
                    density: gtl_wire::viewer::ViewerDiffDensity::Compact,
                },
                keybindings: gtl_models::viewer::ViewerKeybindings::default(),
            },
            feedback: None,
        }))
    }

    fn inert_changes(mutations: &Mutations) -> Vec<&AttributeValue> {
        mutations
            .edits
            .iter()
            .filter_map(|edit| match edit {
                Mutation::SetAttribute {
                    name: "inert",
                    value,
                    ..
                } => Some(value),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn diff_workspace_removes_inert_after_tab_activation() -> TestResult {
        let owner = VirtualDom::new(VNode::empty);
        let initial = pending_shell(1)?;
        let mut shell = owner.in_scope(ScopeId::ROOT, || Signal::new(initial));
        let mut dom = VirtualDom::new_with_props(
            DiffInteractionFixture,
            DiffInteractionFixtureProps { shell },
        );
        let mut mutations = Mutations::default();
        dom.rebuild(&mut mutations);
        assert!(
            inert_changes(&mutations)
                .iter()
                .all(|value| matches!(value, AttributeValue::None)),
            "a matching diff must omit inert, not write inert=false"
        );

        mutations.edits.clear();
        shell.set(pending_shell(2)?);
        dom.render_immediate(&mut mutations);
        assert!(
            matches!(inert_changes(&mutations).as_slice(), [AttributeValue::Text(value)] if value.is_empty())
        );

        mutations.edits.clear();
        shell.set(pending_shell(1)?);
        dom.render_immediate(&mut mutations);
        assert!(
            matches!(inert_changes(&mutations).as_slice(), [AttributeValue::None]),
            "finishing activation must remove inert so pointer, focus, selection, and scroll work"
        );
        Ok(())
    }

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
    fn skipped_snapshot_feedback_localizes_every_label() -> TestResult {
        let feedback = ViewerFeedback::SnapshotRecipesSkipped {
            labels: vec![
                gtl_models::recipes::RecipeLabel::Changes {
                    repository: project_name("api")?,
                    changes: gtl_models::recipes::RecipeLabelChanges::Unpushed,
                },
                recipe_label("web")?,
            ],
        };

        assert_eq!(
            viewer_feedback_toast(Some(&feedback), gtl_models::settings::ViewerLanguage::EnUs),
            Some("Skipped 2 diffs with no commits or changed files: api: diff, web.".to_owned())
        );
        Ok(())
    }
}
