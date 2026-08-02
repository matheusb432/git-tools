//! Center column: one `<details>` block per changed file, carrying the copy
//! buttons, status badge, and selected diff pane.

mod chunks;

pub(super) use chunks::{chunk_fragment, chunk_loader, view_chunks};
use gtl_application::{
    diffs::{FileDiff, FileStatus, View},
    viewer::{DiffDensity, DiffLayout, RenderOptions},
};
use maud::{Markup, PreEscaped, html};

use crate::{
    layout::{Surface, file_status::file_status_presentation},
    rows::{render_diff_lines, render_diff_split, unified_line_number_digits},
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
const OPEN_IN_EDITOR_BUTTON_CLASSES: &str = concat!(
    "open-in-editor cursor-pointer flex size-[22px] flex-none items-center justify-center rounded-sm border border-transparent bg-transparent text-ink-2 [font:inherit] ",
    "hover:border-acc-line hover:bg-acc-soft hover:text-acc active:bg-acc-soft ",
    "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc ",
    "disabled:pointer-events-none disabled:cursor-progress disabled:border-line-2 disabled:bg-surface-2 disabled:text-ink",
);
const OPEN_IN_EDITOR_ICON: &str = r#"<svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><path d="M9 2.5h4.5V7"></path><path d="m13.5 2.5-7 7"></path><path d="M7 4H3.5A1.5 1.5 0 0 0 2 5.5v7A1.5 1.5 0 0 0 3.5 14h7a1.5 1.5 0 0 0 1.5-1.5V9"></path></svg>"#;
const DIFF_CLASSES: &str = concat!(
    "gtl-scroll-rail overflow-x-hidden text-[14px] leading-[22px] ",
    "print:[&_.dl_code]:text-[#111]",
);
const STATUS_BADGE_CLASSES: &str = "inline-flex size-[15px] flex-none items-center justify-center rounded-sm border text-[9.5px] leading-none font-bold";

// ! The app webview (wry/WebKitGTK) never marks swapped-in content-visibility:auto
// ! subtrees relevant, so their rows would stay unpainted; only browser artifacts
// ! opt into the offscreen-skip optimization (and its print escape hatches).
fn filebody_presentation(surface: Surface, file: &FileDiff) -> (&'static str, Option<String>) {
    match surface {
        Surface::App { .. } => (
            "filebody single-variant overflow-hidden rounded-b-panel",
            None,
        ),
        Surface::Artifact { .. } => {
            let rows = file.lines.iter().filter(|l| !l.is_empty()).count();
            (
                "filebody single-variant [content-visibility:auto] overflow-hidden rounded-b-panel print:block! print:[content-visibility:visible] print:overflow-visible",
                Some(format!("contain-intrinsic-size:auto {}px", rows * ROW_PX)),
            )
        }
    }
}

pub(super) fn file_blocks(view: &View, options: RenderOptions, surface: Surface) -> Markup {
    file_blocks_with_mode(view, options, surface, FileBodyMode::Complete)
}

pub(super) fn file_block_shells(view: &View, options: RenderOptions, surface: Surface) -> Markup {
    file_blocks_with_mode(view, options, surface, FileBodyMode::Shell)
}

#[derive(Clone, Copy)]
enum FileBodyMode {
    Complete,
    Shell,
}

fn file_blocks_with_mode(
    view: &View,
    options: RenderOptions,
    surface: Surface,
    mode: FileBodyMode,
) -> Markup {
    if view.files.is_empty() {
        return html! { div class="empty rounded-panel border border-dashed border-line-2 p-4 text-center text-ink-2 italic" { "no file changes" } };
    }

    html! {
        @for (file_index, file) in view.files.iter().enumerate() {
            @let absolute = format!("{}/{}", view.repo_root, file.path);
            @let status = file_status_presentation(file.status());
            @let giant = file.lines.iter().map(String::len).sum::<usize>() > GIANT_FILE_CHARS;
            @let (filebody_classes, intrinsic) = filebody_presentation(surface, file);
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
                {
                summary class="sticky top-0 z-2 flex cursor-pointer list-none items-center gap-2 rounded-t-panel border-b border-line bg-surface-2 px-2.5 py-2 text-[12.5px] hover:bg-line [&::-webkit-details-marker]:hidden mobile:flex-wrap mobile:gap-x-1.5 mobile:px-2 mobile:py-1.5 print:static print:bg-[#f2f2f2]" {
                    span class="file-caret size-0 flex-none border-y-4 border-y-transparent border-l-5 border-l-ink-3 group-open/file:rotate-90" aria-hidden="true" {}
                    span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-ink" { (file.path) }
                    span class={ "status-badge " (status.css_class) " " (STATUS_BADGE_CLASSES) " " (status.badge_classes) } title=(status.label) aria-label=(status.label) { (status.code) }
                    span class="file-actions flex flex-none items-center gap-2 mobile:basis-full mobile:justify-end" {
                        span class="copies flex flex-none gap-[5px] print:hidden!" {
                            button type="button" class=(COPY_BUTTON_CLASSES) data-copy-value=(file.path) data-copy-label="path" { "path" }
                            button type="button" class=(COPY_BUTTON_CLASSES) data-copy-value=(absolute) data-copy-label="abs" { "abs" }
                            // ! mode="code" carries no payload: the button reads its own file's
                            // ! already-rendered diff rows at click time (no per-file content dupe).
                            button type="button" class=(COPY_BUTTON_CLASSES) data-copy-mode="code" data-copy-label="code" { "code" }
                        }
                        span class="flex-none text-[12.5px]" { span.a { "+" (file.added) } " " span.d { "−" (file.removed) } }
                        @if let Surface::App { tab_id } = surface {
                            @if file.status() != FileStatus::Deleted {
                                button type="button"
                                    class=(OPEN_IN_EDITOR_BUTTON_CLASSES)
                                    aria-label="Open in IDE"
                                    title="Open in IDE"
                                    hx-post=(open_diff_file_route(tab_id, &file.path))
                                    hx-disabled-elt="this"
                                    hx-sync="this:drop"
                                    hx-swap="none" {
                                    (PreEscaped(OPEN_IN_EDITOR_ICON))
                                }
                            }
                        }
                    }
                }
                // ! Keep containment below the sticky summary so it can pin to `.main`.
                div class=(filebody_classes) style=[intrinsic] {
                    @match mode {
                        FileBodyMode::Complete => (file_diff(file, options)),
                        FileBodyMode::Shell => (file_diff_shell(file, options, file_index)),
                    }
                }
            }
        }
    }
}

fn file_diff_shell(file: &FileDiff, options: RenderOptions, file_index: usize) -> Markup {
    let (lines, density) = selected_lines(file, options);
    let style = unified_line_number_gutter_style(options.layout(), lines);
    html! {
        div id=(diff_target_id(file_index))
            class=(diff_classes(options.layout(), density))
            style=[style] {}
    }
}

fn open_diff_file_route(tab_id: gtl_application::viewer::ViewerTabId, path: &str) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("path", path)
        .finish();
    format!("/tabs/{tab_id}/files/open?{query}")
}

