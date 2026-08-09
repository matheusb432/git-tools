use gtl_application::{diffs::View, viewer::RenderOptions};
use maud::{Markup, html};

use super::files;
use crate::{
    rows::{INTRALINE_PRESENTATION_CLASSES, ROW_PRESENTATION_CLASSES, SPLIT_PRESENTATION_CLASSES},
    syntax::PreviewResult,
};

pub(crate) fn shell(view: &View, options: RenderOptions) -> PreviewResult<Markup> {
    Ok(html! {
        div data-gtl-diff-document class={
            "diff-document copy-ctx "
            (ROW_PRESENTATION_CLASSES) " "
            (SPLIT_PRESENTATION_CLASSES) " "
            (INTRALINE_PRESENTATION_CLASSES)
        } {
            (files::diff_document_shell(view, options)?)
        }
    })
}
