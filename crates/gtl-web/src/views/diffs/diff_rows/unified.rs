use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;

use super::{
    HeaderTone,
    content::{ChangedTextTone, code_cell_content, non_breaking_if_empty},
};
use crate::{
    entities::diffs::{
        ClientDiffFile, ClientDiffFileStoreExt, ClientDiffRowsStoreExt, ViewerUnifiedRow,
        ViewerUnifiedSourceRow,
    },
    shared::i18n::use_language,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnifiedSourceTone {
    Context,
    Added,
    Removed,
}

#[component]
pub(crate) fn UnifiedDiffRowBatch(file: ReadStore<ClientDiffFile>, batch_index: usize) -> Element {
    let rows = file.rows().unified().index(batch_index);
    rsx! {
        UnifiedRows { rows, first_row: batch_index * 64 }
    }
}

#[component]
fn UnifiedRows(
    rows: ReadStore<Vec<ViewerUnifiedRow>>,

    #[props(default)] first_row: usize,
) -> Element {
    let language = use_language();
    let rows = rows.read();
    rsx! {
        {
            rows.iter()
                .enumerate()
                .map(|(index, row)| unified_row(row, first_row + index, language))
        }
    }
}

fn unified_row(row: &ViewerUnifiedRow, row_index: usize, language: ViewerLanguage) -> Element {
    match row {
        ViewerUnifiedRow::Meta(text) => unified_header_row(HeaderTone::Meta, text, row_index),
        ViewerUnifiedRow::Hunk(text) => unified_header_row(HeaderTone::Hunk, text, row_index),
        ViewerUnifiedRow::Context(source) => {
            unified_source_row(UnifiedSourceTone::Context, source, row_index, language)
        }
        ViewerUnifiedRow::Added(source) => {
            unified_source_row(UnifiedSourceTone::Added, source, row_index, language)
        }
        ViewerUnifiedRow::Removed(source) => {
            unified_source_row(UnifiedSourceTone::Removed, source, row_index, language)
        }
    }
}

fn unified_header_row(tone: HeaderTone, text: &str, row_index: usize) -> Element {
    let tone = match tone {
        HeaderTone::Meta => "meta",
        HeaderTone::Hunk => "hunk",
    };
    let text = non_breaking_if_empty(text);
    rsx! {
        div {
            key: "{row_index}",
            class: "diff-row-unified",
            "data-row-index": "{row_index}",
            "data-diff-tone": tone,
            "data-gtl-diff-row": "",
            code { class: "diff-row-header-code diff-row-code col-[1/-1]", "{text}" }
        }
    }
}

fn unified_source_row(
    tone: UnifiedSourceTone,
    source: &ViewerUnifiedSourceRow,
    row_index: usize,
    language: ViewerLanguage,
) -> Element {
    let (row_tone, gutter_tone, gutter_number, copy_line_number) = match tone {
        UnifiedSourceTone::Context => (
            "context",
            "neutral",
            source.new_line_number,
            source.new_line_number,
        ),
        UnifiedSourceTone::Added => (
            "added",
            "added",
            source.new_line_number,
            source.new_line_number,
        ),
        UnifiedSourceTone::Removed => ("removed", "removed", source.old_line_number, None),
    };
    let gutter_number = gutter_number.map(|number| number.to_string());
    let copy_line = copy_line_number.map(|_| "");
    let new_line_number = copy_line_number.map(|number| number.to_string());
    rsx! {
        div {
            key: "{row_index}",
            class: "diff-row-unified",
            "data-row-index": "{row_index}",
            "data-diff-tone": row_tone,
            "data-gtl-diff-row": "",
            "data-gtl-copy-line": copy_line,
            "data-gtl-new-line": new_line_number,
            span {
                class: "diff-row-unified-gutter",
                "data-diff-tone": gutter_tone,
                {gutter_number}
            }
            code { class: "diff-row-unified-code min-w-0 py-0 pr-1 pl-3 text-sm diff-row-code",
                {
                    code_cell_content(
                        &source.code,
                        None,
                        ChangedTextTone::None,
                        copy_line_number.is_some(),
                        language,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unified_source_row;

    #[component]
    fn UnifiedBatchFixture(rows: Vec<ViewerUnifiedRow>) -> Element {
        let rows = use_store(move || rows);
        rsx! {
            UnifiedRows { rows }
        }
    }

    #[test]
    fn renders_one_gutter_per_source_row_with_typed_line_numbers_and_omission_counts() {
        let rows = vec![
            ViewerUnifiedRow::Hunk("@@ -9999 +10000 @@".to_owned()),
            ViewerUnifiedRow::Removed(unified_source_row("abcd", Some(9999), None, Some(4))),
            ViewerUnifiedRow::Added(unified_source_row("abce", None, Some(10000), Some(4))),
        ];
        let html = dioxus_ssr::render_element(rsx! {
            UnifiedBatchFixture { rows }
        });

        assert!(html.contains(">9999</span>"));
        assert!(html.contains(">10000</span>"));
        assert_eq!(html.matches("diff-row-unified-gutter").count(), 2);
        assert!(!html.contains("class=\"hidden"));
        assert_eq!(
            html.matches(r#"class="diff-row-unified" data-row-index="#)
                .count(),
            3
        );
        assert_eq!(html.matches("(+4 characters omitted)").count(), 2);
        assert_eq!(html.matches(r#"data-gtl-copy-line="""#).count(), 1);
        assert!(html.contains(r#"class="diff-truncated-text">abce</span>"#));
    }
}