fn file_diff(file: &FileDiff, options: RenderOptions) -> Markup {
    let syntax = crate::syntax::syntax_for_path(&file.path);
    let (lines, density) = selected_lines(file, options);
    let style = unified_line_number_gutter_style(options.layout(), lines);
    html! {
        div class=(diff_classes(options.layout(), density)) style=[style] {
            (PreEscaped(render_rows(options.layout(), lines, syntax)))
        }
    }
}

fn selected_lines(file: &FileDiff, options: RenderOptions) -> (&[String], DiffDensity) {
    match (options.density(), file.full_lines.as_ref()) {
        (DiffDensity::Full, Some(full_lines)) => (full_lines, DiffDensity::Full),
        (DiffDensity::Full | DiffDensity::Compact, _) => (&file.lines, DiffDensity::Compact),
    }
}

fn diff_classes(layout: DiffLayout, density: DiffDensity) -> String {
    format!(
        "diff diff-{layout} diff-{density} {DIFF_CLASSES}",
        layout = match layout {
            DiffLayout::Unified => "unified",
            DiffLayout::Split => "split",
        },
        density = match density {
            DiffDensity::Compact => "compact",
            DiffDensity::Full => "full",
        },
    )
}

fn unified_line_number_gutter_style(layout: DiffLayout, lines: &[String]) -> Option<String> {
    match layout {
        DiffLayout::Unified => Some(format!(
            "--unified-line-number-width:calc({}ch + 8px)",
            unified_line_number_digits(lines)
        )),
        DiffLayout::Split => None,
    }
}

