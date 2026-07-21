//! Center column: one `<details>` block per changed file, carrying the copy
//! buttons, status badge, and selected diff pane.

use application::{
    diffs::{FileDiff, FileStatus, View},
    viewer::{DiffDensity, DiffLayout, RenderOptions},
};
use maud::{Markup, PreEscaped, html};

use crate::{
    rows::{render_diff_lines, render_diff_split},
    text::slug,
};

const GIANT_FILE_CHARS: usize = 250_000;
const ROW_PX: usize = 22;
const COPY_BUTTON_CLASSES: &str = concat!(
    "copy-button cursor-pointer rounded-sm border border-acc-line bg-acc-soft px-1.5 py-px text-[10px] tracking-[.04em] text-acc [font:inherit] ",
    "hover:border-acc hover:bg-acc hover:text-bg ",
    "[&[data-state=ok]]:border-add [&[data-state=ok]]:bg-add [&[data-state=ok]]:text-bg ",
    "[&[data-state=err]]:border-del [&[data-state=err]]:bg-del [&[data-state=err]]:text-bg",
);
const DIFF_CLASSES: &str = concat!(
    "overflow-x-hidden text-[14px] leading-[22px] ",
    "[&::-webkit-scrollbar]:h-2.5 [&::-webkit-scrollbar-track]:bg-surface ",
    "[&::-webkit-scrollbar-thumb]:rounded-panel [&::-webkit-scrollbar-thumb]:border-2 ",
    "[&::-webkit-scrollbar-thumb]:border-surface [&::-webkit-scrollbar-thumb]:bg-line-2 ",
    "print:[&_.dl_code]:text-[#111]",
);
const STATUS_BADGE_CLASSES: &str = "inline-flex size-[15px] flex-none items-center justify-center rounded-sm border text-[9.5px] leading-none font-bold";

#[derive(Clone, Copy)]
struct FileStatusPresentation {
    key: &'static str,
    code: &'static str,
    label: &'static str,
    css_class: &'static str,
    badge_classes: &'static str,
}

const STATUS_ADDED: FileStatusPresentation = FileStatusPresentation {
    key: "added",
    code: "A",
    label: "Added file",
    css_class: "status-added",
    badge_classes: "border-add-line bg-add-bg text-add",
};
const STATUS_DELETED: FileStatusPresentation = FileStatusPresentation {
    key: "deleted",
    code: "D",
    label: "Deleted file",
    css_class: "status-deleted",
    badge_classes: "border-del-line bg-del-bg text-del",
};
const STATUS_RENAMED: FileStatusPresentation = FileStatusPresentation {
    key: "renamed",
    code: "R",
    label: "Renamed file",
    css_class: "status-renamed",
    badge_classes: "border-acc-line bg-acc-soft text-acc",
};
const STATUS_MODIFIED: FileStatusPresentation = FileStatusPresentation {
    key: "modified",
    code: "M",
    label: "Modified file",
    css_class: "status-modified",
    badge_classes: "border-line-2 bg-sunk text-ink-3",
};

fn file_status_presentation(status: FileStatus) -> FileStatusPresentation {
    match status {
        FileStatus::Added => STATUS_ADDED,
        FileStatus::Deleted => STATUS_DELETED,
        FileStatus::Renamed => STATUS_RENAMED,
        FileStatus::Modified => STATUS_MODIFIED,
    }
}

fn file_commits(file: &FileDiff) -> String {
    file.commits.join(" ")
}

