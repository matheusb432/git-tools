use std::fmt;

use application::viewer::{
    DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, Theme, ViewerTabId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewerSettingChange {
    Layout(DiffLayout),
    Density(DiffDensity),
    Theme(Theme),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ViewerRoute {
    View {
        tab: ViewerTabId,
        options: RenderOptions,
    },
    CommitPatch {
        tab: ViewerTabId,
        sha: String,
        options: RenderOptions,
    },
    Activate {
        tab: ViewerTabId,
    },
    Refresh {
        tab: ViewerTabId,
    },
    Close {
        tab: ViewerTabId,
    },
    DeleteLiveView {
        tab: ViewerTabId,
    },
    History,
    OpenHistory {
        render: RenderHistoryId,
    },
    Settings(ViewerSettingChange),
}

impl fmt::Display for ViewerRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::View { tab, options } => write!(
                formatter,
                "/tabs/{tab}/view?layout={}&density={}",
                options.layout(),
                options.density()
            ),
            Self::CommitPatch { tab, sha, options } => write!(
                formatter,
                "/tabs/{tab}/commits/{sha}/view?layout={}&density={}",
                options.layout(),
                options.density()
            ),
            Self::Activate { tab } => write!(formatter, "/tabs/{tab}/activate"),
            Self::Refresh { tab } => write!(formatter, "/tabs/{tab}/refresh"),
            Self::Close { tab } => write!(formatter, "/tabs/{tab}/close"),
            Self::DeleteLiveView { tab } => write!(formatter, "/tabs/{tab}/live-view"),
            Self::History => formatter.write_str("/history"),
            Self::OpenHistory { render } => write!(formatter, "/history/{render}/open"),
            Self::Settings(ViewerSettingChange::Layout(value)) => {
                write!(formatter, "/settings?layout={value}")
            }
            Self::Settings(ViewerSettingChange::Density(value)) => {
                write!(formatter, "/settings?density={value}")
            }
            Self::Settings(ViewerSettingChange::Theme(value)) => {
                write!(formatter, "/settings?theme={value}")
            }
        }
    }
}
