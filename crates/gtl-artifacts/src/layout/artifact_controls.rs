//! Mobile navigation and view actions for self-contained raw artifacts.

use maud::{Markup, PreEscaped, html};

const FILES_ICON: &str = r#"<svg fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true" data-slot="icon"><path stroke-linecap="round" stroke-linejoin="round" d="M3.75 6.75h16.5M3.75 12h16.5m-16.5 5.25h16.5"/></svg>"#;
const HISTORY_ICON: &str = r#"<svg fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true" data-slot="icon"><path stroke-linecap="round" stroke-linejoin="round" d="M12 6v6h4.5m4.5 0a9 9 0 1 1-18 0 9 9 0 0 1 18 0Z"/></svg>"#;
const VIEW_ICON: &str = r#"<svg fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor" aria-hidden="true" data-slot="icon"><path stroke-linecap="round" stroke-linejoin="round" d="M10.5 6h9.75M10.5 6a1.5 1.5 0 1 1-3 0m3 0a1.5 1.5 0 1 0-3 0M3.75 6H7.5m3 12h9.75m-9.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-3.75 0H7.5m9-6h3.75m-3.75 0a1.5 1.5 0 0 1-3 0m3 0a1.5 1.5 0 0 0-3 0m-9.75 0h9.75"/></svg>"#;

const MOBILE_NAVIGATION_BUTTON_CLASSES: &str = "relative hidden min-w-0 cursor-pointer flex-col items-center justify-center gap-0.5 border-0 bg-transparent px-1 py-1 text-[10px] leading-none text-ink-2 [font:inherit] hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-acc disabled:cursor-default disabled:opacity-35 mobile:flex [&_svg]:size-5";
const MOBILE_MENU_BUTTON_CLASSES: &str = "inline-flex min-h-11 w-full cursor-pointer items-center rounded-sm border border-line-2 bg-surface-2 px-3 py-2.5 text-left text-[12.5px] text-ink-2 [font:inherit] hover:border-acc-line hover:bg-acc-soft hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";

pub(super) fn files_navigation(target: &str, count: Option<usize>, enabled: bool) -> Markup {
    navigation_button("Changed files", "Files", target, count, enabled, FILES_ICON)
}

pub(super) fn commits_navigation(target: &str, count: Option<usize>, enabled: bool) -> Markup {
    navigation_button(
        "Commits in range",
        "History",
        target,
        count,
        enabled,
        HISTORY_ICON,
    )
}

pub(super) fn view_navigation(target: &str) -> Markup {
    navigation_button("View settings", "View", target, None, true, VIEW_ICON)
}

pub(super) fn popover(target: &str) -> Markup {
    let artifact_actions = artifact_actions();
    html! {
        aside id=(target)
            class="artifact-mobile-controls fixed top-2 right-2 bottom-auto left-auto m-0 max-h-[calc(100vh_-_16px)] w-[min(320px,calc(100vw_-_16px))] max-w-none overflow-y-auto rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)]"
            popover {
            header class="sticky top-0 z-1 flex items-center justify-between border-b border-line bg-surface-2 px-3 py-2.5" {
                div {
                    strong class="block text-[13px]" { "View settings" }
                    span class="block text-[11px] text-ink-3" { "Display and diff actions" }
                }
                button type="button"
                    class="size-9 cursor-pointer rounded-sm border-0 bg-transparent text-lg text-ink-2 [font:inherit] hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    popovertarget=(target)
                    popovertargetaction="hide"
                    aria-label="Close view settings" { "x" }
            }
            div class="grid gap-3 p-3" {
                (section("Diff", &artifact_actions))
            }
        }
    }
}

fn section(title: &str, content: &Markup) -> Markup {
    html! {
        section class="rounded-panel border border-line bg-sunk p-3" {
            h2 class="m-0 mb-2.5 text-[10px] font-bold tracking-[.07em] text-ink-3 uppercase" { (title) }
            (content)
        }
    }
}

fn artifact_actions() -> Markup {
    html! {
        div class="grid gap-2" {
            button type="button"
                class=(MOBILE_MENU_BUTTON_CLASSES)
                data-artifact-action="fold-all" { "Collapse or expand all files" }
            button type="button"
                class=(MOBILE_MENU_BUTTON_CLASSES)
                data-artifact-action="toggle-context" { "Toggle copy context" }
        }
    }
}

fn navigation_button(
    aria_label: &str,
    label: &str,
    target: &str,
    count: Option<usize>,
    enabled: bool,
    icon: &str,
) -> Markup {
    html! {
        button type="button"
            class=(MOBILE_NAVIGATION_BUTTON_CLASSES)
            popovertarget=(target)
            disabled[!enabled]
            aria-label=(aria_label) {
            (PreEscaped(icon))
            span { (label) }
            @if let Some(count) = count {
                span class="absolute top-1 right-1 min-w-4 rounded-full bg-acc-soft px-1 py-0.5 text-center text-[9px] font-semibold leading-none text-acc" { (count) }
            }
        }
    }
}
