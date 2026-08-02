//! Right commit shelf: one card per commit in range plus the native-popover
//! bodies for commits carrying extended notes.

use gtl_application::{diffs::View, viewer::RenderOptions};
use maud::{Markup, html};

use super::Surface;

const SHELF_CLASSES: &str = concat!(
    "gtl-scroll [grid-area:2/3] overflow-auto border-l border-line bg-surface p-3 ",
    "compact:p-2.5 tablet:hidden print:hidden!",
);
const SHELF_STATE_CLASSES: &str = concat!(
    "[&_.cline:hover_.bead::before]:border-acc [&_.cline.active_.bead::before]:border-acc [&_.cline.active_.bead::before]:bg-acc [&_.cline.active_.bead::before]:shadow-[0_0_0_3px_var(--acc-soft)] ",
    "[&_.cline:hover_.sha]:bg-acc [&_.cline:hover_.sha]:text-bg [&_.cline.active_.sha]:bg-acc [&_.cline.active_.sha]:text-bg ",
    "[&_.cline.copied_.sha]:border-add [&_.cline.copied_.sha]:bg-add [&_.cline.copied_.sha]:text-bg [&_.cline.copied_.sha::after]:content-['_\\2713'] ",
    "[&_.cline.active_.merge-pill]:border-acc-line [&_.cline.active_.merge-pill]:bg-acc-soft [&_.cline.active_.merge-pill]:text-acc",
);
const COMMIT_CARD_CLASSES: &str = concat!(
    "relative ml-1.5 rounded-r-sm border-l-2 border-line-2 py-1.5 pr-2 pl-[22px] ",
    "hover:bg-surface-2 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-1 focus-visible:outline-acc ",
    "[&.has:hover]:shadow-[inset_2px_0_0_var(--acc)] [&.active]:shadow-[inset_2px_0_0_var(--acc)]",
);
const BEAD_CLASSES: &str = concat!(
    "pointer-events-none absolute left-[-15px] top-[7px] flex size-[18px] items-center justify-center bg-transparent p-0 ",
    "before:size-[9px] before:rounded-full before:border-2 before:border-line-2 before:bg-bg before:shadow-[0_0_0_3px_var(--surface)] before:content-['']",
);
const NOTES_ICON_CLASSES: &str = "h-2.5 w-3 flex-none opacity-[.85] [background:repeating-linear-gradient(var(--acc),var(--acc)_2px,transparent_2px,transparent_4px)]";
const MERGE_PILL_CLASSES: &str = "whitespace-nowrap rounded-sm border border-line-2 bg-surface-2 px-1.5 py-px text-[10.5px] text-ink-3";
const SHA_CLASSES: &str = "cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-1.5 py-0.5 text-[12px] text-acc [font:inherit]";
const WHEN_CLASSES: &str =
    "ml-auto whitespace-nowrap text-[11px] text-ink-3 [font-variant-numeric:tabular-nums]";
const SUBJECT_CLASSES: &str = "text-[12.5px] leading-[1.42] text-ink-2 [overflow-wrap:anywhere]";
const POPOVER_CLASSES: &str = concat!(
    "fixed inset-auto m-0 w-[330px] max-w-[92vw] overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink ",
    "shadow-[0_14px_44px_rgba(0,0,0,.7)] [&::backdrop]:bg-transparent",
);

pub(super) fn shelf(
    view: &View,
    surface: Surface,
    options: RenderOptions,
    selected_commit_sha: Option<&str>,
) -> Markup {
    html! {
        aside class={ "shelf " (SHELF_CLASSES) " " (SHELF_STATE_CLASSES) } aria-label="Commits in range" {
            div {
                h3 class="mx-0.5 mt-1.5 mb-1 text-[11px] font-semibold tracking-[0.06em] text-ink-3 uppercase" { (view.commits_label) }
                p class="mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-[11px] text-ink-3" {
                    span class="size-2 flex-none rounded-full bg-acc shadow-[0_0_0_3px_var(--acc-soft)]" {}
                    @match surface {
                        Surface::App { .. } => { "view = standalone patch · hash = copy · hover = notes" }
                        Surface::Artifact { .. } => { "hash = copy · hover = notes" }
                    }
                }
            }
            (commit_rows(view, surface, options, selected_commit_sha))
        }
    }
}

