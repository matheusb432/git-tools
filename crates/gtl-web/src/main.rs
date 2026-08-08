use dioxus::prelude::*;

mod shared;

use shared::ui::{
    Badge, BadgeVariant, Button, ButtonSize, ButtonState, ButtonVariant, Separator,
    SeparatorOrientation, Skeleton, TextInput,
};

const FAVICON: Asset = asset!("/src/app/assets/app-icon.ico");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const BRIDGE_JS: Asset = asset!("/assets/generated/dioxus-poc.js");

#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {
    #[layout(Shell)]
        #[route("/")]
        Workbench {},
        #[route("/boundary")]
        Boundary {},
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: BRIDGE_JS }
        Router::<Route> {}
    }
}

#[component]
fn Shell() -> Element {
    rsx! {
        div { class: "min-h-screen bg-bg text-ink antialiased",
            header { class: "sticky top-0 z-20 border-b border-line bg-bg/90 backdrop-blur-xl",
                div { class: "mx-auto flex max-w-[1560px] items-center justify-between gap-6 px-5 py-3 lg:px-8",
                    div { class: "flex min-w-0 items-center gap-3",
                        div { class: "grid size-9 shrink-0 place-items-center rounded-panel border border-acc-line bg-acc-soft font-mono text-sm font-bold text-acc", "±" }
                        div { class: "min-w-0",
                            p { class: "truncate text-sm font-semibold tracking-tight text-ink", "git-tools viewer" }
                            p { class: "truncate font-mono text-[10px] uppercase tracking-[0.16em] text-ink-2", "Dioxus Web proof" }
                        }
                    }
                    nav { class: "flex items-center gap-1 rounded-panel border border-line bg-surface/80 p-1", aria_label: "Proof routes",
                        Link { class: "rounded-sm px-3 py-1.5 text-xs font-medium text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc", to: Route::Workbench {}, "Workbench" }
                        Link { class: "rounded-sm px-3 py-1.5 text-xs font-medium text-ink-2 hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc", to: Route::Boundary {}, "Boundary" }
                    }
                }
            }
            Outlet::<Route> {}
        }
    }
}

#[component]
fn Workbench() -> Element {
    let mut refresh_revision = use_signal(|| 0_u64);
    let revision = refresh_revision();

    rsx! {
        main { class: "mx-auto grid max-w-[1560px] gap-5 px-5 py-5 lg:px-8",
            section { class: "flex flex-col justify-between gap-4 rounded-panel border border-line bg-surface px-5 py-4 shadow-[0_18px_50px_rgba(0,0,0,0.22)] md:flex-row md:items-center",
                div { class: "max-w-3xl",
                    div { class: "mb-2 flex flex-wrap items-center gap-2",
                        span { class: "rounded-full border border-acc-line bg-acc-soft px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.14em] text-acc", "Shell · Dioxus" }
                        span { class: "rounded-full border border-add-line bg-add-bg px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.14em] text-add", "Diff · Rust/Maud" }
                    }
                    h1 { class: "text-xl font-semibold tracking-[-0.025em] text-ink md:text-2xl", "A reactive shell around the proven renderer" }
                    p { class: "mt-1.5 max-w-2xl text-sm leading-6 text-ink-2", "Dioxus owns the navigable shell. The existing backend still renders the diff subtree, which the bridge mounts into an isolated shadow root." }
                }
                Button {
                    id: "dioxus-refresh-diff",
                    size: ButtonSize::Large,
                    onclick: move |_| *refresh_revision.write() += 1,
                    "Replace diff"
                    span { class: "rounded-sm bg-black/15 px-1.5 py-0.5 font-mono text-[10px]", "r{revision}" }
                }
            }

            section { class: "min-h-0 overflow-hidden rounded-panel border border-line bg-surface shadow-[0_24px_80px_rgba(0,0,0,0.3)]",
                header { class: "flex items-center justify-between gap-4 border-b border-line bg-surface-2/70 px-4 py-2.5",
                    div { class: "flex items-center gap-2",
                        span { class: "size-2 rounded-full bg-add shadow-[0_0_16px_rgba(63,185,80,0.65)]" }
                        h2 { class: "text-xs font-semibold text-ink", "Backend-rendered diff island" }
                    }
                    code { class: "font-mono text-[10px] text-ink-3", "#dioxus-diff-island → shadowRoot" }
                }
                div {
                    id: "dioxus-diff-island",
                    class: "block h-[min(68vh,760px)] min-h-[400px] min-w-0",
                    aria_label: "Rust-rendered diff",
                    "data-refresh-revision": revision.to_string(),
                    "data-theme": "dark",
                }
            }
        }
    }
}

