use super::document::{Document, Layout};

#[derive(Default)]
pub(super) struct Search {
    query: String,
    matches: Vec<usize>,
    cursor: Option<usize>,
}

impl Search {
    pub fn new(query: String, document: &Document, layout: &Layout) -> Self {
        let mut search = Self {
            query,
            ..Self::default()
        };
        search.rebuild(document, layout);
        search
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    pub fn matches(&self, screen_start: usize) -> bool {
        self.matches.binary_search(&screen_start).is_ok()
    }

    pub fn reset_cursor(&mut self) {
        self.cursor = None;
    }

    pub fn rebuild(&mut self, document: &Document, layout: &Layout) {
        self.reset_cursor();
        self.matches.clear();
        if self.query.is_empty() {
            return;
        }
        let needle = self.query.to_lowercase();
        self.matches.extend(
            layout
                .entries
                .iter()
                .filter(|entry| {
                    layout
                        .text(document, entry)
                        .to_lowercase()
                        .contains(&needle)
                })
                .map(|entry| entry.screen_start),
        );
    }

    pub fn find(
        &mut self,
        offset: usize,
        forward: bool,
        include_current: bool,
    ) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            return None;
        }
        let cursor = if include_current {
            offset
        } else {
            self.cursor.unwrap_or(offset)
        };
        let index = if forward {
            let index = self.matches.partition_point(|position| {
                *position < cursor || !include_current && *position == cursor
            });
            if index == self.matches.len() {
                0
            } else {
                index
            }
        } else {
            self.matches
                .partition_point(|position| *position < cursor)
                .checked_sub(1)
                .unwrap_or(self.matches.len() - 1)
        };
        let position = self.matches[index];
        self.cursor = Some(position);
        Some((index + 1, position))
    }
}
