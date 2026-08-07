use dioxus::prelude::*;

const FAVICON: Asset = asset!("/assets/favicon.ico");
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
        div { class: "min-h-screen bg-canvas text-ink antialiased",
            header { class: "sticky top-0 z-20 border-b border-line bg-canvas/90 backdrop-blur-xl",
                div { class: "mx-auto flex max-w-[1560px] items-center justify-between gap-6 px-5 py-3 lg:px-8",
                    div { class: "flex min-w-0 items-center gap-3",
                        div { class: "grid size-9 shrink-0 place-items-center rounded-xl border border-accent/35 bg-accent/10 font-mono text-sm font-bold text-accent", "±" }
                        div { class: "min-w-0",
                            p { class: "truncate text-sm font-semibold tracking-tight text-ink", "git-tools viewer" }
                            p { class: "truncate font-mono text-[10px] uppercase tracking-[0.16em] text-muted", "Dioxus Web proof" }
                        }
                    }
                    nav { class: "flex items-center gap-1 rounded-xl border border-line bg-panel/80 p-1", aria_label: "Proof routes",
                        Link { class: "rounded-lg px-3 py-1.5 text-xs font-medium text-muted transition hover:bg-raised hover:text-ink", to: Route::Workbench {}, "Workbench" }
                        Link { class: "rounded-lg px-3 py-1.5 text-xs font-medium text-muted transition hover:bg-raised hover:text-ink", to: Route::Boundary {}, "Boundary" }
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
            section { class: "flex flex-col justify-between gap-4 rounded-2xl border border-line bg-panel px-5 py-4 shadow-[0_18px_50px_rgba(0,0,0,0.22)] md:flex-row md:items-center",
                div { class: "max-w-3xl",
                    div { class: "mb-2 flex flex-wrap items-center gap-2",
                        span { class: "rounded-full border border-accent/30 bg-accent/10 px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.14em] text-accent", "Shell · Dioxus" }
                        span { class: "rounded-full border border-cyan/30 bg-cyan/10 px-2.5 py-1 font-mono text-[10px] uppercase tracking-[0.14em] text-cyan", "Diff · Rust/Maud" }
                    }
                    h1 { class: "text-xl font-semibold tracking-[-0.025em] text-ink md:text-2xl", "A reactive shell around the proven renderer" }
                    p { class: "mt-1.5 max-w-2xl text-sm leading-6 text-muted", "Dioxus owns the navigable shell. The existing backend still renders the diff subtree, which the bridge mounts into an isolated shadow root." }
                }
                button {
                    id: "dioxus-refresh-diff",
                    class: "inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 rounded-xl border border-accent/40 bg-accent px-4 py-2.5 text-sm font-semibold text-accent-ink shadow-[0_8px_24px_rgba(143,174,255,0.2)] transition hover:-translate-y-px hover:bg-accent-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent",
                    onclick: move |_| *refresh_revision.write() += 1,
                    "Replace diff"
                    span { class: "rounded-md bg-black/15 px-1.5 py-0.5 font-mono text-[10px]", "r{revision}" }
                }
            }

            section { class: "min-h-0 overflow-hidden rounded-2xl border border-line bg-panel shadow-[0_24px_80px_rgba(0,0,0,0.3)]",
                header { class: "flex items-center justify-between gap-4 border-b border-line bg-raised/70 px-4 py-2.5",
                    div { class: "flex items-center gap-2",
                        span { class: "size-2 rounded-full bg-cyan shadow-[0_0_16px_rgba(83,211,230,0.65)]" }
                        h2 { class: "text-xs font-semibold text-ink", "Backend-rendered diff island" }
                    }
                    code { class: "font-mono text-[10px] text-faint", "#dioxus-diff-island → shadowRoot" }
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
                    accent: "border-accent/35 bg-accent/5",
                }
                OwnershipCard {
                    eyebrow: "Performance island",
                    title: "Existing Rust renderer",
                    detail: "Owns every diff row and returns the same real viewer fragment. The bridge replaces one shadow-root subtree atomically; Dioxus never reconciles those nodes.",
                    accent: "border-cyan/35 bg-cyan/5",
                }
            }
            div { class: "mt-5 rounded-2xl border border-line bg-panel p-5",
                h1 { class: "text-lg font-semibold text-ink", "Proof boundary" }
                p { class: "mt-2 text-sm leading-6 text-muted", "This crate is a removable integration experiment. It does not migrate the production HTMX shell, package a release frontend, or redefine backend/session ownership." }
                Link { class: "mt-5 inline-flex rounded-xl border border-line bg-raised px-4 py-2 text-sm font-medium text-ink transition hover:border-accent/50 hover:text-accent", to: Route::Workbench {}, "Return to workbench" }
            }
        }
    }
}

#[component]
fn OwnershipCard(eyebrow: String, title: String, detail: String, accent: String) -> Element {
    rsx! {
        article { class: "rounded-2xl border p-5 {accent}",
            p { class: "font-mono text-[10px] uppercase tracking-[0.16em] text-faint", "{eyebrow}" }
            h2 { class: "mt-2 text-xl font-semibold tracking-tight text-ink", "{title}" }
            p { class: "mt-2 text-sm leading-6 text-muted", "{detail}" }
        }
    }
}
