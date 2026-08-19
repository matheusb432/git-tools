use gtl_parser::{
    CharacterCount, DiffParser, DiffRowKind, ParseOptions, SourceLineNumber, SplitDiffRow,
    SplitDiffStream,
};

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
    assert_eq!(
        parsed.rows()[3].old_line_number(),
        Some(SourceLineNumber::new(4))
    );
    assert_eq!(parsed.rows()[3].new_line_number(), None);
    assert_eq!(
        parsed.rows()[4].new_line_number(),
        Some(SourceLineNumber::new(8))
    );
    assert!(matches!(
        parsed.split_rows()[3],
        SplitDiffRow::Pair {
            old: Some(_),
            new: Some(_)
        }
    ));
}

#[test]
fn downstream_consumer_can_read_parser_coordinate_values() {
    let parsed = DiffParser::new().parse(&lines(&["@@ -42 +42 @@", " keep"]));

    assert_eq!(
        parsed.rows()[1]
            .old_line_number()
            .map(SourceLineNumber::into_inner),
        Some(42)
    );
    assert_eq!(parsed.line_number_digits().get(), 2);
}

#[test]
fn downstream_consumer_receives_typed_offsets_counts_and_digit_widths() {
    let parsed = DiffParser::with_options(ParseOptions::new(CharacterCount::new(3)))
        .parse(&lines(&["@@ -9999 +10000 @@", "-abcd", "+abce"]));
    let SplitDiffRow::Pair {
        old: Some(old),
        new: Some(new),
    } = &parsed.split_rows()[1]
    else {
        panic!("changed rows should pair");
    };

    assert_eq!(old.line_number(), SourceLineNumber::new(9_999));
    assert_eq!(new.line_number(), SourceLineNumber::new(10_000));
    assert_eq!(
        old.long_line_character_count(),
        Some(CharacterCount::new(4))
    );
    assert_eq!(parsed.line_number_digits().get(), 5);

    let parsed = DiffParser::new().parse(&lines(&["@@ -1 +1 @@", "-abcd", "+abce"]));
    let SplitDiffRow::Pair { old: Some(old), .. } = &parsed.split_rows()[1] else {
        panic!("short changed rows should pair");
    };
    assert_eq!(old.intraline_spans()[0].start().into_inner(), 3);
    assert_eq!(old.intraline_spans()[0].end().into_inner(), 4);
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

#[cfg(feature = "syntax")]
#[test]
fn downstream_consumer_can_select_a_language_and_attach_semantic_tokens() {
    use gtl_parser::{SemanticTextChange, SyntaxLanguage, SyntaxTokenClass};

    let parsed = DiffParser::new()
        .with_syntax(Some(SyntaxLanguage::Rust))
        .parse(&lines(&["@@ -0,0 +1 @@", "+let value = 1;"]));

    assert!(parsed.rows()[1].semantic_spans().iter().any(|span| {
        span.text(parsed.rows()[1].body()) == "1"
            && span.syntax_class() == Some(SyntaxTokenClass::Constant)
            && span.change() == SemanticTextChange::Unchanged
    }));
    assert!(parsed.syntax_diagnostics().is_empty());
}

#[cfg(feature = "syntax")]
#[test]
fn downstream_consumer_can_resolve_every_supported_extension() {
    use gtl_parser::SyntaxLanguage;

    for (path, expected) in [
        ("source.JS", SyntaxLanguage::JavaScript),
        ("source.TS", SyntaxLanguage::TypeScript),
        ("source.PY", SyntaxLanguage::Python),
        ("source.RS", SyntaxLanguage::Rust),
        ("source.MD", SyntaxLanguage::Markdown),
        ("source.HTML", SyntaxLanguage::Html),
        ("source.YML", SyntaxLanguage::Yaml),
        ("source.YAML", SyntaxLanguage::Yaml),
    ] {
        assert_eq!(SyntaxLanguage::from_path(path), Some(expected));
    }
    assert_eq!(SyntaxLanguage::from_path("source.unknown"), None);
}
