//! Left sidebar: the server-rendered changed-files tree, filter input, and range stats.

use gtl_application::{
    diffs::{FileDiff, View},
    viewer::diff_file_anchor_id,
};
use maud::{Markup, html};

use super::file_status::file_status_presentation;
use crate::text::plural;

const TREE_PRESENTATION_CLASSES: &str = concat!(
    "tree gtl-scroll [grid-area:2/1] overflow-auto border-r border-line bg-surface p-3 ",
    "compact:p-2.5 tablet:hidden print:hidden! ",
    "[&_.tree-body_ul]:m-0 [&_.tree-body_ul]:list-none [&_.tree-body_ul]:pl-2.5 [&_.tree-body>ul]:pl-0 ",
    "[&_.tnode]:min-w-0 ",
    "[&_.tlabel]:flex [&_.tlabel]:cursor-pointer [&_.tlabel]:items-center [&_.tlabel]:gap-1.5 [&_.tlabel]:rounded-sm [&_.tlabel]:border-l-2 [&_.tlabel]:border-transparent [&_.tlabel]:px-1.5 [&_.tlabel]:py-0.5 [&_.tlabel]:leading-snug [&_.tlabel]:text-ink-2 ",
    "[&_.tdir>ul_.tfile>.tlabel]:pl-2 [&_.tlabel:hover]:bg-surface-2 [&_.tlabel:hover]:text-ink ",
    "[&_:is(.tdir>.tlabel,.tdir>.tlabel_.tname)]:text-ink-3 ",
    "[&_.tfile.cur>.tlabel]:border-l-2 [&_.tfile.cur>.tlabel]:border-acc [&_.tfile.cur>.tlabel]:bg-acc-soft [&_.tfile.cur>.tlabel]:text-ink ",
    "[&_.tcaret]:size-0 [&_.tcaret]:flex-none [&_.tcaret]:border-y-4 [&_.tcaret]:border-y-transparent [&_.tcaret]:border-l-5 [&_.tcaret]:border-l-ink-3 ",
    "[&_.tdir.open>.tlabel_.tcaret]:rotate-90 [&_.tdir:not(.open)>ul]:hidden ",
    "[&_.tname]:min-w-0 [&_.tname]:flex-1 [&_.tname]:overflow-hidden [&_.tname]:text-ellipsis ",
    "[&_.tfile.status-added>.tlabel]:bg-add-bg/40 [&_.tfile.status-deleted>.tlabel]:bg-del-bg/40 ",
    "[&_.tfile.status-added>.tlabel:hover]:bg-add-bg/60 [&_.tfile.status-deleted>.tlabel:hover]:bg-del-bg/60 ",
    "[&_.tstatus]:inline-flex [&_.tstatus]:size-4 [&_.tstatus]:flex-none [&_.tstatus]:items-center [&_.tstatus]:justify-center [&_.tstatus]:rounded-sm [&_.tstatus]:border [&_.tstatus]:border-line-2 [&_.tstatus]:text-xs [&_.tstatus]:leading-none [&_.tstatus]:font-bold ",
    "[&_.tstatus.status-added]:border-add-line [&_.tstatus.status-added]:bg-add-bg [&_.tstatus.status-added]:text-add ",
    "[&_.tstatus.status-deleted]:border-del-line [&_.tstatus.status-deleted]:bg-del-bg [&_.tstatus.status-deleted]:text-del ",
    "[&_.tstatus.status-renamed]:border-acc-line [&_.tstatus.status-renamed]:bg-acc-soft [&_.tstatus.status-renamed]:text-acc ",
    "[&_.tstatus.status-modified]:bg-sunk [&_.tstatus.status-modified]:text-ink-3",
);
const STAT_CLASSES: &str = "rounded-sm border px-2 py-0.5 text-xs";

pub(super) struct ChangedFilesPresentation<'view> {
    commits_label: &'view str,
    commit_count: usize,
    files: &'view [FileDiff],
    total_added: u32,
    total_removed: u32,
    root: TreeDirectory<'view>,
}

impl<'view> ChangedFilesPresentation<'view> {
    pub(super) fn new(view: &'view View) -> Self {
        Self {
            commits_label: &view.commits_label,
            commit_count: view.commits.len(),
            files: &view.files,
            total_added: view.files.iter().map(|file| file.added).sum(),
            total_removed: view.files.iter().map(|file| file.removed).sum(),
            root: TreeDirectory::from_files(&view.files),
        }
    }
}

// ! `.search` and `.tree-body` are enhancer hooks. The tree owns presentation for its
// ! server-rendered descendants.
pub(super) fn tree(presentation: &ChangedFilesPresentation<'_>) -> Markup {
    html! {
        aside class=(TREE_PRESENTATION_CLASSES) aria-label="Changed files tree" {
            div class="search relative mb-3 print:hidden!" {
                input type="text"
                    class="filter w-full rounded-sm border border-line-2 bg-sunk px-2.5 py-2 text-ink outline-none hover:border-ink-3 focus-visible:border-acc focus-visible:ring-2 focus-visible:ring-acc-soft"
                    placeholder="Filter files…  /" aria-label="Filter files";
            }
            div class="mx-1 mt-1.5 mb-2 flex justify-between tracking-wider text-ink-3 uppercase" {
                span { (presentation.commits_label) " · " (presentation.files.len()) " file" (plural(presentation.files.len())) }
            }
            div class="mx-0.5 mb-3 flex flex-wrap gap-2" {
                span class={ (STAT_CLASSES) " border-line-2 text-ink-2" } { b class="font-bold text-ink" { (presentation.commit_count) } " commit" (plural(presentation.commit_count)) }
                span class={ (STAT_CLASSES) " border-add-line text-add" } { "+" (presentation.total_added) }
                span class={ (STAT_CLASSES) " border-del-line text-del" } { "−" (presentation.total_removed) }
            }
            div class="tree-body whitespace-nowrap" {
                (render_directory(&presentation.root))
            }
        }
    }
}