#[component]
fn Boundary() -> Element {
    rsx! {
        main { class: "mx-auto max-w-5xl px-5 py-8 lg:px-8",
            div { class: "grid gap-5 md:grid-cols-2",
                OwnershipCard {
                    eyebrow: "Reactive shell",
                    title: "Dioxus Web",
                    detail: "Owns routing, component composition, and transient presentation state. RSX and Rust logic hot reload through the Dioxus dev server inside this Tauri window.",
                    accent: "border-acc-line bg-acc-soft",
                }
                OwnershipCard {
                    eyebrow: "Performance island",
                    title: "Existing Rust renderer",
                    detail: "Owns every diff row and returns the same real viewer fragment. The bridge replaces one shadow-root subtree atomically; Dioxus never reconciles those nodes.",
                    accent: "border-add-line bg-add-bg",
                }
            }
            div { class: "mt-5 rounded-panel border border-line bg-surface p-5",
                h1 { class: "text-lg font-semibold text-ink", "Proof boundary" }
                p { class: "mt-2 text-sm leading-6 text-ink-2", "This crate is a removable integration experiment. It does not migrate the production HTMX shell, package a release frontend, or redefine backend/session ownership." }
                Link { class: "mt-5 inline-flex rounded-sm border border-line-2 bg-surface-2 px-4 py-2 text-sm font-medium text-ink hover:border-acc-line hover:text-acc focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc", to: Route::Workbench {}, "Return to workbench" }
            }
            SharedUiFoundations {}
        }
    }
}

#[component]
fn OwnershipCard(eyebrow: String, title: String, detail: String, accent: String) -> Element {
    rsx! {
        article { class: "rounded-panel border p-5 {accent}",
            p { class: "font-mono text-[10px] uppercase tracking-[0.16em] text-ink-3", "{eyebrow}" }
            h2 { class: "mt-2 text-xl font-semibold tracking-tight text-ink", "{title}" }
            p { class: "mt-2 text-sm leading-6 text-ink-2", "{detail}" }
        }
    }
}

