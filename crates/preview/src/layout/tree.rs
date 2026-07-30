//! Left sidebar: the server-rendered changed-files tree, filter input, and range stats.

use application::diffs::View;
use maud::{Markup, html};

use super::files::file_status_presentation;
use crate::text::{plural, slug};

const TREE_PRESENTATION_CLASSES: &str = concat!(
    "tree gtl-scroll [grid-area:2/1] overflow-auto border-r border-line bg-surface p-3 ",
    "[@media(max-width:1280px)]:p-2.5 [@media(max-width:1024px)]:hidden print:hidden! ",
    "[&_.tree-body_ul]:m-0 [&_.tree-body_ul]:list-none [&_.tree-body_ul]:pl-2.5 [&_.tree-body>ul]:pl-0 ",
    "[&_.tnode]:min-w-0 ",
    "[&_.tlabel]:flex [&_.tlabel]:cursor-pointer [&_.tlabel]:items-center [&_.tlabel]:gap-1.5 [&_.tlabel]:rounded-sm [&_.tlabel]:px-1.5 [&_.tlabel]:py-0.5 [&_.tlabel]:leading-[1.35] [&_.tlabel]:text-ink-2 ",
    "[&_.tdir>ul_.tfile>.tlabel]:pl-2 [&_.tlabel:hover]:bg-surface-2 [&_.tlabel:hover]:text-ink ",
    "[&_:is(.tdir>.tlabel,.tdir>.tlabel_.tname)]:text-ink-3 ",
    "[&_.tfile.cur>.tlabel]:bg-acc-soft [&_.tfile.cur>.tlabel]:text-ink [&_.tfile.cur>.tlabel]:shadow-[inset_2px_0_0_var(--acc)] ",
    "[&_.tcaret]:size-0 [&_.tcaret]:flex-none [&_.tcaret]:border-y-4 [&_.tcaret]:border-y-transparent [&_.tcaret]:border-l-5 [&_.tcaret]:border-l-ink-3 ",
    "[&_.tdir.open>.tlabel_.tcaret]:rotate-90 [&_.tdir:not(.open)>ul]:hidden ",
    "[&_.tname]:min-w-0 [&_.tname]:flex-1 [&_.tname]:overflow-hidden [&_.tname]:text-ellipsis ",
    "[&_.tfile.status-added>.tlabel]:bg-[color-mix(in_srgb,var(--add-bg)_42%,transparent)] ",
    "[&_.tfile.status-deleted>.tlabel]:bg-[color-mix(in_srgb,var(--del-bg)_42%,transparent)] ",
    "[&_.tfile.status-added>.tlabel:hover]:bg-[color-mix(in_srgb,var(--add-bg)_62%,var(--surface-2))] ",
    "[&_.tfile.status-deleted>.tlabel:hover]:bg-[color-mix(in_srgb,var(--del-bg)_62%,var(--surface-2))] ",
    "[&_.tstatus]:inline-flex [&_.tstatus]:size-[15px] [&_.tstatus]:flex-none [&_.tstatus]:items-center [&_.tstatus]:justify-center [&_.tstatus]:rounded-sm [&_.tstatus]:border [&_.tstatus]:border-line-2 [&_.tstatus]:text-[9.5px] [&_.tstatus]:leading-none [&_.tstatus]:font-bold ",
    "[&_.tstatus.status-added]:border-add-line [&_.tstatus.status-added]:bg-add-bg [&_.tstatus.status-added]:text-add ",
    "[&_.tstatus.status-deleted]:border-del-line [&_.tstatus.status-deleted]:bg-del-bg [&_.tstatus.status-deleted]:text-del ",
    "[&_.tstatus.status-renamed]:border-acc-line [&_.tstatus.status-renamed]:bg-acc-soft [&_.tstatus.status-renamed]:text-acc ",
    "[&_.tstatus.status-modified]:bg-sunk [&_.tstatus.status-modified]:text-ink-3",
);

