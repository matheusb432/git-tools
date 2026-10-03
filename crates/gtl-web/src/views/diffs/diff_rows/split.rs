use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use gtl_wire::viewer::ViewerSplitCell;

use super::{
    HeaderTone,
    content::{ChangedTextTone, code_cell_content, non_breaking_if_empty},
};
use crate::{
    entities::diffs::{
        ClientDiffFile, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ViewerCodeLine,
        ViewerSplitRow,
    },
    shared::i18n::use_language,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitSide {
    Old,
    New,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SplitCellPresentation {
    OldContext,
    NewContext,
    Removed,
    Added,
}

#[component]
pub(crate) fn SplitDiffRowBatch(file: ReadStore<ClientDiffFile>, batch_index: usize) -> Element {
    let rows = file.rows().split().index(batch_index);
    rsx! {
        SplitRows { rows, first_row: batch_index * 64 }
    }
}

#[component]
fn SplitRows(rows: ReadStore<Vec<ViewerSplitRow>>, #[props(default)] first_row: usize) -> Element {
    let language = use_language();
    let rows = rows.read();
    rsx! {
        for (index, row) in rows.iter().enumerate() {
            div {
                key: "{index}",
                "data-row-index": (first_row + index).to_string(),
                {split_row(row, language)}
            }
        }
    }
}

fn split_row(row: &ViewerSplitRow, language: ViewerLanguage) -> Element {
    match row {
        ViewerSplitRow::Meta(text) => {
            split_header_row(HeaderTone::Meta, non_breaking_if_empty(text))
        }
        ViewerSplitRow::Hunk(text) => split_header_row(HeaderTone::Hunk, text),
        ViewerSplitRow::Context {
            old_line_number,
            new_line_number,
            code,
        } => rsx! {
            div { class: "diff-row-split", "data-gtl-diff-row": "",
                {split_gutter(SplitSide::Old, Some(*old_line_number))}
                {split_code_cell(SplitCellPresentation::OldContext, code, None, language)}
                {split_gutter(SplitSide::New, Some(*new_line_number))}
                {
                    split_code_cell(
                        SplitCellPresentation::NewContext,
                        code,
                        Some(*new_line_number),
                        language,
                    )
                }
            }
        },
        ViewerSplitRow::Pair { old, new } => rsx! {
            div { class: "diff-row-split", "data-gtl-diff-row": "",
                {split_cell(old.as_ref(), SplitSide::Old, language)}
                {split_cell(new.as_ref(), SplitSide::New, language)}
            }
        },
    }
}

fn split_header_row(tone: HeaderTone, text: &str) -> Element {
    let tone = match tone {
        HeaderTone::Meta => "meta",
        HeaderTone::Hunk => "hunk",
    };
    rsx! {
        div {
            class: "diff-row-split-header",
            "data-diff-tone": tone,
            "data-gtl-diff-row": "",
            code { class: "diff-row-header-code diff-row-code", "{text}" }
        }
    }
}

fn split_cell(
    cell: Option<&ViewerSplitCell>,
    side: SplitSide,
    language: ViewerLanguage,
) -> Element {
    let Some(cell) = cell else {
        return split_pad(side);
    };
    let (presentation, copy_line_number) = match side {
        SplitSide::Old => (SplitCellPresentation::Removed, None),
        SplitSide::New => (SplitCellPresentation::Added, Some(cell.line_number)),
    };
    rsx! {
        {split_gutter(side, Some(cell.line_number))}
        {split_code_cell(presentation, &cell.code, copy_line_number, language)}
    }
}

fn split_gutter(side: SplitSide, number: Option<u32>) -> Element {
    let line_number = number.map(|value| value.to_string());
    let side = match side {
        SplitSide::Old => "old",
        SplitSide::New => "new",
    };
    rsx! {
        span { class: "diff-row-split-gutter", "data-diff-side": side, {line_number} }
    }
}

fn split_code_cell(
    presentation: SplitCellPresentation,
    code: &ViewerCodeLine,
    copy_line_number: Option<u32>,
    language: ViewerLanguage,
) -> Element {
    let (marker, changed_text_tone, presentation) = match presentation {
        SplitCellPresentation::OldContext => (' ', ChangedTextTone::None, "old-context"),
        SplitCellPresentation::NewContext => (' ', ChangedTextTone::None, "new-context"),
        SplitCellPresentation::Removed => ('-', ChangedTextTone::Removed, "removed"),
        SplitCellPresentation::Added => ('+', ChangedTextTone::Added, "added"),
    };
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());
    rsx! {
        code {
            class: "diff-row-split-code diff-row-code",
            "data-diff-cell": presentation,
            "data-gtl-copy-line": copy_line,
            "data-gtl-new-line": new_line_number,
            {
                code_cell_content(
                    code,
                    Some(marker),
                    changed_text_tone,
                    copy_line_number.is_some(),
                    language,
                )
            }
        }
    }
}

fn split_pad(side: SplitSide) -> Element {
    let cell = match side {
        SplitSide::Old => "pad-old",
        SplitSide::New => "pad-new",
    };
    rsx! {
        {split_gutter(side, None)}
        code {
            class: "diff-row-split-code diff-row-code",
            "data-diff-cell": cell,
        }
    }
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerSplitCell;

    use super::*;
    use crate::test_support::code_line;

    #[component]
    fn SplitBatchFixture(rows: Vec<ViewerSplitRow>) -> Element {
        let rows = use_store(move || rows);
        use_context_provider(|| rows);
        let language = use_signal(|| gtl_models::settings::ViewerLanguage::EnUs);
        use_context_provider(|| language);
        crate::shared::i18n::use_language_provider(language.into());
        rsx! {
            SplitRows { rows, first_row: 64 }
        }
    }

    #[test]
    fn row_replacement_eviction_and_language_changes_update_the_mounted_batch() {
        use dioxus::dioxus_core::NoOpMutations;
        use gtl_models::settings::ViewerLanguage;

        let mut dom = VirtualDom::new_with_props(
            SplitBatchFixture,
            SplitBatchFixtureProps {
                rows: vec![ViewerSplitRow::Meta("previous header".to_owned())],
            },
        );
        dom.rebuild_in_place();
        let mut rows = dom
            .runtime()
            .consume_context::<Store<Vec<ViewerSplitRow>>>(ScopeId::APP)
            .unwrap();
        let mut language = dom
            .runtime()
            .consume_context::<Signal<ViewerLanguage>>(ScopeId::APP)
            .unwrap();
        rows.set(vec![ViewerSplitRow::Context {
            old_line_number: 3,
            new_line_number: 4,
            code: code_line("replacement source", Some(4)),
        }]);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(!html.contains("previous header"));
        assert_eq!(html.matches("replacement source").count(), 2);
        assert_eq!(html.matches("(+4 characters omitted)").count(), 2);
        assert!(html.contains(r#"data-row-index="64""#));
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);

        language.set(ViewerLanguage::PtBr);
        dom.render_immediate(&mut NoOpMutations);
        let html = dioxus_ssr::render(&dom);
        assert!(!html.contains("characters omitted"));
        assert!(html.contains("caracteres omitidos"));

        rows.set(Vec::new());
        dom.render_immediate(&mut NoOpMutations);
        assert!(!dioxus_ssr::render(&dom).contains("replacement source"));
        rows.set(vec![ViewerSplitRow::Hunk("reloaded header".to_owned())]);
        dom.render_immediate(&mut NoOpMutations);
        assert!(dioxus_ssr::render(&dom).contains("reloaded header"));
    }

    #[test]
    fn renders_typed_line_numbers_truncated_lines_and_an_absent_split_cell() {
        let rows = vec![
            ViewerSplitRow::Hunk("@@ -9999,2 +10000 @@".to_owned()),
            ViewerSplitRow::Pair {
                old: Some(ViewerSplitCell {
                    line_number: 9999,
                    code: code_line("abcd", Some(4)),
                }),
                new: Some(ViewerSplitCell {
                    line_number: 10000,
                    code: code_line("abce", Some(4)),
                }),
            },
            ViewerSplitRow::Pair {
                old: Some(ViewerSplitCell {
                    line_number: 10000,
                    code: code_line("tail", Some(4)),
                }),
                new: None,
            },
        ];
        let html = dioxus_ssr::render_element(rsx! {
            SplitBatchFixture { rows }
        });
        let absent_gutter = dioxus_ssr::render_element(split_gutter(SplitSide::New, None));

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("(+4 characters omitted)").count(), 3);
        assert!(html.contains(r#"data-diff-cell="pad-new""#));
        assert!(absent_gutter.ends_with("></span>"));
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);
        assert!(html.contains(r#"class="diff-truncated-text">abce</span>"#));
    }
}
