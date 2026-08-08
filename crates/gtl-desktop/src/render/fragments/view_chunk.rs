use maud::{Markup, PreEscaped, html};

// TODO: [gtl-web]: replace this HTMX continuation response with Dioxus-owned chunk requests.
pub(crate) fn view_chunk(chunk: &gtl_preview::ViewChunk, next_load_id: Option<u64>) -> Markup {
    let target = format!("beforeend:#{}", chunk.target_id);
    html! {
        div hx-swap-oob=(target) data-chunk-rows=(chunk.rows) {
            (PreEscaped(&chunk.html))
        }
        @if let Some(load_id) = next_load_id {
            div id="viewer-chunk-loader"
                hx-get=(format!("/loads/{load_id}/next"))
                hx-trigger="load"
                hx-target="this"
                hx-swap="outerHTML" {}
        } @else {
            div id="viewer-chunk-loader" data-complete hidden {}
        }
    }
}
