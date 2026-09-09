use gtl_wire::viewer::{
    StreamViewerRows, ViewerDiffFileId, ViewerDiffLayout, ViewerFileSummary, ViewerRowEvent,
    ViewerRowRange, ViewerRows, ViewerViewIdentity,
};

use super::{ClientDiffFileError, ClientDiffWindow, LoadedRowWindow};
use crate::{
    entities::diffs::{client_diff::CLIENT_LINE_BATCH_SIZE, client_diff_cache, viewer_server},
    shared::viewer_client::ViewerClientError,
};

pub(super) struct WindowRequest {
    identity: ViewerViewIdentity,
    file: ViewerDiffFileId,
    total: u32,
    range: ViewerRowRange,
}

impl WindowRequest {
    pub(super) fn new(
        identity: ViewerViewIdentity,
        window: ClientDiffWindow,
        file: &ViewerFileSummary,
    ) -> Option<Self> {
        let start = window.batch.checked_mul(CLIENT_LINE_BATCH_SIZE)?;
        let count = file
            .row_count
            .checked_sub(start)?
            .min(CLIENT_LINE_BATCH_SIZE);
        Some(Self {
            identity,
            file: file.id.clone(),
            total: u32::try_from(file.row_count).ok()?,
            range: ViewerRowRange::try_new(u32::try_from(start).ok()?, u32::try_from(count).ok()?)
                .ok()?,
        })
    }
}

pub(super) async fn read(request: WindowRequest) -> Result<LoadedRowWindow, ClientDiffFileError> {
    let mut stream = viewer_server::stream_rows(StreamViewerRows {
        identity: request.identity,
        file: Some(request.file.clone()),
        row_range: Some(request.range),
    })
    .await
    .map_err(ClientDiffFileError::Transport)?;
    let mut sequence = 0;
    let mut rows = RowWindow::new(request);
    loop {
        let response = stream
            .message()
            .await
            .map_err(ClientDiffFileError::Transport)?
            .ok_or(ClientDiffFileError::Transport(
                ViewerClientError::Unavailable,
            ))?;
        if response.identity != rows.request.identity || response.sequence != sequence {
            return Err(ClientDiffFileError::InvalidResponse);
        }
        sequence = sequence
            .checked_add(1)
            .ok_or(ClientDiffFileError::InvalidResponse)?;
        rows.accept(response.event)?;
        if let Some(line_number_digits) = rows.finished {
            return Ok(LoadedRowWindow {
                rows: rows.rows,
                line_number_digits,
            });
        }
    }
}

struct RowWindow {
    request: WindowRequest,
    rows: ViewerRows,
    started: bool,
    next: u32,
    finished: Option<u32>,
    bytes: usize,
}

impl RowWindow {
    fn new(request: WindowRequest) -> Self {
        let count = request.range.count() as usize;
        let rows = match request.identity.render_options.layout {
            ViewerDiffLayout::Unified => ViewerRows::Unified(Vec::with_capacity(count)),
            ViewerDiffLayout::Split => ViewerRows::Split(Vec::with_capacity(count)),
        };
        Self {
            bytes: client_diff_cache::row_window_bytes(&rows),
            next: request.range.start(),
            request,
            rows,
            started: false,
            finished: None,
        }
    }

    fn accept(&mut self, event: ViewerRowEvent) -> Result<(), ClientDiffFileError> {
        let file = match &event {
            ViewerRowEvent::FileStarted { file, .. }
            | ViewerRowEvent::UnifiedRows { file, .. }
            | ViewerRowEvent::SplitRows { file, .. }
            | ViewerRowEvent::FileFinished { file, .. }
            | ViewerRowEvent::FileFailed { file, .. } => file,
        };
        if file != &self.request.file || self.finished.is_some() {
            return Err(ClientDiffFileError::InvalidResponse);
        }
        match event {
            ViewerRowEvent::FileStarted {
                row_count,
                start_row,
                ..
            } if !self.started && row_count == self.request.total && start_row == self.next => {
                self.started = true;
            }
            ViewerRowEvent::UnifiedRows {
                start_row, rows, ..
            } if self.started => {
                self.advance(start_row, rows.len())?;
                self.add_bytes(rows.iter().map(client_diff_cache::unified_row_bytes).sum())?;
                append_unified(&mut self.rows, rows)?;
            }
            ViewerRowEvent::SplitRows {
                start_row, rows, ..
            } if self.started => {
                self.advance(start_row, rows.len())?;
                self.add_bytes(rows.iter().map(client_diff_cache::split_row_bytes).sum())?;
                append_split(&mut self.rows, rows)?;
            }
            ViewerRowEvent::FileFinished {
                end_row,
                line_number_digits,
                ..
            } if self.started
                && end_row == self.next
                && end_row == self.request.range.end()
                && line_number_digits > 0 =>
            {
                self.finished = Some(line_number_digits);
            }
            ViewerRowEvent::FileFailed {
                message, retryable, ..
            } => {
                return Err(ClientDiffFileError::Server { message, retryable });
            }
            _ => return Err(ClientDiffFileError::InvalidResponse),
        }
        Ok(())
    }

