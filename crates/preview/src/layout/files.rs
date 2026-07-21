//! Center column: one `<details>` block per changed file, carrying the copy
//! buttons, status badge, and the diff pane(s) for the hosting surface.

use application::{
    diffs::{FileDiff, FileStatus, View},
    viewer::{DiffDensity, DiffLayout},
};
use maud::{Markup, PreEscaped, html};

use super::Surface;
use crate::{
    rows::{render_diff_lines, render_diff_split},
    text::slug,
};

const GIANT_FILE_CHARS: usize = 250_000;
const ROW_PX: usize = 22;

#[derive(Clone, Copy)]
struct FileStatusPresentation {
    key: &'static str,
    code: &'static str,
    label: &'static str,
    css_class: &'static str,
}

const STATUS_ADDED: FileStatusPresentation = FileStatusPresentation {
    key: "added",
    code: "A",
    label: "Added file",
    css_class: "status-added",
};
const STATUS_DELETED: FileStatusPresentation = FileStatusPresentation {
    key: "deleted",
    code: "D",
    label: "Deleted file",
    css_class: "status-deleted",
};
const STATUS_RENAMED: FileStatusPresentation = FileStatusPresentation {
    key: "renamed",
    code: "R",
    label: "Renamed file",
    css_class: "status-renamed",
};
const STATUS_MODIFIED: FileStatusPresentation = FileStatusPresentation {
    key: "modified",
    code: "M",
    label: "Modified file",
    css_class: "status-modified",
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

pub(super) fn file_blocks(view: &View, surface: Surface) -> Markup {
    if view.files.is_empty() {
        return html! { div.empty { "no file changes" } };
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
                class=(format!("file {}", status.css_class))
                data-path=(file.path)
                data-comment=(crate::comment_syntax::comment_leader(&file.path))
                data-commits=(file_commits(file))
                data-status=(status.key)
                data-status-code=(status.code)
                data-status-label=(status.label) {
                summary {
                    span.path { (file.path) }
                    span class=(format!("status-badge {}", status.css_class)) title=(status.label) aria-label=(status.label) { (status.code) }
                    span.copies {
                        button type="button" class="copy-button" data-copy-value=(file.path) data-copy-label="path" { "path" }
                        button type="button" class="copy-button" data-copy-value=(absolute) data-copy-label="abs" { "abs" }
                        // ! mode="code" carries no payload: the button reads its own file's
                        // ! already-rendered diff rows at click time (no per-file content dupe).
                        button type="button" class="copy-button" data-copy-mode="code" data-copy-label="code" { "code" }
                    }
                    span.filestat { span.a { "+" (file.added) } " " span.d { "−" (file.removed) } }
                }
                // ! Diff rows live in their own body so content-visibility virtualizes the
                // ! heavy content here while the summary stays sticky against `.main` (size
                // ! containment on `details.file` itself would trap the sticky in the box).
                // ! Artifacts ship four variants selected by <html> data attributes; app
                // ! fragments ship one visible variant selected by validated server options.
                div class=(if matches!(surface, Surface::Artifact) { "filebody" } else { "filebody single-variant" }) style=(intrinsic) {
                    (file_diff(file, surface))
                }
            }
        }
    }
}

