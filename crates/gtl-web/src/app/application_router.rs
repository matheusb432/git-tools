use dioxus::{prelude::*, router::Navigator};
use gtl_models::{
    live_views::LiveComparison,
    paths::RepositoryRoot,
    viewer::{ViewerTabId, ViewerVersion},
};
use gtl_wire::viewer::{ViewerActiveState, ViewerShell, ViewerTabRequest};

use crate::{
    app::application_layout::{ApplicationLayout, ViewerContext, ViewerShellLoad},
    entities::diffs::viewer_server,
    shared::{
        browser,
        ui::{ToastHandle, use_toast},
    },
    views::{DiffHistoryView, DiffWorkspaceView, UserSettingsView},
};

#[derive(Debug, Clone, Routable, PartialEq, Eq)]
#[rustfmt::skip]
pub(crate) enum Route {
    #[layout(ApplicationLayout)]
        #[redirect("/", || Route::Projects {})]
        #[route("/projects")]
        Projects {},
        #[route("/projects/local?:..query")]
        ProjectLocalDiff { query: ProjectDiffQuery },
        #[route("/projects/unpushed?:..query")]
        ProjectUnpushedDiff { query: ProjectDiffQuery },
        #[route("/diffs")]
        CurrentDiff {},
        #[route("/diffs/:tab_id")]
        Diff { tab_id: ViewerTabId },
        #[route("/history")]
        History {},
        #[route("/settings")]
        Settings {},
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ProjectDiffQuery(Option<RepositoryRoot>);

impl std::fmt::Display for ProjectDiffQuery {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use dioxus::router::exports::percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
        if let Some(path) = &self.0 {
            write!(
                formatter,
                "path={}",
                utf8_percent_encode(&path.to_string(), NON_ALPHANUMERIC)
            )?;
        }
        Ok(())
    }
}

impl From<&str> for ProjectDiffQuery {
    fn from(query: &str) -> Self {
        // Dioxus decodes the entire query before invoking this parser.
        Self(
            query
                .strip_prefix("path=")
                .and_then(|path| RepositoryRoot::try_new(path.into()).ok()),
        )
    }
}

impl Route {
    pub(crate) fn project_diff(path: &RepositoryRoot, comparison: LiveComparison) -> Self {
        let query = ProjectDiffQuery(Some(path.clone()));
        match comparison {
            LiveComparison::LocalChanges => Self::ProjectLocalDiff { query },
            LiveComparison::UnpushedCommits => Self::ProjectUnpushedDiff { query },
        }
    }

    fn for_active(active: &ViewerActiveState) -> Self {
        active_tab_id(active).map_or(Self::Projects {}, |tab_id| Self::Diff { tab_id })
    }

    pub(super) const fn tab_id(&self) -> Option<ViewerTabId> {
        match self {
            Self::Diff { tab_id } => Some(*tab_id),
            Self::Projects {}
            | Self::CurrentDiff {}
            | Self::History {}
            | Self::Settings {}
            | Self::ProjectLocalDiff { .. }
            | Self::ProjectUnpushedDiff { .. } => None,
        }
    }
}

pub(crate) const fn active_tab_id(active: &ViewerActiveState) -> Option<ViewerTabId> {
    match active {
        ViewerActiveState::Empty => None,
        ViewerActiveState::Pending { tab_id }
        | ViewerActiveState::Broken { tab_id, .. }
        | ViewerActiveState::Error { tab_id, .. } => Some(*tab_id),
        ViewerActiveState::Ready { view } => Some(view.identity.tab_id),
    }
}

#[derive(Clone, Default, PartialEq, Eq)]
struct ViewerRouteObservation {
    server_instance_id: Option<String>,
    focus_request_version: Option<ViewerVersion>,
}

#[derive(Debug, PartialEq, Eq)]
enum ViewerRouteAction {
    None,
    Replace(Route),
    Focus(Route),
    Activate {
        tab_id: ViewerTabId,
        fallback: Route,
    },
}

