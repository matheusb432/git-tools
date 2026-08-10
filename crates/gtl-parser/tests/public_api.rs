use gtl_parser::{DiffParser, DiffRowKind, SplitDiffRow, SplitDiffStream};

fn lines(raw: &[&str]) -> Vec<String> {
    raw.iter().map(ToString::to_string).collect()
}

#[test]
fn downstream_consumer_can_parse_unified_and_split_rows() {
    let parsed = DiffParser::new().parse(&lines(&[
        "--- a/file.rs",
        "+++ b/file.rs",
        "@@ -4 +8 @@",
        "-let value = 1;",
        "+let value = 2;",
    ]));

    assert_eq!(parsed.rows()[3].kind(), DiffRowKind::Removed);
    assert_eq!(parsed.rows()[3].old_line_number(), Some(4));
    assert_eq!(parsed.rows()[4].new_line_number(), Some(8));
    assert!(matches!(
        parsed.split_rows()[3],
        SplitDiffRow::Pair {
            old: Some(_),
            new: Some(_)
        }
    ));
}

#[test]
fn downstream_consumer_can_stream_parser_and_split_batches() {
    let parser = DiffParser::new();
    let mut parser_stream = parser.stream();
    let mut split_stream = SplitDiffStream::new();
    let header = parser_stream.push(&lines(&["@@ -1 +1 @@", "-old"]));
    let change = parser_stream.push(&lines(&["+new"]));
    let mut rows = split_stream.push(header.into_rows());

    rows.extend(split_stream.push(change.into_rows()));
    rows.extend(split_stream.finish());

    assert!(matches!(rows[0], SplitDiffRow::Hunk { .. }));
    assert!(matches!(
        rows[1],
        SplitDiffRow::Pair {
            old: Some(_),
            new: Some(_)
        }
    ));
}

#[cfg(feature = "bundled-syntaxes")]
#[test]
fn downstream_consumer_can_attach_bundled_semantic_tokens() {
    use gtl_parser::{SyntaxTokenClass, bundled_syntax_catalog};

    let syntax = bundled_syntax_catalog()
        .expect("bundled syntax catalog should load")
        .syntax_for_path("file.rs");
    let parsed = DiffParser::new()
        .with_syntax(syntax)
        .parse(&lines(&["@@ -0,0 +1 @@", "+let value = 1;"]));

    assert!(
        parsed.rows()[1]
            .syntax_tokens()
            .iter()
            .any(|token| token.class() == SyntaxTokenClass::Number)
    );
    assert!(parsed.syntax_diagnostics().is_empty());
}