#[derive(Default)]
struct TreeDirectory<'a> {
    directories: Vec<(&'a str, TreeDirectory<'a>)>,
    files: Vec<(&'a str, &'a FileDiff)>,
}

impl<'view> TreeDirectory<'view> {
    fn from_files(files: &'view [FileDiff]) -> Self {
        let mut root = Self::default();
        for file in files {
            let mut directory = &mut root;
            let mut segments = file.path.split('/').peekable();
            while let Some(segment) = segments.next() {
                if segments.peek().is_none() {
                    directory.files.push((segment, file));
                    break;
                }
                let index = directory
                    .directories
                    .iter()
                    .position(|(name, _)| *name == segment)
                    .unwrap_or_else(|| {
                        directory.directories.push((segment, Self::default()));
                        directory.directories.len() - 1
                    });
                directory = &mut directory.directories[index].1;
            }
        }
        root
    }
}

fn render_directory(directory: &TreeDirectory<'_>) -> Markup {
    html! {
        ul {
            @for (name, child) in &directory.directories {
                li class="tnode tdir open" {
                    div class="tlabel" {
                        span class="tcaret" {}
                        span class="tname" { (name) }
                    }
                    (render_directory(child))
                }
            }
            @for (name, file) in &directory.files {
                @let status = file_status_presentation(file.status());
                li class={ "tnode tfile " (status.css_class) }
                    data-target=(diff_file_anchor_id(&file.path))
                    data-path=(file.path.to_lowercase()) {
                    div class="tlabel" {
                        span class={ "tstatus " (status.css_class) } title=(status.label) {
                            (status.code)
                        }
                        span class="tname" { (name) }
                    }
                }
            }
        }
    }
}

pub(super) fn mobile_popover(presentation: &ChangedFilesPresentation<'_>, target: &str) -> Markup {
    html! {
        aside id=(target)
            data-artifact-files-popover
            class="fixed inset-3 m-0 h-[calc(100vh_-_24px)] w-[calc(100vw_-_24px)] max-w-none overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)]"
            aria-label="Changed files"
            popover {
            header class="flex items-center justify-between border-b border-line bg-surface-2 px-4 py-3" {
                div {
                    strong class="block" { "Changed files" }
                    div class="flex flex-wrap items-center gap-1.5 pt-1" {
                        span class="text-xs text-ink-3" {
                            (presentation.files.len()) " file" (plural(presentation.files.len()))
                        }
                        span class={ (STAT_CLASSES) " border-add-line text-add" } title="Lines added" {
                            "+" (presentation.total_added)
                        }
                        span class={ (STAT_CLASSES) " border-del-line text-del" } title="Lines removed" {
                            "−" (presentation.total_removed)
                        }
                    }
                }
                button type="button" class="size-8 cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 hover:bg-line hover:text-ink active:bg-acc-soft focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    popovertarget=(target) popovertargetaction="hide" aria-label="Close changed files" { "×" }
            }
            div class="gtl-scroll h-[calc(100%_-_57px)] overflow-y-auto p-2" {
                @for file in presentation.files {
                    @let status = file_status_presentation(file.status());
                    button type="button"
                        class="flex min-h-11 w-full cursor-pointer items-center gap-2 rounded-sm border-0 bg-transparent px-2 py-2 text-left text-ink-2 hover:bg-surface-2 hover:text-ink active:bg-acc-soft focus-visible:-outline-offset-2 focus-visible:outline-2 focus-visible:outline-acc"
                        data-file-target=(diff_file_anchor_id(&file.path)) {
                        span class={ "inline-flex size-4 flex-none items-center justify-center rounded-sm border text-xs font-bold " (status.badge_classes) }
                            title=(status.label) { (status.code) }
                        span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap" { (file.path) }
                        span class="flex-none text-xs" {
                            span class="text-add" { "+" (file.added) } " "
                            span class="text-del" { "−" (file.removed) }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::RenderOptions;

    use crate::{fixtures::sample_view, test_render::build_html};

    #[test]
    fn tree_renders_server_owned_file_nodes() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None).replace("&amp;", "&");
        let tree_classes = html
            .split_once(r#"<aside class="tree "#)
            .and_then(|(_, tail)| tail.split_once('"'))
            .map(|(classes, _)| classes)
            .expect("tree class attribute");

        for hook in [
            "[&_.tnode]",
            "[&_.tdir",
            "[&_.tfile",
            "[&_.tlabel]",
            "[&_.tcaret]",
            "[&_.tname]",
            "[&_.tstatus]",
        ] {
            assert!(tree_classes.contains(hook), "tree root must style `{hook}`");
        }
        assert!(html.contains(r#"class="search "#));
        assert!(html.contains(r#"class="tree-body "#));
        assert!(html.contains(r#"data-target="f-src-a-b-rs""#));
    }

    #[test]
    fn mobile_changed_files_popover_renders_total_line_chips() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, None);

        let mobile_popover = html
            .split_once(r#"id="artifact-files-popover-0""#)
            .and_then(|(_, tail)| tail.split_once(r#"<div class="gtl-scroll"#))
            .map(|(header, _)| header)
            .expect("mobile changed-files popover header");

        assert!(mobile_popover.contains("title=\"Lines added\""));
        assert!(mobile_popover.contains("title=\"Lines removed\""));
        assert!(mobile_popover.contains("border-add-line text-add"));
        assert!(mobile_popover.contains("border-del-line text-del"));
        assert!(mobile_popover.contains(">+2</span>"));
        assert!(mobile_popover.contains(">−1</span>"));
    }
}
