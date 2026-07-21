//! Right commit shelf: one card per commit in range plus the native-popover
//! bodies for commits carrying extended notes.

use application::diffs::View;
use maud::{Markup, html};

const SHELF_CLASSES: &str = concat!(
    "[grid-area:2/3] overflow-auto border-l border-line bg-surface p-3 ",
    "[@media(max-width:1280px)]:p-2.5 [@media(max-width:1024px)]:hidden print:hidden! ",
    "[&::-webkit-scrollbar]:w-2 [&::-webkit-scrollbar-thumb]:rounded-panel [&::-webkit-scrollbar-thumb]:border-2 [&::-webkit-scrollbar-thumb]:border-surface [&::-webkit-scrollbar-thumb]:bg-line-2",
);
const COMMIT_CARD_CLASSES: &str = concat!(
    "relative ml-1.5 cursor-pointer rounded-r-sm border-l-2 border-line-2 py-1.5 pr-2 pl-[22px] ",
    "hover:bg-surface-2 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-1 focus-visible:outline-acc ",
    "[&.has:hover]:shadow-[inset_2px_0_0_var(--acc)] [&.active]:shadow-[inset_2px_0_0_var(--acc)] ",
    "[&:hover_.bead::before]:border-acc [&.active_.bead::before]:border-acc [&.active_.bead::before]:bg-acc [&.active_.bead::before]:shadow-[0_0_0_3px_var(--acc-soft)] ",
    "[&:hover_.sha]:bg-acc [&:hover_.sha]:text-bg [&.active_.sha]:bg-acc [&.active_.sha]:text-bg ",
    "[&.copied_.sha]:border-add [&.copied_.sha]:bg-add [&.copied_.sha]:text-bg [&.copied_.sha::after]:content-['_\\2713'] ",
    "[&.active_.merge-pill]:border-acc-line [&.active_.merge-pill]:bg-acc-soft [&.active_.merge-pill]:text-acc",
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

pub(super) fn shelf(view: &View) -> Markup {
    html! {
        aside class={ "shelf " (SHELF_CLASSES) } aria-label="Commits in range" {
            div {
                h3 class="mx-0.5 mt-1.5 mb-1 text-[11px] font-semibold tracking-[0.06em] text-ink-3 uppercase" { (view.commits_label) }
                p class="mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-[11px] text-ink-3" {
                    span class="size-2 flex-none rounded-full bg-acc shadow-[0_0_0_3px_var(--acc-soft)]" {}
                    "click card = focus commit · hash = copy · hover = notes"
                }
            }
            (commit_rows(view))
        }
    }
}

// Right-hand commit shelf cards. The card filters files by commit; the hash tag copies its sha.
// A card with a body also gets a distinct `notes-ico` glyph + a `data-pop` pointer to its sibling
// [popover] (emitted by commit_popovers). The always-visible body `pre` of the old terminal layout
// is gone.
fn commit_rows(view: &View) -> Markup {
    if view.commits.is_empty() {
        return html! { div class="empty rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic" { "no commits in range" } };
    }

    html! {
        @for commit in &view.commits {
            @let has_notes = !commit.body.trim().is_empty();
            div class={ (if has_notes { "cline has " } else { "cline " }) (COMMIT_CARD_CLASSES) }
                data-sha=(commit.sha)
                data-members=[merge_members_attr(commit)]
                data-pop=[has_notes.then(|| format!("pop-{}", commit.sha))]
                role="button" tabindex="0" title="focus this commit's changes" {
                span class={ "bead " (BEAD_CLASSES) } aria-hidden="true" {}
                div class="top mb-1 flex items-center gap-1.5" {
                    button class={ "sha " (SHA_CLASSES) } type="button" title="copy hash" { (commit.sha) }
                    @if has_notes {
                        span class={ "notes-ico " (NOTES_ICON_CLASSES) } aria-hidden="true" title="has extended notes" {}
                    }
                    @if commit.is_merge() && !commit.members.is_empty() {
                        span class={ "merge-pill " (MERGE_PILL_CLASSES) } title="commits this merge brought in — focus to highlight them" {
                            "merge · " (commit.members.len())
                        }
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

// ! Serialize a merge's brought-in commits for the focus set; non-merge / empty -> no attribute.
fn merge_members_attr(commit: &domain::diffs::Commit) -> Option<String> {
    (commit.is_merge() && !commit.members.is_empty()).then(|| commit.members.join(" "))
}

#[cfg(test)]
mod tests {
    use application::viewer::RenderOptions;
    use domain::diffs::Commit;

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
    fn commit_shelf_click_contract_focuses_card_and_copies_hash_tag() {
        // ! JS behavior: sha-guard predicate (isShaTarget) covered by Vitest wheel.test.ts.
        // ! copyText + stopPropagation wiring is event-listener-only and not extracted.
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(html.contains(r#"title="focus this commit's changes""#));
        assert!(html.contains(r#"<button class="sha "#));
        assert!(html.contains(r#"type="button" title="copy hash""#));
        assert!(html.contains(r#"<span class="bead "#));
        assert!(html.contains(r#"aria-hidden="true"></span>"#));
        assert!(!html.contains(r#"<button class="bead""#));
        assert!(!html.contains(r#"title="click to copy hash""#));
    }

    #[test]
    fn commit_rows_marks_a_merge_card_with_members_and_pill() {
        let mut view = sample_view();
        view.commits = vec![
            Commit {
                sha: "merge1234".to_string(),
                subject: "Merge branch 'sub'".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: vec!["p1aaaaaaa".to_string(), "p2bbbbbbb".to_string()],
                members: vec!["aaa111aaa".to_string(), "bbb222bbb".to_string()],
            },
            Commit {
                sha: "plain5678".to_string(),
                subject: "feat: x".to_string(),
                body: String::new(),
                date: String::new(),
                iso: String::new(),
                parents: vec!["p1aaaaaaa".to_string()],
                members: Vec::new(),
            },
        ];

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains(r#"data-members="aaa111aaa bbb222bbb""#));
        assert!(html.contains("merge · 2"));
        // exactly one card is a merge: no pill / no data-members leaks onto the plain card
        assert_eq!(html.matches("merge · ").count(), 1);
        assert_eq!(html.matches("data-members=").count(), 1);
        assert!(html.contains("[&amp;.active_.merge-pill]:border-acc-line"));
    }

    #[test]
    fn commit_rows_skips_pill_for_a_merge_with_no_in_range_members() {
        let mut view = sample_view();
        view.commits = vec![Commit {
            sha: "merge1234".to_string(),
            subject: "Merge branch 'main'".to_string(),
            body: String::new(),
            date: String::new(),
            iso: String::new(),
            parents: vec!["p1aaaaaaa".to_string(), "p2bbbbbbb".to_string()],
            members: Vec::new(), // base-bounded walk found nothing in range
        }];

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(!html.contains("merge · "));
        assert!(!html.contains("data-members="));
    }
}