fn file_diff(file: &FileDiff, surface: Surface) -> Markup {
    match surface {
        Surface::Artifact => html! {
            div class="diff diff-split diff-compact" { (PreEscaped(render_diff_split(&file.lines, &file.owners))) }
            div class="diff diff-unified diff-compact" { (PreEscaped(render_diff_lines(&file.lines, &file.owners))) }
            @if let Some(full_lines) = &file.full_lines {
                div class="diff diff-split diff-full" { (PreEscaped(render_diff_split(full_lines, &file.owners))) }
                div class="diff diff-unified diff-full" { (PreEscaped(render_diff_lines(full_lines, &file.owners))) }
            }
        },
        Surface::App(options) => {
            let lines = match options.density() {
                DiffDensity::Compact => &file.lines,
                DiffDensity::Full => file.full_lines.as_ref().unwrap_or(&file.lines),
            };
            match (options.layout(), options.density()) {
                (DiffLayout::Unified, DiffDensity::Compact) => html! {
                    div class="diff diff-unified diff-compact" { (PreEscaped(render_diff_lines(lines, &file.owners))) }
                },
                (DiffLayout::Split, DiffDensity::Compact) => html! {
                    div class="diff diff-split diff-compact" { (PreEscaped(render_diff_split(lines, &file.owners))) }
                },
                (DiffLayout::Unified, DiffDensity::Full) => html! {
                    div class="diff diff-unified diff-full" { (PreEscaped(render_diff_lines(lines, &file.owners))) }
                },
                (DiffLayout::Split, DiffDensity::Full) => html! {
                    div class="diff diff-split diff-full" { (PreEscaped(render_diff_split(lines, &file.owners))) }
                },
            }
        }
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

        let html = build_html(&view);

        assert!(html.contains(r#"data-status="added""#));
        assert!(html.contains(r#"data-status-label="Added file""#));
        assert!(html.contains(r#"class="file status-added""#));
        assert!(html.contains(r#"data-status="deleted""#));
        assert!(html.contains(r#"data-status-label="Deleted file""#));
        assert!(html.contains(r#"class="file status-deleted""#));
        assert!(html.contains(r#"data-status="renamed""#));
        assert!(html.contains(r#"data-status-label="Renamed file""#));
        assert!(html.contains(r#"class="file status-renamed""#));
        assert!(html.contains("tstatus"));
    }

    #[test]
    fn app_fragment_emits_only_the_requested_variant() {
        let html = view_fragment(
            &sample_view(),
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
        )
        .into_string();

        assert!(html.contains(r#"class="diff diff-split diff-full""#));
        assert!(!html.contains("diff-unified"));
        assert!(!html.contains("diff-compact"));
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

        assert!(html.contains(r#"class="diff diff-unified diff-full""#));
        assert!(html.contains("+extra"));
        assert!(!html.contains("diff-compact"));
    }

    #[test]
    fn build_html_file_block_carries_copy_buttons() {
        let html = build_html(&sample_view());

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

        let html = build_html(&view);

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
        let html = build_html(&view);
        // the giant file's <details> renders WITHOUT `open` (Maud emits `open` before `id`)
        assert!(html.contains(r#"<details id="f-src-a-b-rs""#));
        assert!(!html.contains(r#"<details open id="f-src-a-b-rs""#));
    }

    #[test]
    fn file_blocks_emits_per_file_intrinsic_size() {
        let html = build_html(&sample_view());
        // sample file renders 4 rows -> 4 * ROW_PX
        assert!(html.contains(&format!("contain-intrinsic-size:auto {}px", 4 * ROW_PX)));
    }

    #[test]
    fn build_html_ships_content_visibility_perf_rule() {
        let html = build_html(&sample_view());

        assert!(html.contains("content-visibility:auto"));
    }

    #[test]
    fn content_visibility_stays_in_css_not_inline_so_print_override_wins() {
        let html = build_html(&sample_view());
        // content-visibility must NOT be inline (an inline style out-specifies the @media print
        // override)
        assert!(!html.contains(r#"style="content-visibility"#));
        // the old inline pattern must not appear (CSS rule contains this substring but not as an
        // inline style)
        assert!(!html.contains(r#"style="content-visibility:auto;contain-intrinsic-size"#));
        // it still lives in the stylesheet, and the print override is present
        assert!(html.contains("content-visibility:auto")); // base CSS rule
        assert!(html.contains("content-visibility:visible")); // @media print override
        // per-file intrinsic-size is still emitted inline
        assert!(html.contains(&format!("contain-intrinsic-size:auto {}px", 4 * ROW_PX)));
    }
}
