mod card;
mod comparison_action;
mod comparison_editor;
mod loading;
mod motion;
mod presentation;
mod status;
mod table;
mod view_mode;

use dioxus::prelude::*;
use gtl_models::settings::ProjectsViewMode;
use lucide_dioxus::RefreshCw;

use self::{
    card::ProjectCard,
    loading::use_projects,
    presentation::ProjectsSummary,
    table::ProjectTable,
    view_mode::{ProjectsViewToggle, use_projects_presentation},
};
use crate::{
    app::application_layout::ViewerContext,
    shared::{
        browser,
        ui::{Button, ButtonSize, ButtonState, ButtonVariant, PageNotice, ScrollArea, Skeleton},
    },
};

const PROJECT_GRID_CLASSES: &str = "projects-grid gap-4";

#[component]
pub(crate) fn ProjectsView() -> Element {
    let projects = use_projects();
    use_context_provider(|| projects);
    let presentation = use_projects_presentation();
    let viewer = use_context::<ViewerContext>();
    let try_again = use_callback(move |()| {
        if (presentation.error)().is_some() {
            (presentation.retry)(());
        } else {
            (projects.refresh)(());
        }
    });
    use_effect(move || browser::focus_element("projects-heading".into()));
    let load = projects.load.read();
    let mode = (presentation.mode)();
    let disabled = !viewer.actions_enabled();
    rsx! {
        document::Title { "Projects - git-tools" }
        main {
            class: "projects-shell h-full min-h-0",
            "data-testid": "projects-view",
            header { class: "projects-header h-14 gap-4 px-4 sm:px-6",
                h1 {
                    id: "projects-heading",
                    tabindex: "-1",
                    class: "projects-title text-xl font-semibold tracking-tight",
                    "Projects"
                }
                ProjectsSummary { load: projects.load }
                div { class: "projects-header-actions ml-auto gap-3",
                    ProjectsViewToggle { presentation }
                    Button {
                        variant: ButtonVariant::Outline,
                        state: if load.refreshing { ButtonState::Loading } else if viewer.actions_enabled() { ButtonState::Enabled } else { ButtonState::Disabled },
                        onclick: move |_| (projects.refresh)(()),
                        aria_label: "Refresh projects",
                        icon: rsx! {
                            RefreshCw { size: 14 }
                        },
                        span { class: "hidden sm:inline", "Refresh" }
                    }
                }
            }
            ScrollArea {
                class: "projects-content min-h-0 p-4 sm:p-6",
                "data-testid": "projects-content",
                div { class: "mx-auto max-w-7xl",
                    if let Some(error) = load.error.or((presentation.error)()) {
                        div {
                            class: "projects-error mb-4 gap-3 px-3 py-2",
                            role: "alert",
                            span { class: "min-w-0",
                                "{error.message()} "
                                if load.projects.is_some() {
                                    "Showing the last known status."
                                }
                            }
                            Button {
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Small,
                                class: "ml-auto text-warn hover:text-ink active:text-ink",
                                onclick: move |_| try_again(()),
                                "Try again"
                            }
                        }
                    }
                    match &load.projects {
                        None if load.error.is_some() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "Projects unavailable",
                                message: "Check the project catalogue and try Refresh.",
                            }
                        },
                        None => rsx! {
                            div { class: PROJECT_GRID_CLASSES, aria_label: "Loading projects",
                                for index in 0..6 {
                                    Skeleton { key: "{index}", class: "h-52 rounded-panel" }
                                }
                            }
                        },
                        Some(items) if items.is_empty() => rsx! {
                            PageNotice {
                                class: "min-h-64",
                                title: "No managed projects",
                                message: "Projects managed in Git Tools appear here.",
                            }
                        },
                        Some(items) if mode == ProjectsViewMode::Table => rsx! {
                            ProjectTable { projects: items.clone(), disabled }
                        },
                        Some(items) => rsx! {
                            div { class: PROJECT_GRID_CLASSES, aria_label: "Managed projects",
                                for project in items {
                                    ProjectCard { key: "{project.path}", project: project.clone(), disabled }
                                }
                            }
                        },
                    }
                }
            }
        }
    }
}