pub(super) fn mobile_popover(
    view: &View,
    surface: Surface,
    options: RenderOptions,
    selected_commit_sha: Option<&str>,
    target: &str,
) -> Markup {
    html! {
        aside id=(target)
            data-preview-commits-popover
            class={ "fixed inset-3 m-0 h-[calc(100vh_-_24px)] w-[calc(100vw_-_24px)] max-w-none overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)] " (SHELF_STATE_CLASSES) }
            aria-label="Commits in range"
            popover {
            header class="flex items-center justify-between border-b border-line bg-surface-2 px-4 py-3" {
                div {
                    strong class="block text-[13px]" { "Commit history" }
                    span class="text-[11px] text-ink-3" { (view.commits_label) }
                }
                button type="button" class="size-[30px] cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 [font:inherit] hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    popovertarget=(target) popovertargetaction="hide" aria-label="Close commits in range" { "×" }
            }
            div class="gtl-scroll h-[calc(100%_-_57px)] overflow-y-auto p-3" {
                (commit_rows(view, surface, options, selected_commit_sha))
            }
        }
    }
}

fn commit_rows(
    view: &View,
    surface: Surface,
    options: RenderOptions,
    selected_commit_sha: Option<&str>,
) -> Markup {
    if view.commits.is_empty() {
        return html! { div class="empty rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic" { "no commits in range" } };
    }

    html! {
        @for commit in &view.commits {
            @let has_notes = !commit.body.trim().is_empty();
            @let selected = selected_commit_sha == Some(commit.sha.as_str());
            div class={
                (if has_notes { "cline has " } else { "cline " })
                (if selected { "active " } else { "" })
                (COMMIT_CARD_CLASSES)
            }
                data-sha=(commit.sha)
                data-pop=[has_notes.then(|| format!("pop-{}", commit.sha))] {
                span class={ "bead " (BEAD_CLASSES) } aria-hidden="true" {}
                div class="top mb-1 flex items-center gap-1.5" {
                    button class={ "sha " (SHA_CLASSES) } type="button" title="copy hash" {
                        (abbreviate(&commit.sha))
                    }
                    @if has_notes {
                        span class={ "notes-ico " (NOTES_ICON_CLASSES) } aria-hidden="true" title="has extended notes" {}
                    }
                    @if commit.is_merge() {
                        span class={ "merge-pill " (MERGE_PILL_CLASSES) } { "merge" }
                    }
                    @if !commit.date.is_empty() {
                        @if commit.iso.is_empty() {
                            time class={ "when " (WHEN_CLASSES) } { (commit.date) }
                        } @else {
                            time class={ "when " (WHEN_CLASSES) } datetime=(commit.iso) title=(commit.iso) { (commit.date) }
                        }
                    }
                }
                @match surface {
                    Surface::App { tab_id } => {
                        button type="button"
                            class={ "commit-select block w-full cursor-pointer border-0 bg-transparent p-0 text-left [font:inherit] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc sub " (SUBJECT_CLASSES) }
                            aria-pressed=(selected)
                            hx-get=(if selected {
                                format!(
                                    "/tabs/{tab_id}/view?layout={}&density={}",
                                    options.layout(),
                                    options.density(),
                                )
                            } else {
                                format!(
                                    "/tabs/{tab_id}/commits/{}/view?layout={}&density={}",
                                    commit.sha,
                                    options.layout(),
                                    options.density(),
                                )
                            })
                            hx-target="#viewer-view"
                            hx-sync="#viewer-view:replace"
                            hx-swap="outerHTML" {
                            (commit.subject)
                        }
                    }
                    Surface::Artifact { .. } => {
                        div class={ "sub " (SUBJECT_CLASSES) } { (commit.subject) }
                    }
                }
            }
        }
    }
}

