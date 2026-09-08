use std::{cell::Cell, rc::Rc};

use dioxus::dioxus_core::NoOpMutations;

use super::{tests::test_file, *};
use crate::test_support::{TestResult, viewer_tab_id};

#[derive(Clone)]
struct NavigationRenderCounts(Rc<Vec<Cell<usize>>>);

pub(super) fn record_render(file_index: usize) {
    if let Some(counts) = try_consume_context::<NavigationRenderCounts>() {
        let count = &counts.0[file_index];
        count.set(count.get() + 1);
    }
}

#[component]
fn NavigationFiles(files: Vec<ClientDiffFile>, artifact_tab_id: Option<ViewerTabId>) -> Element {
    let file_count = files.len();
    let files = use_store(move || files);
    use_context_provider(|| {
        NavigationRenderCounts(Rc::new((0..file_count).map(|_| Cell::new(0)).collect()))
    });
    let folded = use_signal(|| None::<bool>);
    let flashing_file = use_signal(|| None::<String>);
    use_context_provider(|| (folded, flashing_file));
    rsx! {
        for (file_index, file) in files.iter().enumerate() {
            DiffFileCard {
                key: "{file_index}",
                file,
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
                folded,
                flashing_file,
                onopen: None,
                onretry: move |()| {},
                retry_allowed: false,
                file_index,
                artifact_tab_id,
            }
        }
    }
}

fn file_opening_tag<'a>(html: &'a str, anchor_id: &str) -> &'a str {
    let id = format!("id=\"{anchor_id}\"");
    html.split("<details ")
        .filter_map(|file| file.split_once('>'))
        .map(|(tag, _)| tag)
        .find(|tag| tag.contains(&id))
        .unwrap()
}

#[test]
fn file_navigation_renders_changed_cards_and_reopens_the_same_target() -> TestResult {
    for artifact_tab_id in [None, Some(viewer_tab_id(7)?)] {
        let files = (0..3)
            .map(|index| {
                let mut file = test_file()?;
                file.summary.id = ViewerDiffFileId::for_index(index);
                file.summary.anchor_id = format!("file-{index}");
                Ok(file)
            })
            .collect::<TestResult<Vec<_>>>()?;
        let anchors = files
            .iter()
            .map(|file| {
                artifact_tab_id.map_or_else(
                    || file.summary.anchor_id.clone(),
                    |tab_id| static_artifact_file_id(tab_id, &file.summary.id),
                )
            })
            .collect::<Vec<_>>();
        let mut dom = VirtualDom::new_with_props(
            NavigationFiles,
            NavigationFilesProps {
                files,
                artifact_tab_id,
            },
        );
        dom.rebuild_in_place();
        dom.render_immediate(&mut NoOpMutations);
        let (mut folded, mut flashing_file) = dom
            .runtime()
            .consume_context::<(Signal<Option<bool>>, Signal<Option<String>>)>(ScopeId::APP)
            .unwrap();
        let counts = dom
            .runtime()
            .consume_context::<NavigationRenderCounts>(ScopeId::APP)
            .unwrap();
        let unselected_renders = counts.0[2].get();
        let initial = dioxus_ssr::render(&dom);

        for (index, anchor) in anchors[..2].iter().enumerate() {
            let selected_renders = counts.0[index].get();
            flashing_file.set(Some(anchor.clone()));
            dom.render_immediate(&mut NoOpMutations);
            dom.render_immediate(&mut NoOpMutations);
            let html = dioxus_ssr::render(&dom);
            let selected = file_opening_tag(&html, anchor);
            assert!(selected.contains(" open="));
            assert!(selected.contains("outline-acc"));
            assert_eq!(html.matches("outline-offset-[-1px]").count(), 1);
            assert_eq!(
                file_opening_tag(&html, &anchors[2]),
                file_opening_tag(&initial, &anchors[2]),
            );
            assert_eq!(html.matches("echo static").count(), 3);
            assert!(counts.0[index].get() > selected_renders);
            assert_eq!(counts.0[2].get(), unselected_renders);
        }

        folded.set(Some(true));
        dom.render_immediate(&mut NoOpMutations);
        dom.render_immediate(&mut NoOpMutations);
        let folded_html = dioxus_ssr::render(&dom);
        assert!(!file_opening_tag(&folded_html, &anchors[1]).contains(" open="));

        flashing_file.set(Some(anchors[1].clone()));
        dom.render_immediate(&mut NoOpMutations);
        dom.render_immediate(&mut NoOpMutations);
        let reopened = dioxus_ssr::render(&dom);
        assert!(file_opening_tag(&reopened, &anchors[1]).contains(" open="));

        let other_renders = (counts.0[0].get(), counts.0[2].get());
        flashing_file.set(None);
        dom.render_immediate(&mut NoOpMutations);
        dom.render_immediate(&mut NoOpMutations);
        let cleared = dioxus_ssr::render(&dom);
        assert!(!cleared.contains("outline-offset-[-1px]"));
        assert!(file_opening_tag(&cleared, &anchors[1]).contains(" open="));
        assert_eq!((counts.0[0].get(), counts.0[2].get()), other_renders);
    }
    Ok(())
}
