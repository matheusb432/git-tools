use dioxus::prelude::*;
use lucide_dioxus::{GitCompareArrows, History, Settings};

use super::application_router::Route;

const NAVIGATION_LINK_CLASSES: &str = "inline-flex h-8 items-center gap-2 rounded-sm border border-transparent px-3 text-xs font-semibold text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";
const NAVIGATION_LINK_ACTIVE_CLASSES: &str = "inline-flex h-8 items-center gap-2 rounded-sm border border-acc-line bg-acc-soft px-3 text-xs font-semibold text-acc focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";

#[component]
pub(crate) fn ApplicationNavigation() -> Element {
    let route = use_route::<Route>();
    let workspace_active = matches!(route, Route::Workspace {});
    let history_active = matches!(route, Route::History {});
    let settings_active = matches!(route, Route::Settings {});

    rsx! {
        header { class: "shrink-0 border-b border-line bg-bg",
            div { class: "flex min-h-12 items-center gap-3 px-3 sm:px-4",
                Link {
                    class: "flex min-w-0 items-center gap-2 rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc",
                    to: Route::Workspace {},
                    aria_label: "git-tools viewer",
                    span { class: "grid size-7 shrink-0 place-items-center rounded-sm border border-acc-line bg-acc-soft font-mono text-xs font-bold text-acc", "+/-" }
                    span { class: "hidden min-w-0 sm:block",
                        span { class: "block truncate text-xs font-semibold text-ink", "git-tools" }
                        span { class: "block truncate font-mono text-[9px] uppercase tracking-[0.14em] text-ink-3", "Viewer" }
                    }
                }

                nav { class: "ml-2 flex min-w-0 flex-1 items-center gap-1", aria_label: "Application",
                    Link {
                        class: if workspace_active { NAVIGATION_LINK_ACTIVE_CLASSES } else { NAVIGATION_LINK_CLASSES },
                        to: Route::Workspace {},
                        aria_current: workspace_active.then_some("page"),
                        span { aria_hidden: "true", GitCompareArrows { size: 15 } }
                        "Viewer"
                    }
                    Link {
                        class: if history_active { NAVIGATION_LINK_ACTIVE_CLASSES } else { NAVIGATION_LINK_CLASSES },
                        to: Route::History {},
                        aria_current: history_active.then_some("page"),
                        span { aria_hidden: "true", History { size: 15 } }
                        "History"
                    }
                }

                Link {
                    class: if settings_active { NAVIGATION_LINK_ACTIVE_CLASSES } else { NAVIGATION_LINK_CLASSES },
                    to: Route::Settings {},
                    aria_current: settings_active.then_some("page"),
                    aria_label: "User settings",
                    title: "User settings",
                    span { aria_hidden: "true", Settings { size: 16 } }
                    span { class: "hidden md:inline", "Settings" }
                }
            }
            div { class: "h-px bg-acc", aria_hidden: "true" }
        }
    }
}
