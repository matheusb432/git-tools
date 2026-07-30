//! Per-row syntax tokens: one side-stateful walk over a file's structured rows.

use syntect::parsing::SyntaxReference;

use super::model::{Row, RowKind, line_body, long_line_len};
use crate::syntax::{SideHighlighter, Token};

/// Tokens per row, parallel to `rows`. The old side consumes context and
/// deleted lines, the new side context and added lines; context rows render
/// the new side's tokens. Meta and hunk rows stay untokenized. Long lines skip
/// the parser entirely: their parse cost is unbounded and their effect on
/// multiline state is assumed nil.
pub(super) fn row_tokens(rows: &[Row], syntax: Option<&SyntaxReference>) -> Vec<Vec<Token>> {
    let Some(syntax) = syntax else {
        return vec![Vec::new(); rows.len()];
    };
    let mut old_side = SideHighlighter::new(syntax);
    let mut new_side = SideHighlighter::new(syntax);

    rows.iter()
        .map(|row| {
            if long_line_len(&row.text).is_some() {
                return Vec::new();
            }
            let body = line_body(&row.text);
            match row.kind {
                RowKind::Meta | RowKind::Hunk => Vec::new(),
                RowKind::Del => old_side.tokens(body),
                RowKind::Add => new_side.tokens(body),
                RowKind::Context => {
                    let _ = old_side.tokens(body);
                    new_side.tokens(body)
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        super::model::{MAX_LINE_COLS, derive_rows},
        *,
    };
    use crate::syntax::{TokenClass, syntax_for_path};

    fn rows_for(raw: &[&str]) -> Vec<super::super::model::Row> {
        let lines: Vec<String> = raw.iter().map(ToString::to_string).collect();
        derive_rows(&lines)
    }

    #[test]
    fn context_feeds_both_sides_so_changed_lines_inside_constructs_stay_classed() {
        let rows = rows_for(&[
            "@@ -1,4 +1,4 @@",
            " /* open",
            "-old inside",
            "+new inside",
            " */",
        ]);
        let tokens = row_tokens(&rows, syntax_for_path("a.rs"));
        assert!(
            tokens[2].iter().all(|t| t.class == TokenClass::Comment),
            "{:?}",
            tokens[2]
        );
        assert!(
            tokens[3].iter().all(|t| t.class == TokenClass::Comment),
            "{:?}",
            tokens[3]
        );
        assert!(!tokens[2].is_empty() && !tokens[3].is_empty());
    }

    #[test]
    fn no_syntax_meta_hunk_and_long_rows_stay_untokenized() {
        let long = format!("+{}", "a".repeat(MAX_LINE_COLS + 5));
        let rows = rows_for(&[
            "index 111..222 100644",
            "@@ -1,1 +1,2 @@",
            " fn x() {}",
            &long,
        ]);
        assert!(row_tokens(&rows, None).iter().all(Vec::is_empty));
        let tokens = row_tokens(&rows, syntax_for_path("a.rs"));
        assert!(tokens[0].is_empty(), "meta row");
        assert!(tokens[1].is_empty(), "hunk row");
        assert!(!tokens[2].is_empty(), "context code row");
        assert!(tokens[3].is_empty(), "long row");
    }
}