impl ViewerRouteObservation {
    fn next(
        &self,
        server_instance_id: String,
        route: &Route,
        shell: &ViewerShell,
    ) -> (Self, ViewerRouteAction) {
        let server_changed = self
            .server_instance_id
            .as_ref()
            .is_some_and(|previous| previous != &server_instance_id);
        let focus_requested = shell.focus_request_version.is_some()
            && self.server_instance_id.is_some()
            && (server_changed || self.focus_request_version != shell.focus_request_version);
        let next = Self {
            server_instance_id: Some(server_instance_id),
            focus_request_version: shell.focus_request_version,
        };
        let active_route = Route::for_active(&shell.active);
        // The project route resolves its own open response into the canonical tab URL.
        if matches!(
            route,
            Route::ProjectLocalDiff { .. } | Route::ProjectUnpushedDiff { .. }
        ) {
            return (next, ViewerRouteAction::None);
        }
        if focus_requested {
            return (next, ViewerRouteAction::Focus(active_route));
        }
        let action = match route {
            Route::CurrentDiff {} => ViewerRouteAction::Replace(active_route),
            Route::Diff { tab_id }
                if server_changed || !shell.tabs.iter().any(|tab| tab.id == *tab_id) =>
            {
                ViewerRouteAction::Replace(active_route)
            }
            Route::Diff { tab_id } if *route != active_route => ViewerRouteAction::Activate {
                tab_id: *tab_id,
                fallback: active_route,
            },
            Route::Projects {}
            | Route::Diff { .. }
            | Route::History {}
            | Route::Settings {}
            | Route::ProjectLocalDiff { .. }
            | Route::ProjectUnpushedDiff { .. } => ViewerRouteAction::None,
        };
        (next, action)
    }
}

pub(super) fn use_viewer_routes(viewer: ViewerContext) {
    let route = use_route::<Route>();
    let navigator = use_navigator();
    let toast = use_toast();
    let observation = use_signal(ViewerRouteObservation::default);
    use_resource(use_reactive((&route,), move |(route,)| {
        let shell = viewer.shell();
        let shell = shell.read();
        let server_instance_id = viewer.server_instance_id();
        let route_transition = match (&*shell, server_instance_id) {
            (ViewerShellLoad::Ready(shell), Some(server_instance_id))
                if viewer.actions_enabled() =>
            {
                Some(observation.peek().next(server_instance_id, &route, shell))
            }
            _ => None,
        };
        apply_viewer_route(
            route_transition,
            observation,
            route,
            viewer,
            navigator,
            toast,
        )
    }));
}

async fn apply_viewer_route(
    route_transition: Option<(ViewerRouteObservation, ViewerRouteAction)>,
    mut observation: Signal<ViewerRouteObservation>,
    route: Route,
    viewer: ViewerContext,
    navigator: Navigator,
    toast: ToastHandle,
) {
    let Some((next, action)) = route_transition else {
        return;
    };
    if *observation.peek() != next {
        observation.set(next);
    }
    match action {
        ViewerRouteAction::None => {}
        ViewerRouteAction::Replace(target) => {
            let focus_workspace = matches!(target, Route::Diff { .. });
            navigator.replace(target);
            if focus_workspace {
                browser::focus_element("workspace-heading".to_owned());
            }
        }
        ViewerRouteAction::Focus(target) => {
            if route != target {
                navigator.push(target);
            }
            browser::focus_element("workspace-heading".to_owned());
        }
        ViewerRouteAction::Activate { tab_id, fallback } => {
            match viewer_server::activate_tab(ViewerTabRequest { tab_id }).await {
                Ok(shell) => viewer.replace_shell(shell),
                Err(error) => {
                    toast.error(error.message());
                    navigator.replace(fallback);
                }
            }
        }
    }
}

#[component]
fn Projects() -> Element {
    rsx! {
        crate::views::projects::ProjectsView {}
    }
}

#[component]
fn ProjectLocalDiff(query: ProjectDiffQuery) -> Element {
    rsx! {
        crate::views::diffs::ProjectDiffView { path: query.0, comparison: LiveComparison::LocalChanges }
    }
}