#[component]
fn SharedUiFoundations() -> Element {
    let mut filter_value = use_signal(String::new);
    let filter_preview = filter_value();

    rsx! {
        section { class: "mt-8 overflow-hidden rounded-panel border border-line bg-surface",
            header { class: "flex flex-col gap-2 border-b border-line bg-surface-2 px-5 py-4 sm:flex-row sm:items-end sm:justify-between",
                div {
                    p { class: "font-mono text-[10px] font-semibold uppercase tracking-[0.16em] text-acc", "Shared UI" }
                    h1 { class: "mt-1 text-xl font-semibold tracking-tight text-ink", "Foundational components" }
                }
                p { class: "max-w-xl text-sm leading-6 text-ink-2", "A compact inspection surface for the controls used throughout the viewer shell." }
            }

            div { class: "grid gap-8 p-5 sm:p-6",
                section { aria_labelledby: "button-foundations",
                    h2 { id: "button-foundations", class: "text-sm font-semibold text-ink", "Buttons" }
                    p { class: "mt-1 text-xs leading-5 text-ink-2", "Variants, size rhythm, and request states share one focus and interaction contract." }

                    div { class: "mt-4 flex flex-wrap items-center gap-3",
                        Button { variant: ButtonVariant::Primary, "Primary" }
                        Button { variant: ButtonVariant::Secondary, "Secondary" }
                        Button { variant: ButtonVariant::Destructive, "Destructive" }
                        Button { variant: ButtonVariant::Outline, "Outline" }
                        Button { variant: ButtonVariant::Ghost, "Ghost" }
                    }

                    div { class: "mt-4 flex flex-wrap items-center gap-3",
                        Button { size: ButtonSize::Small, variant: ButtonVariant::Outline, "Small" }
                        Button { size: ButtonSize::Medium, variant: ButtonVariant::Outline, "Medium" }
                        Button { size: ButtonSize::Large, variant: ButtonVariant::Outline, "Large" }
                        Button { size: ButtonSize::IconSmall, variant: ButtonVariant::Ghost, aria_label: "Small icon button", "±" }
                        Button { size: ButtonSize::IconMedium, variant: ButtonVariant::Ghost, aria_label: "Medium icon button", "±" }
                        Button { size: ButtonSize::IconLarge, variant: ButtonVariant::Ghost, aria_label: "Large icon button", "±" }
                    }

                    div { class: "mt-4 flex flex-wrap items-center gap-3",
                        Button { state: ButtonState::Enabled, "Enabled" }
                        Button { state: ButtonState::Disabled, "Disabled" }
                        Button { state: ButtonState::Loading, "Loading diff" }
                    }
                }

                Separator {}

                section { aria_labelledby: "text-input-foundations",
                    h2 { id: "text-input-foundations", class: "text-sm font-semibold text-ink", "Text inputs" }
                    p { class: "mt-1 text-xs leading-5 text-ink-2", "The parent owns input state; the component keeps labels, controls, and supporting content aligned." }

                    div { class: "mt-4 grid gap-5 md:grid-cols-3",
                        TextInput {
                            label: "Filter files",
                            placeholder: "src/render",
                            value: filter_preview,
                            oninput: move |event: FormEvent| filter_value.set(event.value()),
                            supporting_content: Some(rsx! {
                                p { "Current filter: " code { class: "font-mono text-ink", "{filter_value}" } }
                            }),
                        }
                        TextInput {
                            label: "Pinned revision",
                            value: "39347d0",
                            disabled: true,
                            supporting_content: Some(rsx! { p { "Resolved from the active render." } }),
                        }
                        TextInput {
                            label: "Base revision",
                            value: "missing-branch",
                            aria_invalid: "true",
                            aria_describedby: "base-revision-error",
                            supporting_content: Some(rsx! {
                                p { id: "base-revision-error", class: "text-del", "The revision does not exist in this repository." }
                            }),
                        }
                    }
                }

                Separator {}

                section { aria_labelledby: "badge-foundations",
                    h2 { id: "badge-foundations", class: "text-sm font-semibold text-ink", "Badges" }
                    div { class: "mt-4 flex flex-wrap items-center gap-3",
                        Badge { variant: BadgeVariant::Neutral, "Modified" }
                        Badge { variant: BadgeVariant::Accent, "Renamed" }
                        Badge { variant: BadgeVariant::Addition, "Added" }
                        Badge { variant: BadgeVariant::Deletion, "Deleted" }
                    }
                }

                Separator {}

                section { aria_labelledby: "separator-foundations",
                    h2 { id: "separator-foundations", class: "text-sm font-semibold text-ink", "Separators" }
                    div { class: "mt-4 grid gap-5 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-center",
                        p { class: "text-xs text-ink-2", "Horizontal separators divide stacked regions." }
                        div { class: "hidden h-8 sm:block",
                            Separator { orientation: SeparatorOrientation::Vertical }
                        }
                        p { class: "text-xs text-ink-2", "Vertical separators divide compact tool groups." }
                    }
                }

                Separator {}

                section { aria_labelledby: "skeleton-foundations",
                    h2 { id: "skeleton-foundations", class: "text-sm font-semibold text-ink", "Skeletons" }
                    div { class: "mt-4 grid gap-3", role: "status", aria_label: "Loading diff preview",
                        Skeleton { class: "h-3 w-2/5" }
                        Skeleton { class: "h-3 w-full" }
                        Skeleton { class: "h-3 w-5/6" }
                        Skeleton { class: "h-20 w-full" }
                        span { class: "sr-only", "Loading diff preview" }
                    }
                }
            }
        }
    }
}
