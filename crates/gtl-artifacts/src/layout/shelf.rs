//! Right commit shelf: one card per commit in range plus the native-popover
//! bodies for commits carrying extended notes.

use gtl_application::diffs::View;
use gtl_models::diffs::Commit;
use maud::{Markup, html};

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
    "relative ml-1.5 rounded-r-sm border-l-2 border-line-2 py-1.5 pr-2 pl-6 ",
    "hover:bg-surface-2 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-acc ",
    "[&.has:hover]:border-l-acc [&.active]:border-l-acc [&.active]:bg-acc-soft",
);
const BEAD_CLASSES: &str = concat!(
    "pointer-events-none absolute -left-4 top-2 flex size-5 items-center justify-center bg-transparent p-0 ",
    "before:size-2.5 before:rounded-full before:border-2 before:border-line-2 before:bg-bg before:shadow-[0_0_0_3px_var(--surface)] before:content-['']",
);
const NOTES_ICON_CLASSES: &str = "h-2.5 w-3 flex-none opacity-[.85] [background:repeating-linear-gradient(var(--acc),var(--acc)_2px,transparent_2px,transparent_4px)]";
const MERGE_PILL_CLASSES: &str = "whitespace-nowrap rounded-sm border border-line-2 bg-surface-2 px-1.5 py-px text-xs text-ink-3";
const SHA_CLASSES: &str = "cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-1.5 py-0.5 text-xs text-acc hover:border-acc active:bg-acc active:text-bg focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc";
const WHEN_CLASSES: &str =
    "ml-auto whitespace-nowrap text-ink-3 [font-variant-numeric:tabular-nums]";
const SUBJECT_CLASSES: &str = "wrap-anywhere leading-normal text-ink-2";
const POPOVER_CLASSES: &str = concat!(
    "fixed inset-auto m-0 w-[330px] max-w-[92vw] overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink ",
    "shadow-[0_14px_44px_rgba(0,0,0,.7)] [&::backdrop]:bg-transparent",
);

pub(super) struct CommitShelfPresentation<'view> {
    commits_label: &'view str,
    hint: &'static str,
    commits: Vec<CommitPresentation<'view>>,
}

impl<'view> CommitShelfPresentation<'view> {
    pub(super) fn new(view: &'view View) -> Self {
        Self {
            commits_label: &view.commits_label,
            hint: "hash = copy · hover = notes",
            commits: view.commits.iter().map(CommitPresentation::new).collect(),
        }
    }
}

struct CommitPresentation<'view> {
    commit: &'view Commit,
    abbreviated_sha: String,
    popover_id: Option<String>,
}

impl<'view> CommitPresentation<'view> {
    fn new(commit: &'view Commit) -> Self {
        let has_notes = !commit.body.trim().is_empty();
        Self {
            commit,
            abbreviated_sha: commit.sha.chars().take(10).collect(),
            popover_id: has_notes.then(|| format!("pop-{}", commit.sha)),
        }
    }

    fn has_notes(&self) -> bool {
        self.popover_id.is_some()
    }
}

pub(super) fn shelf(presentation: &CommitShelfPresentation<'_>) -> Markup {
    html! {
        aside class={ "shelf " (SHELF_CLASSES) " " (SHELF_STATE_CLASSES) } aria-label="Commits in range" {
            div {
                h3 class="mx-0.5 mt-1.5 mb-1 font-semibold tracking-wider text-ink-3 uppercase" { (presentation.commits_label) }
                p class="mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-ink-3" {
                    span class="size-2 flex-none rounded-full bg-acc shadow-[0_0_0_3px_var(--acc-soft)]" {}
                    (presentation.hint)
                }
            }
            (commit_rows(presentation))
        }
    }
}

pub(super) fn mobile_popover(presentation: &CommitShelfPresentation<'_>, target: &str) -> Markup {
    html! {
        aside id=(target)
            data-artifact-commits-popover
            class={ "fixed inset-3 m-0 h-[calc(100vh_-_24px)] w-[calc(100vw_-_24px)] max-w-none overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)] " (SHELF_STATE_CLASSES) }
            aria-label="Commits in range"
            popover {
            header class="flex items-center justify-between border-b border-line bg-surface-2 px-4 py-3" {
                div {
                    strong class="block" { "Commit history" }
                    span class="text-ink-3" { (presentation.commits_label) }
                }
                button type="button" class="size-8 cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 hover:bg-line hover:text-ink active:bg-acc-soft focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    popovertarget=(target) popovertargetaction="hide" aria-label="Close commits in range" { "×" }
            }
            div class="gtl-scroll h-[calc(100%_-_57px)] overflow-y-auto p-3" {
                (commit_rows(presentation))
            }
        }
    }
}

fn commit_rows(presentation: &CommitShelfPresentation<'_>) -> Markup {
    if presentation.commits.is_empty() {
        return html! { div class="empty rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic" { "no commits in range" } };
    }

    html! {
        @for commit_presentation in &presentation.commits {
            @let commit = commit_presentation.commit;
            div class={
                (if commit_presentation.has_notes() { "cline has " } else { "cline " })
                (COMMIT_CARD_CLASSES)
            }
                data-sha=(commit.sha)
                data-pop=[commit_presentation.popover_id.as_deref()] {
                span class={ "bead " (BEAD_CLASSES) } aria-hidden="true" {}
                div class="top mb-1 flex items-center gap-1.5" {
                    button class={ "sha " (SHA_CLASSES) } type="button" title="copy hash" {
                        (commit_presentation.abbreviated_sha)
                    }
                    @if commit_presentation.has_notes() {
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
                div class={ "sub " (SUBJECT_CLASSES) } { (commit.subject) }
            }
        }
    }
}

// Native-popover bodies for commits that carry a body, emitted once per .layout (top-level
// [popover] elements escape the sidebar's scroll clip). The id is keyed to the sha so the
// shelf card's data-pop can resolve its popover within `root`.
pub(super) fn commit_popovers(presentation: &CommitShelfPresentation<'_>) -> Markup {
    html! {
        @for commit_presentation in &presentation.commits {
            @let commit = commit_presentation.commit;
            @if commit_presentation.has_notes() {
                div id={ "pop-" (commit.sha) } class={ "print:hidden! " (POPOVER_CLASSES) } popover {
                    div class="flex items-center gap-2 border-b border-line bg-surface-2 px-3 py-2.5" {
                        span class="text-xs text-acc" { (commit.sha) }
                        @if !commit.date.is_empty() {
                            span class="ml-auto text-ink-3" { (commit.date) }
                        }
                    }
                    div class="px-3 pt-2.5 pb-1 font-semibold text-ink" { (commit.subject) }
                    div class="px-3 pt-1 pb-3 leading-relaxed whitespace-pre-wrap text-ink-2" { (commit.body.trim()) }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::RenderOptions;
    use gtl_models::diffs::Commit;

    use crate::{fixtures::sample_view, test_render::build_html};

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
