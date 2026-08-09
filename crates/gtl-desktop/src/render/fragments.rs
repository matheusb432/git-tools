//! Independently swappable htmx fragments of the viewer shell, one module per
//! region. `SwapMode` and `SwapFeedback` model how a fragment participates in
//! a compound response: the primary swap target versus an out-of-band sibling,
//! and the one-shot feedback a mutation carries into the next render.

mod controls;
mod history;
mod tabs;
pub(super) mod theme;
mod view;
mod view_chunk;

pub(super) use history::history;
pub(super) use tabs::{MobileNavigationCounts, tabs};
pub(super) use view::{loading_template, view};
pub(crate) use view_chunk::view_chunk;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SwapMode {
    Primary,
    OutOfBand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwapFeedback {
    None,
    TabClosed,
    LiveViewDeleted,
}

impl SwapMode {
    const fn out_of_band(self) -> Option<&'static str> {
        match self {
            Self::Primary => None,
            Self::OutOfBand => Some("outerHTML"),
        }
    }
}