// Native-popover bodies for commits that carry a body, emitted once per .layout (top-level
// [popover] elements escape the sidebar's scroll clip). The id is keyed to the sha so the
// shelf card's data-pop can resolve its popover within `root`.
pub(super) fn commit_popovers(view: &View) -> Markup {
    html! {
        @for commit in &view.commits {
            @if !commit.body.trim().is_empty() {
                div id={ "pop-" (commit.sha) } class={ "print:hidden! " (POPOVER_CLASSES) } popover {
                    div class="flex items-center gap-2 border-b border-line bg-surface-2 px-3 py-2.5" {
                        span class="text-[12px] text-acc" { (commit.sha) }
                        @if !commit.date.is_empty() {
                            span class="ml-auto text-[11.5px] text-ink-3" { (commit.date) }
                        }
                    }
                    div class="px-3 pt-2.5 pb-1 text-[13px] font-semibold text-ink" { (commit.subject) }
                    div class="px-3 pt-1 pb-3 text-[12.5px] leading-[1.6] whitespace-pre-wrap text-ink-2" { (commit.body.trim()) }
                }
            }
        }
    }
}

fn abbreviate(sha: &str) -> String {
    sha.chars().take(10).collect()
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::RenderOptions;
    use gtl_models::diffs::Commit;

    use crate::{build_html, fixtures::sample_view};

    #[test]
    fn no_commit_state_owns_its_presentation_utilities() {
        let mut view = sample_view();
        view.commits.clear();

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains(
            r#"<div class="empty rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic">no commits in range</div>"#
        ));
    }

    #[test]
    fn artifact_commit_shelf_is_informational_and_copies_hashes() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(!html.contains(r#"class="commit-select"#));
        assert!(!html.contains(r#"role="button""#));
        assert!(!html.contains(r#"tabindex="0""#));
        assert!(!html.contains("commits/abc123def/view"));
        assert!(html.contains(r#"<button class="sha "#));
        assert!(html.contains(r#"type="button" title="copy hash""#));
        assert!(html.contains(r#"<span class="bead "#));
        assert!(html.contains(r#"aria-hidden="true"></span>"#));
        assert!(!html.contains(r#"<button class="bead""#));
        assert!(!html.contains(r#"title="click to copy hash""#));
    }

    #[test]
    fn shelf_owns_dynamic_descendant_state_presentation_once() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None).replace("&amp;", "&");
        let shelf_classes = html
            .split_once(r#"<aside class="shelf "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("shelf class attribute");
        let card_classes = html
            .split_once(r#"<div class="cline has "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("commit card class attribute");

        for state_selector in [
            "[&_.cline:hover_.bead::before]",
            "[&_.cline.active_.bead::before]",
            "[&_.cline.active_.sha]",
            "[&_.cline.copied_.sha]",
            "[&_.cline.active_.merge-pill]",
        ] {
            assert!(
                shelf_classes.contains(state_selector),
                "shelf must own `{state_selector}`"
            );
        }
        for repeated_tail in ["_.bead", "_.sha", "_.merge-pill", ".copied"] {
            assert!(
                !card_classes.contains(repeated_tail),
                "card must not repeat `{repeated_tail}` state presentation"
            );
        }
    }

    #[test]
    fn commit_rows_marks_a_merge_card_without_derived_members() {
        let mut view = sample_view();
        view.commits = vec![
            Commit {
                sha: "merge1234".to_string(),
                subject: "Merge branch 'sub'".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: vec!["p1aaaaaaa".to_string(), "p2bbbbbbb".to_string()],
            },
            Commit {
                sha: "plain5678".to_string(),
                subject: "feat: x".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: vec!["p1aaaaaaa".to_string()],
            },
        ];

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains(">merge</span>"));
        assert!(!html.contains("data-members="));
        assert!(html.contains("[&amp;_.cline.active_.merge-pill]:border-acc-line"));
    }
}
