use super::*;
use crate::DiffParser;

fn fixture() -> Vec<String> {
    [
        "index a..b 100644",
        "--- a/file.rs",
        "+++ b/file.rs",
        "",
        "@@ -1,6 +1,6 @@",
        " /* open",
        "-old inside",
        "+new inside",
        " */",
        "-let café = 1;",
        "+let café = 2;",
        "-old second",
        "+new second",
        " retained",
        "\\ No newline at end of file",
        "@@ -40,2 +50,2 @@",
        "--- header-like body",
        "+++ header-like body",
        " context",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[test]
fn windows_match_complete_unified_and_split_parsing_at_every_boundary() {
    let mut lines = fixture();
    lines.push(format!("+{}", "x".repeat(2001)));
    let index = DiffIndex::new(&lines);
    let parser = DiffParser::new();
    #[cfg(feature = "syntax")]
    let parser = parser.with_syntax(Some(crate::SyntaxLanguage::Rust));
    let complete = parser.parse(&lines);
    let split = complete.split_rows();
    assert_eq!(index.len(DiffRowLayout::Unified), complete.rows().len());
    assert_eq!(index.len(DiffRowLayout::Split), split.len());
    assert_eq!(index.line_number_digits(), complete.line_number_digits());
    for layout in [DiffRowLayout::Unified, DiffRowLayout::Split] {
        let mut windows = index.window_parser(
            ParseOptions::default(),
            #[cfg(feature = "syntax")]
            Some(crate::SyntaxLanguage::Rust),
        );
        for start in 0..index.len(layout) {
            for count in [1, 3, 64] {
                let range = start..(start + count).min(index.len(layout));
                let window = windows
                    .parse_range(
                        |index| &lines[index],
                        range.clone(),
                        layout,
                        &ParseCancellation::default(),
                    )
                    .unwrap();
                match window.rows {
                    ParsedDiffWindowRows::Unified(rows) => assert_eq!(rows, complete.rows()[range]),
                    ParsedDiffWindowRows::Split(rows) => assert_eq!(rows, split[range]),
                }
            }
        }
    }
}

#[test]
fn an_oversized_hunk_reads_only_the_requested_source_rows() {
    let lines = [
        "@@ -1,2 +1,2 @@",
        "-old content",
        "-old second",
        "+new content",
        "+new second",
    ];
    let index = DiffIndex::new(lines);
    let reads = std::cell::RefCell::new(Vec::new());
    let window = index
        .parse_range(
            |index| {
                reads.borrow_mut().push(index);
                lines[index]
            },
            2..3,
            DiffRowLayout::Split,
            ParseOptions::default().with_max_syntax_hunk_bytes(crate::SyntaxHunkByteLimit::new(8)),
            #[cfg(feature = "syntax")]
            Some(crate::SyntaxLanguage::Rust),
            &ParseCancellation::default(),
        )
        .unwrap();
    assert!(matches!(window.rows, ParsedDiffWindowRows::Split(rows) if rows.len() == 1));
    assert_eq!(*reads.borrow(), [2, 4]);
}

#[test]
fn cancellation_discards_partial_rows_and_stops_source_reads() {
    let lines = fixture();
    let index = DiffIndex::new(&lines);
    let cancellation = ParseCancellation::default();
    let reads = std::cell::Cell::new(0);
    let result = index.parse_range(
        |index| {
            reads.set(reads.get() + 1);
            cancellation.cancel();
            &lines[index]
        },
        0..index.len(DiffRowLayout::Unified),
        DiffRowLayout::Unified,
        ParseOptions::default(),
        #[cfg(feature = "syntax")]
        Some(crate::SyntaxLanguage::Rust),
        &cancellation,
    );
    assert_eq!(result, Err(DiffWindowError::Cancelled));
    assert_eq!(reads.get(), 1);
}

#[cfg(feature = "syntax")]
#[test]
fn adjacent_windows_reuse_context_and_crossing_hunks_releases_it() {
    let lines = fixture();
    let index = DiffIndex::new(&lines);
    let mut parser =
        index.window_parser(ParseOptions::default(), Some(crate::SyntaxLanguage::Rust));
    let reads = std::cell::Cell::new(0);
    let source = |index: usize| {
        reads.set(reads.get() + 1);
        lines[index].as_str()
    };
    let cancellation = ParseCancellation::default();
    parser
        .parse_range(source, 5..7, DiffRowLayout::Unified, &cancellation)
        .unwrap();
    reads.set(0);
    parser
        .parse_range(source, 8..10, DiffRowLayout::Unified, &cancellation)
        .unwrap();
    assert_eq!(
        reads.get(),
        2,
        "a neighboring window must read only its requested rows"
    );
    parser
        .parse_range(source, 15..17, DiffRowLayout::Unified, &cancellation)
        .unwrap();
    reads.set(0);
    parser
        .parse_range(source, 5..7, DiffRowLayout::Unified, &cancellation)
        .unwrap();
    assert!(
        reads.get() > 2,
        "only the most recent hunk may remain cached"
    );
}