fn render_rows(
    layout: DiffLayout,
    lines: &[String],
    syntax: Option<&syntect::parsing::SyntaxReference>,
) -> String {
    match layout {
        DiffLayout::Unified => render_diff_lines(lines, syntax),
        DiffLayout::Split => render_diff_split(lines, syntax),
    }
}

fn diff_target_id(file_index: usize) -> String {
    format!("viewer-diff-{file_index}")
}

#[cfg(test)]
mod tests {
    use gtl_application::{
        diffs::FileDiff,
        viewer::{DiffDensity, DiffLayout, RenderOptions, ViewerTabId},
    };

    use super::{GIANT_FILE_CHARS, ROW_PX};
    use crate::{build_html, fixtures::sample_view, view_fragment};

    fn tab_id(raw: u64) -> ViewerTabId {
        ViewerTabId::try_new(raw).expect("positive tab id")
    }

    fn file(path: &str, status_line: &str) -> FileDiff {
        FileDiff {
            path: path.to_string(),
            added: 1,
            removed: 1,
            lines: vec![status_line.to_string(), "@@ -1 +1 @@".to_string()],
            full_lines: None,
        }
    }

    fn added_file(path: &str) -> FileDiff {
        file(path, "new file mode 100644")
    }

    fn modified_file(path: &str) -> FileDiff {
        file(path, "index abc1234..def5678 100644")
    }

    fn renamed_file(path: &str) -> FileDiff {
        file(path, "rename from src/old.rs")
    }

    fn deleted_file(path: &str) -> FileDiff {
        file(path, "deleted file mode 100644")
    }

    #[test]
    fn app_fragment_renders_one_accessible_open_action_for_each_non_deleted_file() {
        let mut view = sample_view();
        view.files = vec![
            added_file("src/added.rs"),
            modified_file("src/modified.rs"),
            renamed_file("src/renamed.rs"),
            deleted_file("src/deleted.rs"),
        ];

        let html = view_fragment(&view, RenderOptions::DEFAULT, tab_id(7)).into_string();

        assert_eq!(html.matches(r#"aria-label="Open in IDE""#).count(), 3);
        assert_eq!(html.matches(r#"title="Open in IDE""#).count(), 3);
        assert_eq!(html.matches(r#"hx-disabled-elt="this""#).count(), 3);
        assert_eq!(html.matches(r#"hx-sync="this:drop""#).count(), 3);
        assert!(html.contains("disabled:pointer-events-none"));
        assert!(!html.contains("path=src%2Fdeleted.rs"));
    }

    #[test]
    fn app_fragment_encodes_the_tab_and_repository_relative_path() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id(7)).into_string();

        assert!(html.contains(r#"hx-post="/tabs/7/files/open?path=src%2Fa+b.rs""#));
    }

    #[test]
    fn build_html_marks_file_status_in_files_and_server_tree() {
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
            },
        ];

        let html = build_html(&view, RenderOptions::DEFAULT, None);

        assert!(html.contains(r#"class="tnode tfile status-added""#));
        assert!(html.contains(r#"class="tstatus status-added" title="Added file""#));
        assert!(html.contains(r#"class="file status-added "#));
        assert!(html.contains(r#"class="tnode tfile status-deleted""#));
        assert!(html.contains(r#"class="tstatus status-deleted" title="Deleted file""#));
        assert!(html.contains(r#"class="file status-deleted "#));
        assert!(html.contains(r#"class="tnode tfile status-renamed""#));
        assert!(html.contains(r#"class="tstatus status-renamed" title="Renamed file""#));
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
            tab_id(1),
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
            tab_id(1),
        )
        .into_string();

        assert!(html.contains(r#"class="diff diff-unified diff-compact "#));
        assert!(html.contains("extra"));
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
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        assert!(html.contains("[content-visibility:auto]"));
        assert!(html.contains("print:[content-visibility:visible]"));
        assert!(html.contains("print:block!"));
    }

    #[test]
    fn app_fragment_omits_content_visibility_for_the_webview() {
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id(1)).into_string();

        assert!(!html.contains("content-visibility"));
        assert!(!html.contains("contain-intrinsic-size"));
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
