use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use gtl_benchmarks::require;
use gtl_models::{
    diffs::DiffLineCount,
    paths::{AbsoluteFilePath, RepositoryRelativePath},
};
use gtl_wire::viewer::{
    ViewerCodeLine, ViewerCodeSpan, ViewerDiffFileId, ViewerFileStatus, ViewerFileSummary,
    ViewerRows, ViewerSplitCell, ViewerSplitRow, ViewerSyntaxClass, ViewerUnifiedRow,
    ViewerUnifiedSourceRow,
};
use sha2::{Digest, Sha256};

fn mount_rows(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("web-rows/mount");
    for (layout, count, rows) in [64, 256].into_iter().flat_map(|count| {
        [
            ("unified", count, unified_rows(count)),
            ("split", count, split_rows(count)),
        ]
    }) {
        let summary = summary(count);
        let mut dom = gtl_web::benchmark::row_batches_dom(summary.clone(), rows.clone());
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        assert_eq!(html.matches("data-row-index=").count(), count);
        eprintln!(
            "{layout}/{count} html_bytes={} sha256={:02x?}",
            html.len(),
            Sha256::digest(html.as_bytes()).as_slice()
        );
        drop(dom);
        group.bench_with_input(BenchmarkId::new(layout, count), &rows, |bencher, rows| {
            bencher.iter_batched(
                || gtl_web::benchmark::row_batches_dom(summary.clone(), rows.clone()),
                |mut dom| {
                    dom.rebuild_in_place();
                    black_box(dom)
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn summary(row_count: usize) -> ViewerFileSummary {
    ViewerFileSummary {
        review: None,
        source_id: None,
        id: ViewerDiffFileId::for_index(0),
        path: require(
            RepositoryRelativePath::try_new("src/render.rs".into()),
            "fixture path",
        ),
        absolute_path: Some(require(
            AbsoluteFilePath::try_new(std::env::temp_dir().join("src/render.rs")),
            "absolute fixture path",
        )),
        anchor_id: "f-render".to_owned(),
        added: DiffLineCount::new(64),
        removed: DiffLineCount::new(64),
        status: ViewerFileStatus::Modified,
        can_open_in_editor: true,
        initially_expanded: true,
        row_count,
    }
}

fn code(index: usize) -> ViewerCodeLine {
    let text = format!("let rendered_{index} = render(&source);");
    ViewerCodeLine {
        spans: vec![
            ViewerCodeSpan {
                byte_start: 0,
                byte_end: 3,
                syntax_class: Some(ViewerSyntaxClass::Keyword),
                changed: false,
            },
            ViewerCodeSpan {
                byte_start: 3,
                byte_end: text.len() - 1,
                syntax_class: None,
                changed: index.is_multiple_of(3),
            },
            ViewerCodeSpan {
                byte_start: text.len() - 1,
                byte_end: text.len(),
                syntax_class: Some(ViewerSyntaxClass::Operator),
                changed: false,
            },
        ],
        text,
        omitted_character_count: (index.is_multiple_of(31)).then_some(12),
    }
}

fn unified_rows(count: usize) -> ViewerRows {
    ViewerRows::Unified(
        (0..count)
            .map(|index| {
                let line = require(u32::try_from(index + 1), "fixture line number");
                let source = ViewerUnifiedSourceRow {
                    old_line_number: Some(line),
                    new_line_number: Some(line),
                    code: code(index),
                };
                match index % 8 {
                    0 => ViewerUnifiedRow::Hunk("@@ -1,64 +1,64 @@".to_owned()),
                    1 => ViewerUnifiedRow::Meta("source metadata".to_owned()),
                    2 => ViewerUnifiedRow::Added(source),
                    3 => ViewerUnifiedRow::Removed(source),
                    _ => ViewerUnifiedRow::Context(source),
                }
            })
            .collect(),
    )
}

fn split_rows(count: usize) -> ViewerRows {
    ViewerRows::Split(
        (0..count)
            .map(|index| {
                let line = require(u32::try_from(index + 1), "fixture line number");
                match index % 8 {
                    0 => ViewerSplitRow::Hunk("@@ -1,64 +1,64 @@".to_owned()),
                    1 => ViewerSplitRow::Meta("source metadata".to_owned()),
                    2..=4 => ViewerSplitRow::Pair {
                        old: (index % 8 != 2).then(|| ViewerSplitCell {
                            line_number: line,
                            code: code(index),
                        }),
                        new: (index % 8 != 3).then(|| ViewerSplitCell {
                            line_number: line,
                            code: code(index + 1),
                        }),
                    },
                    _ => ViewerSplitRow::Context {
                        old_line_number: line,
                        new_line_number: line,
                        code: code(index),
                    },
                }
            })
            .collect(),
    )
}

criterion_group!(benches, mount_rows);
criterion_main!(benches);
