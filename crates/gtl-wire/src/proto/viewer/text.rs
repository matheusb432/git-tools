use super::{
    ViewerCodecError, decode_viewer_diff_file_id, decode_viewer_view_identity,
    encode_viewer_view_identity, required,
};
use crate::{
    v1,
    viewer::{ReadViewerDiffText, ViewerDiffTextLine, ViewerRowRange},
};

#[must_use]
pub fn encode_request(request: &ReadViewerDiffText) -> v1::ReadViewerDiffTextRequest {
    v1::ReadViewerDiffTextRequest {
        identity: Some(encode_viewer_view_identity(request.identity)),
        file_id: request.file.as_str().to_owned(),
        row_range: Some(v1::ViewerRowRange {
            start: request.row_range.start(),
            count: request.row_range.count(),
        }),
        old_side: request.old_side,
    }
}

pub fn decode_request(
    request: v1::ReadViewerDiffTextRequest,
) -> Result<ReadViewerDiffText, ViewerCodecError> {
    let range = required(request.row_range)?;
    Ok(ReadViewerDiffText {
        identity: decode_viewer_view_identity(required(request.identity)?)?,
        file: decode_viewer_diff_file_id(request.file_id)?,
        row_range: ViewerRowRange::try_new(range.start, range.count)
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        old_side: request.old_side,
    })
}

#[must_use]
pub fn encode_response(lines: Vec<ViewerDiffTextLine>) -> v1::ReadViewerDiffTextResponse {
    v1::ReadViewerDiffTextResponse {
        lines: lines
            .into_iter()
            .map(|line| v1::ViewerDiffTextLine {
                line_number: line.line_number,
                text: line.text,
            })
            .collect(),
    }
}

pub fn decode_response(
    response: v1::ReadViewerDiffTextResponse,
) -> Result<Vec<ViewerDiffTextLine>, ViewerCodecError> {
    response
        .lines
        .into_iter()
        .map(|line| {
            if line.line_number == 0 {
                return Err(ViewerCodecError::InvalidMessage);
            }
            Ok(ViewerDiffTextLine {
                line_number: line.line_number,
                text: line.text,
            })
        })
        .collect()
}
