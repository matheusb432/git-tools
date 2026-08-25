use std::fmt;

#[cfg(feature = "desktop")]
use dioxus::prelude::ClipboardEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SelectedLineRange {
    first: u32,
    last: u32,
}

impl fmt::Display for SelectedLineRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.first == self.last {
            write!(formatter, "{}", self.first)
        } else {
            write!(formatter, "{}..{}", self.first, self.last)
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct SelectedDiffLines {
    lines: Vec<String>,
    line_range: Option<SelectedLineRange>,
}

impl SelectedDiffLines {
    fn push(&mut self, line: String, line_number: Option<u32>) {
        self.lines.push(line);
        if let Some(line_number) = line_number {
            self.line_range = Some(match self.line_range {
                Some(range) => SelectedLineRange {
                    first: range.first,
                    last: line_number,
                },
                None => SelectedLineRange {
                    first: line_number,
                    last: line_number,
                },
            });
        }
    }

    fn with_context(self, path: &str, comment_leader: &str) -> Option<ContextualizedCopy> {
        if self.lines.is_empty() {
            return None;
        }

        let line_range = self
            .line_range
            .map(|range| format!(", lines: {range}"))
            .unwrap_or_default();
        let status = self.line_range.map_or_else(
            || "Copied with context".to_owned(),
            |range| format!("Copied with context - lines {range}"),
        );
        Some(ContextualizedCopy {
            text: format!(
                "{comment_leader} * {path}{line_range}\n{}",
                self.lines.join("\n")
            ),
            status,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ContextualizedCopy {
    text: String,
    status: String,
}

#[cfg(feature = "desktop")]
pub(super) fn copy_selected_diff_lines(event: &ClipboardEvent) -> Option<String> {
    use dioxus::web::WebEventExt as _;
    use wasm_bindgen::JsCast as _;

    let clipboard_event = event
        .data()
        .try_as_web_event()?
        .dyn_into::<web_sys::ClipboardEvent>()
        .ok()?;
    let selection = web_sys::window()?.get_selection().ok().flatten()?;
    if selection.is_collapsed() || selection.range_count() == 0 {
        return None;
    }

    let anchor_node = selection.anchor_node()?;
    let focus_node = selection.focus_node()?;
    let file = closest_diff_file(&anchor_node)?;
    let focus_file = closest_diff_file(&focus_node)?;
    if !file.is_same_node(Some(&focus_file)) {
        return None;
    }

    let rows = file.query_selector_all("[data-gtl-copy-line]").ok()?;
    let mut selected = SelectedDiffLines::default();
    for index in 0..rows.length() {
        let Some(row) = rows
            .item(index)
            .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
        else {
            continue;
        };
        if !selection
            .contains_node_with_allow_partial_containment(&row, true)
            .unwrap_or(false)
        {
            continue;
        }
        let Some(text) = row
            .query_selector("[data-gtl-copy-text]")
            .ok()
            .flatten()
            .and_then(|element| element.text_content())
        else {
            continue;
        };
        let line_number = row
            .get_attribute("data-gtl-new-line")
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|number| *number > 0);
        selected.push(text, line_number);
    }

    let path = file.get_attribute("data-path")?;
    let comment_leader = file
        .get_attribute("data-gtl-comment-leader")
        .unwrap_or_else(|| "//".to_owned());
    let copied = selected.with_context(&path, &comment_leader)?;
    clipboard_event
        .clipboard_data()?
        .set_data("text/plain", &copied.text)
        .ok()?;
    clipboard_event.prevent_default();
    Some(copied.status)
}

#[cfg(feature = "desktop")]
fn closest_diff_file(node: &web_sys::Node) -> Option<web_sys::Element> {
    use wasm_bindgen::JsCast as _;

    let element = node
        .dyn_ref::<web_sys::Element>()
        .cloned()
        .or_else(|| node.parent_element())?;
    element.closest("[data-gtl-diff-file]").ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_lines_copy_as_clean_source_under_their_file_context() {
        let mut selected = SelectedDiffLines::default();
        selected.push("let first = 1;".to_owned(), Some(12));
        selected.push("let second = 2;".to_owned(), Some(13));

        assert_eq!(
            selected.with_context("src/main.rs", "//"),
            Some(ContextualizedCopy {
                text: "// * src/main.rs, lines: 12..13\nlet first = 1;\nlet second = 2;".to_owned(),
                status: "Copied with context - lines 12..13".to_owned(),
            })
        );
    }

    #[test]
    fn a_single_selected_line_uses_one_line_number() {
        let mut selected = SelectedDiffLines::default();
        selected.push("echo ready".to_owned(), Some(7));

        assert_eq!(
            selected.with_context("scripts/run.sh", "#"),
            Some(ContextualizedCopy {
                text: "# * scripts/run.sh, lines: 7\necho ready".to_owned(),
                status: "Copied with context - lines 7".to_owned(),
            })
        );
    }

    #[test]
    fn a_selection_without_source_rows_falls_through() {
        assert_eq!(
            SelectedDiffLines::default().with_context("src/main.rs", "//"),
            None
        );
    }
}
