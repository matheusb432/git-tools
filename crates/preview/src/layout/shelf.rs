//! Right commit shelf: one card per commit in range plus the native-popover
//! bodies for commits carrying extended notes.

use application::diffs::View;
use maud::{Markup, html};

// ! `.shelf` stays as the class anchor for the responsive column overrides, the print rule,
// ! and the scrollbar styling. The `.cline` card family keeps component styling in
// ! styles/shelf.css: its hover/active/copied states are toggled by the enhancer.
pub(super) fn shelf(view: &View) -> Markup {
    html! {
        aside class="shelf [grid-area:2/3] overflow-auto border-l border-line bg-surface p-[13px]" aria-label="Commits in range" {
            div {
                h3 class="mx-0.5 mt-1.5 mb-1 text-[11px] font-semibold tracking-[0.06em] text-ink-3 uppercase" { (view.commits_label) }
                p class="mx-0.5 mt-0 mb-3 flex items-center gap-1.5 text-[11px] text-ink-3" {
                    span class="size-[7px] flex-none rounded-full bg-acc shadow-[0_0_0_3px_var(--acc-soft)]" {}
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
        return html! { div.empty { "no commits in range" } };
    }

    html! {
        @for commit in &view.commits {
            @let has_notes = !commit.body.trim().is_empty();
            div class=(if has_notes { "cline has" } else { "cline" })
                data-sha=(commit.sha)
                data-members=[merge_members_attr(commit)]
                data-pop=[has_notes.then(|| format!("pop-{}", commit.sha))]
                role="button" tabindex="0" title="focus this commit's changes" {
                span.bead aria-hidden="true" {}
                div.top {
                    button.sha type="button" title="copy hash" { (commit.sha) }
                    @if has_notes {
                        span.notes-ico aria-hidden="true" title="has extended notes" {}
                    }
                    @if commit.is_merge() && !commit.members.is_empty() {
                        span.merge-pill title="commits this merge brought in — focus to highlight them" {
                            "merge · " (commit.members.len())
                        }
                    }
                    @if !commit.date.is_empty() {
                        @if commit.iso.is_empty() {
                            time.when { (commit.date) }
                        } @else {
                            time.when datetime=(commit.iso) title=(commit.iso) { (commit.date) }
                        }
                    }
                }
                div.sub { (commit.subject) }
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
                div id={ "pop-" (commit.sha) } popover {
                    div class="flex items-center gap-2 border-b border-line bg-surface-2 px-[13px] py-2.5" {
                        span class="text-[12px] text-acc" { (commit.sha) }
                        @if !commit.date.is_empty() {
                            span class="ml-auto text-[11.5px] text-ink-3" { (commit.date) }
                        }
                    }
                    div class="px-[13px] pt-2.5 pb-1 text-[13px] font-semibold text-ink" { (commit.subject) }
                    div class="px-[13px] pt-1 pb-[13px] text-[12.5px] leading-[1.6] whitespace-pre-wrap text-ink-2" { (commit.body.trim()) }
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

    use crate::{build_html, fixtures::sample_view, preview_css};

    #[test]
    fn commit_shelf_click_contract_focuses_card_and_copies_hash_tag() {
        // ! JS behavior: sha-guard predicate (isShaTarget) covered by Vitest wheel.test.ts.
        // ! copyText + stopPropagation wiring is event-listener-only and not extracted.
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(html.contains(r#"title="focus this commit's changes""#));
        assert!(html.contains(r#"<button class="sha" type="button" title="copy hash""#));
        assert!(html.contains(r#"<span class="bead" aria-hidden="true"></span>"#));
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
        // the styling shipped
        assert!(preview_css().contains(".cline .merge-pill{"));
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