    fn advance(&mut self, start: u32, count: usize) -> Result<(), ClientDiffFileError> {
        let count = u32::try_from(count).map_err(|_| ClientDiffFileError::InvalidResponse)?;
        let end = start
            .checked_add(count)
            .ok_or(ClientDiffFileError::InvalidResponse)?;
        if start != self.next || count == 0 || end > self.request.range.end() {
            return Err(ClientDiffFileError::InvalidResponse);
        }
        self.next = end;
        Ok(())
    }

    fn add_bytes(&mut self, bytes: usize) -> Result<(), ClientDiffFileError> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(super::window_too_large)?;
        if self.bytes > client_diff_cache::RETAINED_ROW_BYTES_MAX {
            return Err(super::window_too_large());
        }
        Ok(())
    }
}

fn append_unified(
    target: &mut ViewerRows,
    rows: Vec<gtl_wire::viewer::ViewerUnifiedRow>,
) -> Result<(), ClientDiffFileError> {
    let ViewerRows::Unified(retained) = target else {
        return Err(ClientDiffFileError::InvalidResponse);
    };
    retained.extend(rows);
    Ok(())
}

fn append_split(
    target: &mut ViewerRows,
    rows: Vec<gtl_wire::viewer::ViewerSplitRow>,
) -> Result<(), ClientDiffFileError> {
    let ViewerRows::Split(retained) = target else {
        return Err(ClientDiffFileError::InvalidResponse);
    };
    retained.extend(rows);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestResult, viewer_tab_id};

    fn request(layout: ViewerDiffLayout) -> TestResult<WindowRequest> {
        Ok(WindowRequest {
            identity: ViewerViewIdentity {
                tab_id: viewer_tab_id(1)?,
                range_generation: gtl_models::viewer::ViewerRangeGeneration::new(1),
                selection_generation: gtl_models::viewer::ViewerSelectionGeneration::default(),
                render_options: gtl_wire::viewer::ViewerRenderOptions {
                    layout,
                    density: gtl_wire::viewer::ViewerDiffDensity::Compact,
                },
            },
            file: ViewerDiffFileId::for_index(0),
            total: 130,
            range: ViewerRowRange::try_new(128, 2)?,
        })
    }

    fn started() -> ViewerRowEvent {
        ViewerRowEvent::FileStarted {
            file: ViewerDiffFileId::for_index(0),
            row_count: 130,
            start_row: 128,
        }
    }

    fn row(start_row: u32) -> ViewerRowEvent {
        ViewerRowEvent::UnifiedRows {
            file: ViewerDiffFileId::for_index(0),
            start_row,
            rows: vec![gtl_wire::viewer::ViewerUnifiedRow::Meta(
                "retained".to_owned(),
            )],
        }
    }

    #[test]
    fn a_distant_window_accepts_contiguous_fragments_only() -> TestResult {
        let mut window = RowWindow::new(request(ViewerDiffLayout::Unified)?);
        assert!(window.accept(row(128)).is_err());
        window.accept(started())?;
        window.accept(row(128))?;
        assert!(window.accept(row(128)).is_err());
        window.accept(row(129))?;
        window.accept(ViewerRowEvent::FileFinished {
            file: ViewerDiffFileId::for_index(0),
            end_row: 130,
            line_number_digits: 3,
        })?;
        assert!(window.accept(row(130)).is_err());
        assert_eq!(window.finished, Some(3));
        Ok(())
    }

    #[test]
    fn truncated_or_wrong_layout_windows_are_rejected() -> TestResult {
        let mut window = RowWindow::new(request(ViewerDiffLayout::Split)?);
        window.accept(started())?;
        assert!(window.accept(row(128)).is_err());
        assert!(
            window
                .accept(ViewerRowEvent::FileFinished {
                    file: ViewerDiffFileId::for_index(0),
                    end_row: 130,
                    line_number_digits: 3
                })
                .is_err()
        );
        Ok(())
    }
}
