#[cfg(feature = "desktop")]
mod loading;
use dioxus::prelude::*;
#[cfg(feature = "artifact")]
use gtl_wire::viewer::{ViewerFileRows, ViewerRows};
use gtl_wire::viewer::{ViewerFileSummary, ViewerViewIdentity};
#[cfg(feature = "desktop")]
pub(super) use loading::{ClientDiffFetch, LoadedRowWindow, window_too_large};
#[cfg(feature = "desktop")]
pub(crate) use loading::{
    ClientDiffWindow, ClientDiffWorkspaceController, use_client_diff_workspace,
};

use super::{ViewerSplitRow, ViewerUnifiedRow};
#[cfg(feature = "desktop")]
use crate::shared::viewer_client::ViewerClientError;

pub(crate) const CLIENT_LINE_BATCH_SIZE: usize = 64;

#[derive(Debug, Store, Clone, Default, PartialEq, Eq)]
pub(crate) struct ClientDiffRows {
    pub(crate) unified: Vec<Vec<ViewerUnifiedRow>>,
    pub(crate) split: Vec<Vec<ViewerSplitRow>>,
}

impl ClientDiffRows {
    #[cfg(any(feature = "artifact", test))]
    fn append_unified(&mut self, rows: Vec<ViewerUnifiedRow>) {
        self.unified.extend(bounded_batches(rows));
    }

    #[cfg(any(feature = "artifact", test))]
    fn append_split(&mut self, rows: Vec<ViewerSplitRow>) {
        self.split.extend(bounded_batches(rows));
    }
}

#[cfg(any(feature = "artifact", test))]
fn bounded_batches<Row>(rows: Vec<Row>) -> impl Iterator<Item = Vec<Row>> {
    let mut rows = rows.into_iter();
    std::iter::from_fn(move || {
        let batch = rows
            .by_ref()
            .take(CLIENT_LINE_BATCH_SIZE)
            .collect::<Vec<_>>();
        (!batch.is_empty()).then_some(batch)
    })
}

#[cfg(feature = "desktop")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffFileError {
    Transport(ViewerClientError),
    InvalidResponse,
    Server {
        failure: gtl_models::failure::Failure,
        retryable: bool,
    },
}

#[cfg(feature = "desktop")]
impl ClientDiffFileError {
    pub(crate) const fn retryable(&self) -> bool {
        match self {
            Self::Transport(_) | Self::InvalidResponse => true,
            Self::Server { retryable, .. } => *retryable,
        }
    }

    const fn retries_automatically(&self) -> bool {
        match self {
            Self::Transport(error) => matches!(
                error.class(),
                gtl_models::failure::ErrorClass::Unavailable
                    | gtl_models::failure::ErrorClass::ResourceExhausted
            ),
            Self::InvalidResponse | Self::Server { .. } => false,
        }
    }
}

#[cfg(feature = "desktop")]
impl std::fmt::Display for ClientDiffFileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(ViewerClientError::Disconnected | ViewerClientError::StreamClosed) => {
                formatter.write_str("The diff row stream disconnected.")
            }
            Self::Transport(error) => error.fmt(formatter),
            Self::InvalidResponse => formatter.write_str(
                "The server returned invalid diff rows. Retry this view to load it again.",
            ),
            Self::Server { failure, .. } => failure.fmt(formatter),
        }
    }
}

#[cfg(feature = "desktop")]
impl std::error::Error for ClientDiffFileError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientDiffFileState {
    #[cfg(feature = "desktop")]
    Loading,
    Complete,
    #[cfg(feature = "desktop")]
    Error(ClientDiffFileError),
}

#[derive(Debug, Store, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffFile {
    pub(crate) summary: ViewerFileSummary,
    pub(crate) rows: ClientDiffRows,
    pub(crate) line_number_digits: u32,
    pub(crate) state: ClientDiffFileState,
}

impl ClientDiffFile {
    #[cfg(feature = "desktop")]
    fn loading(summary: ViewerFileSummary, layout: gtl_wire::viewer::ViewerDiffLayout) -> Self {
        let count = summary.row_count.div_ceil(CLIENT_LINE_BATCH_SIZE);
        let rows = match layout {
            gtl_wire::viewer::ViewerDiffLayout::Unified => ClientDiffRows {
                unified: vec![Vec::new(); count],
                split: Vec::new(),
            },
            gtl_wire::viewer::ViewerDiffLayout::Split => ClientDiffRows {
                unified: Vec::new(),
                split: vec![Vec::new(); count],
            },
        };
        Self {
            summary,
            rows,
            line_number_digits: 1,
            state: ClientDiffFileState::Loading,
        }
    }
}

#[derive(Debug, Store, Clone, PartialEq, Eq)]
pub(crate) struct ClientDiffWorkspace {
    pub(crate) identity: ViewerViewIdentity,
    pub(crate) files: Vec<ClientDiffFile>,
}

impl ClientDiffWorkspace {
    #[cfg(feature = "desktop")]
    pub(super) fn loading(identity: ViewerViewIdentity, files: Vec<ViewerFileSummary>) -> Self {
        Self {
            identity,
            files: files
                .into_iter()
                .map(|file| ClientDiffFile::loading(file, identity.render_options.layout))
                .collect(),
        }
    }
}

#[cfg(feature = "artifact")]
pub(crate) fn static_diff_workspace(
    identity: ViewerViewIdentity,
    files: Vec<(ViewerFileSummary, ViewerFileRows)>,
) -> ClientDiffWorkspace {
    let files = files
        .into_iter()
        .map(|(summary, rows)| static_diff_file(summary, rows))
        .collect();
    ClientDiffWorkspace { identity, files }
}

#[cfg(feature = "artifact")]
fn static_diff_file(summary: ViewerFileSummary, rows: ViewerFileRows) -> ClientDiffFile {
    let line_number_digits = rows.line_number_digits;
    let mut projected = ClientDiffRows::default();
    match rows.rows {
        ViewerRows::Unified(rows) => projected.append_unified(rows),
        ViewerRows::Split(rows) => projected.append_split(rows),
    }
    ClientDiffFile {
        summary,
        rows: projected,
        line_number_digits,
        state: ClientDiffFileState::Complete,
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;
    #[test]
    fn received_rows_are_rebatched_to_the_render_limit() {
        let mut unified = ClientDiffRows::default();
        unified.append_unified(
            (0..129)
                .map(|index| ViewerUnifiedRow::Meta(index.to_string()))
                .collect(),
        );
        let unified_lengths = unified.unified.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(unified_lengths, vec![64, 64, 1]);

        let mut split = ClientDiffRows::default();
        split.append_split(
            (0..129)
                .map(|index| ViewerSplitRow::Meta(index.to_string()))
                .collect(),
        );
        let split_lengths = split.split.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(split_lengths, vec![64, 64, 1]);
    }
}