pub(super) fn file_blocks(view: &View, options: RenderOptions) -> Markup {
    if view.files.is_empty() {
        return html! { div class="empty rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic" { "no file changes" } };
    }

    html! {
        @for file in &view.files {
            @let absolute = format!("{}/{}", view.repo_root, file.path);
            @let status = file_status_presentation(file.status());
            @let giant = file.lines.iter().map(String::len).sum::<usize>() > GIANT_FILE_CHARS;
            @let rows = file.lines.iter().filter(|l| !l.is_empty()).count();
            @let intrinsic = format!("contain-intrinsic-size:auto {}px", rows * ROW_PX);
            details open[!giant]
                id=(slug(&file.path))
                class={
                    "file " (status.css_class) " group/file mb-2.5 rounded-panel border border-line bg-surface "
                    "[&:not([open])>summary]:rounded-panel [&:not([open])>summary]:border-b-0 "
                    "[&.status-added>summary]:bg-[color-mix(in_srgb,var(--add-bg)_34%,var(--surface-2))] "
                    "[&.status-deleted>summary]:bg-[color-mix(in_srgb,var(--del-bg)_34%,var(--surface-2))] "
                    "[&.flash]:outline [&.flash]:outline-acc [&.flash]:outline-offset-[-1px] "
                    "print:break-inside-avoid print:[&[hidden]]:block!"
                }
                data-path=(file.path)
                data-comment=(crate::comment_syntax::comment_leader(&file.path))
                data-commits=(file_commits(file))
                data-status=(status.key)
                data-status-code=(status.code)
                data-status-label=(status.label) {
                summary class="sticky top-0 z-2 flex cursor-pointer list-none items-center gap-2 rounded-t-panel border-b border-line bg-surface-2 px-2.5 py-2 text-[12.5px] hover:bg-line [&::-webkit-details-marker]:hidden print:static print:bg-[#f2f2f2]" {
                    span class="file-caret size-0 flex-none border-y-4 border-y-transparent border-l-5 border-l-ink-3 group-open/file:rotate-90" aria-hidden="true" {}
                    span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-ink" { (file.path) }
                    span class={ "status-badge " (status.css_class) " " (STATUS_BADGE_CLASSES) " " (status.badge_classes) } title=(status.label) aria-label=(status.label) { (status.code) }
                    span class="copies flex flex-none gap-[5px] print:hidden!" {
                        button type="button" class=(COPY_BUTTON_CLASSES) data-copy-value=(file.path) data-copy-label="path" { "path" }
                        button type="button" class=(COPY_BUTTON_CLASSES) data-copy-value=(absolute) data-copy-label="abs" { "abs" }
                        // ! mode="code" carries no payload: the button reads its own file's
                        // ! already-rendered diff rows at click time (no per-file content dupe).
                        button type="button" class=(COPY_BUTTON_CLASSES) data-copy-mode="code" data-copy-label="code" { "code" }
                    }
                    span class="flex-none text-[12.5px]" { span.a { "+" (file.added) } " " span.d { "−" (file.removed) } }
                }
                // ! Keep containment below the sticky summary so it can pin to `.main`.
                div class="filebody single-variant [content-visibility:auto] overflow-hidden rounded-b-panel print:block! print:[content-visibility:visible] print:overflow-visible" style=(intrinsic) {
                    (file_diff(file, options))
                }
            }
        }
    }
}

fn file_diff(file: &FileDiff, options: RenderOptions) -> Markup {
    let (lines, density) = match (options.density(), file.full_lines.as_ref()) {
        (DiffDensity::Full, Some(full_lines)) => (full_lines, DiffDensity::Full),
        (DiffDensity::Full | DiffDensity::Compact, _) => (&file.lines, DiffDensity::Compact),
    };
    match (options.layout(), density) {
        (DiffLayout::Unified, DiffDensity::Compact) => html! {
            div class={ "diff diff-unified diff-compact " (DIFF_CLASSES) } { (PreEscaped(render_diff_lines(lines, &file.owners))) }
        },
        (DiffLayout::Split, DiffDensity::Compact) => html! {
            div class={ "diff diff-split diff-compact " (DIFF_CLASSES) } { (PreEscaped(render_diff_split(lines, &file.owners))) }
        },
        (DiffLayout::Unified, DiffDensity::Full) => html! {
            div class={ "diff diff-unified diff-full " (DIFF_CLASSES) } { (PreEscaped(render_diff_lines(lines, &file.owners))) }
        },
        (DiffLayout::Split, DiffDensity::Full) => html! {
            div class={ "diff diff-split diff-full " (DIFF_CLASSES) } { (PreEscaped(render_diff_split(lines, &file.owners))) }
        },
    }
}