#[component]
fn ProjectUnpushedDiff(query: ProjectDiffQuery) -> Element {
    rsx! {
        crate::views::diffs::ProjectDiffView { path: query.0, comparison: LiveComparison::UnpushedCommits }
    }
}

#[component]
fn CurrentDiff() -> Element {
    rsx! {}
}

#[component]
fn Diff(tab_id: ViewerTabId) -> Element {
    rsx! {
        DiffWorkspaceView { tab_id }
    }
}

#[component]
fn History() -> Element {
    rsx! {
        DiffHistoryView {}
    }
}

#[component]
fn Settings() -> Element {
    rsx! {
        UserSettingsView {}
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::{
        ViewerPreferences, ViewerRenderOptions, ViewerTab, ViewerTabKind, ViewerTabState,
        ViewerTheme,
    };

    use super::*;
    use crate::test_support::{TestResult, viewer_tab_id};

    fn shell(
        active: Option<ViewerTabId>,
        focus_request_version: Option<ViewerVersion>,
    ) -> ViewerShell {
        ViewerShell {
            version: ViewerVersion::new(10),
            focus_request_version,
            tabs: active
                .into_iter()
                .map(|id| ViewerTab {
                    id,
                    label: "diff".to_owned(),
                    kind: ViewerTabKind::Snapshot,
                    state: ViewerTabState::Pending,
                })
                .collect(),
            active: active.map_or(ViewerActiveState::Empty, |tab_id| {
                ViewerActiveState::Pending { tab_id }
            }),
            preferences: ViewerPreferences {
                sidebars: gtl_models::viewer::ViewerSidebarVisibility::default(),
                theme: ViewerTheme::Dark,
                render_options: ViewerRenderOptions {
                    wrap_lines: false,
                    layout: gtl_wire::viewer::ViewerDiffLayout::Split,
                    density: gtl_wire::viewer::ViewerDiffDensity::Full,
                },
                keybindings: gtl_models::viewer::ViewerKeybindings::default(),
            },
            feedback: None,
        }
    }

    #[test]
    fn projects_home_only_leaves_for_an_explicit_diff_open() -> TestResult {
        let tab_id = viewer_tab_id(7)?;
        let mut shell = shell(Some(tab_id), None);
        let (observed, action) = ViewerRouteObservation::default().next(
            "server".to_owned(),
            &Route::Projects {},
            &shell,
        );
        assert_eq!(action, ViewerRouteAction::None);
        shell.focus_request_version = Some(ViewerVersion::new(1));
        let (observed, action) = observed.next("server".to_owned(), &Route::Projects {}, &shell);
        assert_eq!(action, ViewerRouteAction::Focus(Route::Diff { tab_id }));
        let (_, action) = observed.next("server".to_owned(), &Route::Projects {}, &shell);
        assert_eq!(action, ViewerRouteAction::None);
        let (_, action) = ViewerRouteObservation::default().next(
            "server".to_owned(),
            &Route::Projects {},
            &shell,
        );
        assert_eq!(action, ViewerRouteAction::None);
        let (_, action) = ViewerRouteObservation::default().next(
            "server".to_owned(),
            &Route::CurrentDiff {},
            &shell,
        );
        assert_eq!(action, ViewerRouteAction::Replace(Route::Diff { tab_id }));
        Ok(())
    }

    #[test]
    fn projects_has_its_own_route_without_a_selected_diff() {
        assert_eq!(Route::Projects {}.to_string(), "/projects");
        assert_eq!("/".parse::<Route>().ok(), Some(Route::Projects {}));
        assert_eq!("/projects".parse::<Route>().ok(), Some(Route::Projects {}));
        for route in [Route::Projects {}, Route::History {}, Route::Settings {}] {
            assert_eq!(route.tab_id(), None);
        }
    }

    #[test]
    fn diff_routes_round_trip_validated_tab_ids() -> TestResult {
        let route = Route::Diff {
            tab_id: viewer_tab_id(7)?,
        };
        assert_eq!(route.to_string(), "/diffs/7");
        assert_eq!("/diffs/7".parse::<Route>().ok(), Some(route));
        assert!("/diffs/0".parse::<Route>().is_err());
        assert!("/diffs/invalid".parse::<Route>().is_err());
        Ok(())
    }

    #[test]
    fn project_links_round_trip_paths_and_resolve_their_own_open() -> TestResult {
        let path = RepositoryRoot::try_new("/tmp/project with spaces & # + %20 ?/repo".into())?;
        let tab_id = viewer_tab_id(7)?;
        let initial = shell(Some(tab_id), None);
        let (observed, _) = ViewerRouteObservation::default().next(
            "server".to_owned(),
            &Route::Projects {},
            &initial,
        );
        for comparison in [
            LiveComparison::LocalChanges,
            LiveComparison::UnpushedCommits,
        ] {
            let route = Route::project_diff(&path, comparison);
            assert_eq!(route.to_string().parse::<Route>().ok(), Some(route.clone()));
            let (_, action) = observed.next(
                "server".to_owned(),
                &route,
                &shell(Some(tab_id), Some(ViewerVersion::new(1))),
            );
            assert_eq!(action, ViewerRouteAction::None);
        }
        Ok(())
    }

    #[test]
    fn repeated_cli_open_leaves_settings_even_when_the_active_tab_is_unchanged() -> TestResult {
        let tab_id = viewer_tab_id(7)?;
        let mut shell = shell(Some(tab_id), Some(ViewerVersion::new(1)));
        let (observed, _) = ViewerRouteObservation::default().next(
            "server".to_owned(),
            &Route::Projects {},
            &shell,
        );
        shell.focus_request_version = Some(ViewerVersion::new(2));
        let (observed, action) = observed.next("server".to_owned(), &Route::Settings {}, &shell);
        assert_eq!(action, ViewerRouteAction::Focus(Route::Diff { tab_id }));

        shell.version = shell.version.next();
        for route in [Route::Settings {}, Route::History {}] {
            let (_, action) = observed.next("server".to_owned(), &route, &shell);
            assert_eq!(action, ViewerRouteAction::None);
        }
        Ok(())
    }

    #[test]
    fn initial_direct_route_and_back_navigation_activate_the_requested_tab() -> TestResult {
        let first = viewer_tab_id(4)?;
        let second = viewer_tab_id(8)?;
        let mut shell = shell(Some(second), Some(ViewerVersion::new(1)));
        shell.tabs.push(ViewerTab {
            id: first,
            label: "first".to_owned(),
            kind: ViewerTabKind::Snapshot,
            state: ViewerTabState::Ready,
        });
        let (observed, action) = ViewerRouteObservation::default().next(
            "server".to_owned(),
            &Route::Diff { tab_id: first },
            &shell,
        );
        let expected = ViewerRouteAction::Activate {
            tab_id: first,
            fallback: Route::Diff { tab_id: second },
        };
        assert_eq!(action, expected);
        let (_, action) =
            observed.next("server".to_owned(), &Route::Diff { tab_id: first }, &shell);
        assert_eq!(action, expected);
        Ok(())
    }

    #[test]
    fn closed_tabs_and_server_replacement_resolve_to_the_current_workspace() -> TestResult {
        let first = viewer_tab_id(4)?;
        let second = viewer_tab_id(8)?;
        let shell = shell(Some(second), None);
        let (observed, action) = ViewerRouteObservation::default().next(
            "old".to_owned(),
            &Route::Diff { tab_id: first },
            &shell,
        );
        assert_eq!(
            action,
            ViewerRouteAction::Replace(Route::Diff { tab_id: second })
        );
        let (_, action) = observed.next("new".to_owned(), &Route::Diff { tab_id: second }, &shell);
        assert_eq!(
            action,
            ViewerRouteAction::Replace(Route::Diff { tab_id: second })
        );
        let (_, action) = observed.next("new".to_owned(), &Route::Settings {}, &shell);
        assert_eq!(action, ViewerRouteAction::None);

        let empty = self::shell(None, None);
        let (_, action) = observed.next("old".to_owned(), &Route::Diff { tab_id: second }, &empty);
        assert_eq!(action, ViewerRouteAction::Replace(Route::Projects {}));
        Ok(())
    }
}
