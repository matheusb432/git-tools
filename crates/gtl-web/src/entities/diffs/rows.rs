#[cfg(feature = "desktop")]
pub(crate) use gtl_wire::viewer::ViewerRowEvent;
pub(crate) use gtl_wire::viewer::{
    ViewerCodeLine, ViewerCodeSpan, ViewerSplitCell, ViewerSplitRow, ViewerSyntaxClass,
    ViewerUnifiedRow, ViewerUnifiedSourceRow,
};
