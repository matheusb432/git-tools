use std::collections::VecDeque;

use gtl_application::{diffs::View, viewer::RenderOptions};
use maud::{Markup, html};

use super::{diff_target_id, render_rows, selected_lines};
use crate::{ViewChunk, syntax::PreviewResult};

const MAX_CHUNK_ROWS: usize = 256;
const MAX_CHUNK_BYTES: usize = 256 * 1024;

// TODO: [gtl-web]: replace this HTMX load trigger with Dioxus-owned chunk requests.
pub(in crate::layout) fn chunk_loader(load_id: u64) -> Markup {
    html! {
        div id="viewer-chunk-loader"
            hx-get=(format!("/loads/{load_id}/next"))
            hx-trigger="load"
            hx-target="this"
            hx-swap="outerHTML" {}
    }
}

pub(in crate::layout) fn view_chunks(
    view: &View,
    options: RenderOptions,
) -> PreviewResult<VecDeque<ViewChunk>> {
    let mut chunks = VecDeque::new();
    for (file_index, file) in view.files.iter().enumerate() {
        let syntax = crate::syntax::syntax_for_path(&file.path)?;
        let (lines, _) = selected_lines(file, options);
        let rendered = render_rows(options.layout(), lines, syntax);
        chunks.extend(
            split_rows(&rendered)
                .into_iter()
                .map(|(html, rows)| ViewChunk {
                    target_id: diff_target_id(file_index),
                    html,
                    rows,
                }),
        );
    }
    Ok(chunks)
}

fn split_rows(rendered: &str) -> Vec<(String, usize)> {
    let mut chunks = Vec::new();
    let mut chunk = String::new();
    let mut row_count = 0;
    for row in rendered.split_inclusive("</div>") {
        let crosses_bound = row_count > 0
            && (row_count == MAX_CHUNK_ROWS
                || chunk.len().saturating_add(row.len()) > MAX_CHUNK_BYTES);
        if crosses_bound {
            chunks.push((std::mem::take(&mut chunk), row_count));
            row_count = 0;
        }
        chunk.push_str(row);
        row_count += 1;
    }
    if row_count > 0 {
        chunks.push((chunk, row_count));
    }
    chunks
}

#[cfg(test)]
mod tests {
    use gtl_application::viewer::{DiffLayout, RenderOptions};

    use super::*;
    use crate::fixtures::sample_view;

    #[test]
    fn desktop_chunks_recompose_the_complete_server_rendered_rows() {
        let view = sample_view();
        let file = &view.files[0];
        let syntax = crate::test_render::syntax_for_path(&file.path);
        let (lines, _) = selected_lines(file, RenderOptions::DEFAULT);
        let complete = render_rows(DiffLayout::Unified, lines, syntax);

        let chunks =
            view_chunks(&view, RenderOptions::DEFAULT).expect("embedded syntax assets should load");
        let recomposed = chunks
            .iter()
            .map(|chunk| chunk.html.as_str())
            .collect::<String>();

        assert_eq!(recomposed, complete);
        assert!(chunks.iter().all(|chunk| chunk.rows <= MAX_CHUNK_ROWS));
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.html.len() <= MAX_CHUNK_BYTES)
        );
    }

    #[test]
    fn oversized_row_is_isolated_and_chunking_always_advances() {
        let oversized = format!("+{}", "x".repeat(MAX_CHUNK_BYTES + 1));
        let rendered = format!(
            "<div class=\"dl\">small</div><div class=\"dl\">{oversized}</div><div class=\"dl\">tail</div>"
        );

        let chunks = split_rows(&rendered);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks.iter().map(|(_, rows)| rows).sum::<usize>(), 3);
        assert_eq!(
            chunks
                .iter()
                .map(|(html, _)| html.as_str())
                .collect::<String>(),
            rendered
        );
        assert!(chunks[1].0.len() > MAX_CHUNK_BYTES);
        assert_eq!(chunks[1].1, 1);
    }
}