#[cfg(test)]
mod tests {
    use application::{
        diffs::{FileDiff, LineOwners},
        viewer::{DiffDensity, DiffLayout, RenderOptions},
    };

    use super::{GIANT_FILE_CHARS, ROW_PX};
    use crate::{build_html, fixtures::sample_view, view_fragment};

    #[test]
    fn build_html_marks_file_status_for_sidebar_tree() {
        let mut view = sample_view();
        view.files = vec![
            FileDiff {
                path: "src/new.rs".to_string(),
                added: 1,
                removed: 0,
                lines: vec![
                    "new file mode 100644".to_string(),
                    "--- /dev/null".to_string(),
                    "+++ b/src/new.rs".to_string(),
                    "@@ -0,0 +1 @@".to_string(),
                    "+hello".to_string(),
                ],
                full_lines: None,
                commits: vec!["abc123def".to_string()],
                owners: LineOwners::default(),
            },
            FileDiff {
                path: "src/gone.rs".to_string(),
                added: 0,
                removed: 1,
                lines: vec![
                    "deleted file mode 100644".to_string(),
                    "--- a/src/gone.rs".to_string(),
                    "+++ /dev/null".to_string(),
                    "@@ -1 +0,0 @@".to_string(),
                    "-bye".to_string(),
                ],
                full_lines: None,
                commits: vec!["abc123def".to_string()],
                owners: LineOwners::default(),
            },
            FileDiff {
                path: "src/new-name.rs".to_string(),
                added: 0,
                removed: 0,
                lines: vec![
                    "similarity index 100%".to_string(),
                    "rename from src/old-name.rs".to_string(),
                    "rename to src/new-name.rs".to_string(),
                ],
                full_lines: None,
                commits: vec!["abc123def".to_string()],
                owners: LineOwners::default(),
            },
        ];

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains(r#"data-status="added""#));
        assert!(html.contains(r#"data-status-label="Added file""#));
        assert!(html.contains(r#"class="file status-added "#));
        assert!(html.contains(r#"data-status="deleted""#));
        assert!(html.contains(r#"data-status-label="Deleted file""#));
        assert!(html.contains(r#"class="file status-deleted "#));
        assert!(html.contains(r#"data-status="renamed""#));
        assert!(html.contains(r#"data-status-label="Renamed file""#));
        assert!(html.contains(r#"class="file status-renamed "#));
        assert!(html.contains("tstatus"));
        assert!(html.contains(r#"class="status-badge status-added inline-flex size-[15px]"#));
        assert!(html.contains("border-add-line bg-add-bg text-add"));
        let added_badge = html
            .split_once(r#"class="status-badge status-added "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("added badge classes");
        assert!(!added_badge.contains("border-line-2"));
    }

    #[test]
    fn app_fragment_emits_only_the_requested_variant() {
        let html = view_fragment(
            &sample_view(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
        )
        .into_string();

        assert!(html.contains(r#"class="diff diff-split diff-full "#));
        assert!(!html.contains(r#"class="diff diff-unified"#));
        assert!(!html.contains(r#"class="diff diff-split diff-compact"#));
    }

    #[test]
    fn app_fragment_full_density_falls_back_to_compact_source_lines() {
        let mut view = sample_view();
        view.files[0].full_lines = None;

        let html = view_fragment(
            &view,
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Full),
        )
        .into_string();

        assert!(html.contains(r#"class="diff diff-unified diff-compact "#));
        assert!(html.contains("+extra"));
        assert!(!html.contains("diff-full"));
    }

    #[test]
    fn each_file_emits_exactly_one_requested_layout_and_density_block() {
        let mut view = sample_view();
        view.files.push(view.files[0].clone());
        view.files[1].path = "src/second.rs".to_string();
        let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);

        let html = build_html(&view, options, None);

        assert_eq!(
            html.matches(r#"class="diff diff-split diff-full "#).count(),
            2
        );
        assert_eq!(html.matches(r#"class="diff diff-unified"#).count(), 0);
        assert_eq!(
            html.matches(r#"class="diff diff-split diff-compact"#)
                .count(),
            0
        );
    }

    #[test]
    fn build_html_file_block_carries_copy_buttons() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        // relative path copy button: data-copy-value attribute
        assert!(html.contains(r#"data-copy-value="src/a b.rs" data-copy-label="path""#));
        // absolute path copy button: repo_root + "/" + path (POSIX join)
        assert!(
            html.contains(r#"data-copy-value="/home/user/api/src/a b.rs" data-copy-label="abs""#)
        );
        // copy-code-without-markers reads its own file's rendered rows at click time
        assert!(html.contains(r#"data-copy-mode="code""#));
    }

    #[test]
    fn build_html_copy_button_absolute_path_escapes_user_controlled_values() {
        let mut view = sample_view();
        view.repo_root = "/tmp/<r>".to_string();
        view.files[0].path = "src/<x>&\".rs".to_string();

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        // both relative and absolute data-copy-value attributes stay Maud-escaped
        assert!(
            html.contains(
                r#"data-copy-value="src/&lt;x&gt;&amp;&quot;.rs" data-copy-label="path""#
            )
        );
        assert!(html.contains(
            r#"data-copy-value="/tmp/&lt;r&gt;/src/&lt;x&gt;&amp;&quot;.rs" data-copy-label="abs""#
        ));
        assert!(!html.contains("<x>"));
    }

    #[test]
    fn file_blocks_collapses_giant_files() {
        let mut view = sample_view();
        let huge = "+".to_string() + &"x".repeat(GIANT_FILE_CHARS);
        view.files[0].lines = vec!["@@ -0,0 +1 @@".to_string(), huge];
        view.files[0].full_lines = None;
        let html = build_html(&view, RenderOptions::DEFAULT, None);
        // the giant file's <details> renders WITHOUT `open` (Maud emits `open` before `id`)
        assert!(html.contains(r#"<details id="f-src-a-b-rs""#));
        assert!(!html.contains(r#"<details open id="f-src-a-b-rs""#));
    }

    #[test]
    fn file_blocks_emits_per_file_intrinsic_size() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);
        // sample file renders 4 rows -> 4 * ROW_PX
        assert!(html.contains(&format!("contain-intrinsic-size:auto {}px", 4 * ROW_PX)));
    }

    #[test]
    fn build_html_ships_content_visibility_perf_rule() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT).into_string();

        assert!(html.contains("[content-visibility:auto]"));
        assert!(html.contains("print:[content-visibility:visible]"));
        assert!(html.contains("print:block!"));
    }

    #[test]
    fn content_visibility_stays_in_css_not_inline_so_print_override_wins() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);
        // content-visibility must NOT be inline (an inline style out-specifies the @media print
        // override)
        assert!(!html.contains(r#"style="content-visibility"#));
        // the old inline pattern must not appear (CSS rule contains this substring but not as an
        // inline style)
        assert!(!html.contains(r#"style="content-visibility:auto;contain-intrinsic-size"#));
        // utilities keep both screen and print behavior discoverable in the file body markup
        assert!(html.contains("[content-visibility:auto]"));
        assert!(html.contains("print:[content-visibility:visible]"));
        // per-file intrinsic-size is still emitted inline
        assert!(html.contains(&format!("contain-intrinsic-size:auto {}px", 4 * ROW_PX)));
    }
}
