use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use gtl_parser::{DiffParser, SyntaxLanguage};

const BENCHMARK_NAME: &str = "parser-syntax/rust-45k";
const SOURCE_LINE_COUNT: usize = 45_000;
const SAMPLE_SIZE: usize = 10;
const HUNK_SOURCE_LINE_COUNT: usize = 90;
const REPLACEMENT_INTERVAL: usize = 9;

fn rust_syntax(criterion: &mut Criterion) {
    let lines = rust_diff_fixture();
    let parser = DiffParser::new().with_syntax(Some(SyntaxLanguage::Rust));
    let parsed = parser.parse(&lines);
    let syntax_token_count = parsed
        .rows()
        .iter()
        .map(|row| row.syntax_tokens().len())
        .sum::<usize>();
    eprintln!(
        "{} source_lines_per_side={SOURCE_LINE_COUNT} diff_rows={} output_rows={} syntax_tokens={syntax_token_count}",
        BENCHMARK_NAME,
        lines.len(),
        parsed.rows().len(),
    );

    criterion.bench_function(BENCHMARK_NAME, |bencher| {
        bencher.iter(|| black_box(&parser).parse(black_box(&lines)));
    });
}

fn rust_diff_fixture() -> Vec<String> {
    assert_eq!(SOURCE_LINE_COUNT % HUNK_SOURCE_LINE_COUNT, 0);
    let replacement_count = SOURCE_LINE_COUNT / REPLACEMENT_INTERVAL;
    let hunk_count = SOURCE_LINE_COUNT / HUNK_SOURCE_LINE_COUNT;
    let mut lines = Vec::with_capacity(SOURCE_LINE_COUNT + replacement_count + hunk_count);

    for hunk_start in (0..SOURCE_LINE_COUNT).step_by(HUNK_SOURCE_LINE_COUNT) {
        let source_start = hunk_start + 1;
        lines.push(format!(
            "@@ -{source_start},{HUNK_SOURCE_LINE_COUNT} +{source_start},{HUNK_SOURCE_LINE_COUNT} @@"
        ));

        for source_index in hunk_start..hunk_start + HUNK_SOURCE_LINE_COUNT {
            push_rust_diff_line(&mut lines, source_index);
        }
    }

    lines
}

fn push_rust_diff_line(lines: &mut Vec<String>, source_index: usize) {
    if source_index.is_multiple_of(REPLACEMENT_INTERVAL) {
        lines.push(format!("-{}", rust_source_line(source_index, false)));
        lines.push(format!("+{}", rust_source_line(source_index, true)));
    } else {
        lines.push(format!(" {}", rust_source_line(source_index, false)));
    }
}

fn rust_source_line(source_index: usize, replacement: bool) -> String {
    let value = source_index + usize::from(replacement);
    match source_index % 8 {
        0 => format!("pub async fn task_{source_index:05}() -> usize {{ {value} }}"),
        1 => format!("const RAW_{source_index:05}: &str = r##\"async {value} # raw\"##;"),
        2 => format!(
            "fn raw_identifier_{source_index:05}() {{ let r#async = {value}; consume(r#async); }}"
        ),
        3 => format!("macro_rules! bench_{source_index:05} {{ () => {{ async {{ {value} }} }}; }}"),
        4 => format!("type Result_{source_index:05} = Result<usize, &'static str>;"),
        5 => format!("#[derive(Clone, Debug)] struct Record_{source_index:05}(usize);"),
        6 => format!("const VALUE_{source_index:05}: usize = calculate!({value});"),
        _ => format!("// deterministic syntax benchmark row {source_index:05}"),
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(SAMPLE_SIZE);
    targets = rust_syntax
}
criterion_main!(benches);