// ! `.search` and `.tree-body` are enhancer hooks. The tree owns presentation for its
// ! server-rendered descendants.
pub(super) fn tree(view: &View) -> Markup {
    let total_add: u32 = view.files.iter().map(|f| f.added).sum();
    let total_del: u32 = view.files.iter().map(|f| f.removed).sum();
    let commit_count = view.commits.len();
    let file_count = view.files.len();
    // ! One border-color and one text-color utility per chip: stacking a neutral and an
    // ! accent utility of the same property leaves the winner to stylesheet order.
    let stat = "rounded-sm border px-2 py-0.5 text-[11px]";

    html! {
        aside class=(TREE_PRESENTATION_CLASSES) aria-label="Changed files tree" {
            div class="search relative mb-3 print:hidden!" {
                input type="text"
                    class="filter [font:inherit] w-full rounded-sm border border-line-2 bg-sunk px-2.5 py-2 text-[13px] text-ink focus:border-acc-line focus:shadow-[0_0_0_2px_var(--acc-soft)] focus:outline-none"
                    placeholder="Filter files…  /" aria-label="Filter files";
            }
            div class="mx-1 mt-1.5 mb-2 flex justify-between text-[11px] tracking-[0.06em] text-ink-3 uppercase" {
                span { (view.commits_label) " · " (file_count) " file" (plural(file_count)) }
            }
            div class="mx-0.5 mb-3 flex flex-wrap gap-2" {
                span class={ (stat) " border-line-2 text-ink-2" } { b class="font-bold text-ink" { (commit_count) } " commit" (plural(commit_count)) }
                span class={ (stat) " border-add-line text-add" } { "+" (total_add) }
                span class={ (stat) " border-del-line text-del" } { "−" (total_del) }
            }
            div class="tree-body text-[12.5px] whitespace-nowrap" {
                (render_file_tree(view))
            }
        }
    }
}

#[derive(Default)]
struct TreeDirectory<'a> {
    directories: Vec<(&'a str, TreeDirectory<'a>)>,
    files: Vec<(&'a str, &'a application::diffs::FileDiff)>,
}

fn render_file_tree(view: &View) -> Markup {
    let mut root = TreeDirectory::default();
    for file in &view.files {
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
                    directory
                        .directories
                        .push((segment, TreeDirectory::default()));
                    directory.directories.len() - 1
                });
            directory = &mut directory.directories[index].1;
        }
    }
    render_directory(&root)
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
                li class={ "tnode tfile status-" (status.key) }
                    data-target=(slug(&file.path))
                    data-path=(file.path.to_lowercase()) {
                    div class="tlabel" {
                        span class={ "tstatus status-" (status.key) } title=(status.label) {
                            (status.code)
                        }
                        span class="tname" { (name) }
                    }
                }
            }
        }
    }
}

pub(super) fn mobile_popover(view: &View) -> Markup {
    let total_add: u32 = view.files.iter().map(|file| file.added).sum();
    let total_del: u32 = view.files.iter().map(|file| file.removed).sum();

    html! {
        aside id="viewer-files-popover"
            class="fixed inset-3 m-0 h-[calc(100vh_-_24px)] w-[calc(100vw_-_24px)] max-w-none overflow-hidden rounded-panel border border-line-2 bg-surface p-0 text-ink shadow-[0_24px_80px_rgba(0,0,0,.72)] [&::backdrop]:bg-[rgba(0,0,0,.42)]"
            aria-label="Changed files"
            popover {
            header class="flex items-center justify-between border-b border-line bg-surface-2 px-4 py-3" {
                div {
                    strong class="block text-[13px]" { "Changed files" }
                    span class="text-[11px] text-ink-3" {
                        (view.files.len()) " file" (plural(view.files.len())) " · "
                        span class="text-add" { "+" (total_add) } " "
                        span class="text-del" { "−" (total_del) }
                    }
                }
                button type="button" class="size-[30px] cursor-pointer rounded-sm border-0 bg-transparent text-xl text-ink-2 [font:inherit] hover:bg-line hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc"
                    popovertarget="viewer-files-popover" popovertargetaction="hide" aria-label="Close changed files" { "×" }
            }
            div class="gtl-scroll h-[calc(100%_-_57px)] overflow-y-auto p-2" {
                @for file in &view.files {
                    @let status = file_status_presentation(file.status());
                    button type="button"
                        class="flex min-h-11 w-full cursor-pointer items-center gap-2 rounded-sm border-0 bg-transparent px-2 py-2 text-left text-[12.5px] text-ink-2 [font:inherit] hover:bg-surface-2 hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-acc"
                        data-file-target=(slug(&file.path)) {
                        span class={ "inline-flex size-[17px] flex-none items-center justify-center rounded-sm border text-[9.5px] font-bold " (status.badge_classes) }
                            title=(status.label) { (status.code) }
                        span class="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap" { (file.path) }
                        span class="flex-none text-[11px]" {
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
    use application::viewer::{RenderOptions, ViewerTabId};

    use crate::{fixtures::sample_view, view_fragment};

    #[test]
    fn tree_renders_server_owned_file_nodes() {
        let tab_id = ViewerTabId::try_new(1).expect("positive tab id");
        let html = view_fragment(&sample_view(), RenderOptions::DEFAULT, tab_id)
            .into_string()
            .replace("&amp;", "&");
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
}
