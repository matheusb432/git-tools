use gtl_models::{
    diffs::{ExtensionFilter, ExtensionFilterMode, FileExtensions},
    viewer::ViewerTabId,
};

use super::{ViewerCodecError, required};
use crate::{
    v1,
    viewer::file_filters::{SetViewerFileFilters, ViewerFileFilters},
};

const EXTENSION_COUNT_MAX: usize = 4096;
const EXTENSION_BYTES_MAX: usize = 255;

fn decode_extensions(extensions: Vec<String>) -> Result<FileExtensions, ViewerCodecError> {
    if extensions.len() > EXTENSION_COUNT_MAX
        || extensions.iter().any(|extension| {
            extension.len() > EXTENSION_BYTES_MAX || extension.contains(['/', '\\', '\0'])
        })
    {
        return Err(ViewerCodecError::InvalidMessage);
    }
    Ok(FileExtensions::new(extensions))
}

pub(super) fn encode_extension_filter(filter: ExtensionFilter) -> v1::ViewerExtensionFilter {
    let (mode, extensions) = filter.into_parts();
    let mode = match mode {
        ExtensionFilterMode::Hide => v1::ViewerExtensionFilterMode::Hide,
        ExtensionFilterMode::Only => v1::ViewerExtensionFilterMode::Only,
    };
    v1::ViewerExtensionFilter {
        mode: mode as i32,
        extensions: extensions.into(),
    }
}

pub(super) fn decode_extension_filter(
    filter: v1::ViewerExtensionFilter,
) -> Result<ExtensionFilter, ViewerCodecError> {
    let mode = match v1::ViewerExtensionFilterMode::try_from(filter.mode) {
        Ok(v1::ViewerExtensionFilterMode::Hide) => ExtensionFilterMode::Hide,
        Ok(v1::ViewerExtensionFilterMode::Only) => ExtensionFilterMode::Only,
        Ok(v1::ViewerExtensionFilterMode::Unspecified) | Err(_) => {
            return Err(ViewerCodecError::InvalidMessage);
        }
    };
    Ok(ExtensionFilter::new(
        mode,
        decode_extensions(filter.extensions)?,
    ))
}

#[must_use]
pub fn encode_get(tab_id: ViewerTabId) -> v1::GetViewerFileFiltersRequest {
    v1::GetViewerFileFiltersRequest {
        tab_id: tab_id.into_inner(),
    }
}

pub fn decode_get(value: v1::GetViewerFileFiltersRequest) -> Result<ViewerTabId, ViewerCodecError> {
    ViewerTabId::try_new(value.tab_id).map_err(|_| ViewerCodecError::InvalidMessage)
}

#[must_use]
pub fn encode_filters(value: ViewerFileFilters) -> v1::GetViewerFileFiltersResponse {
    v1::GetViewerFileFiltersResponse {
        filter: Some(encode_extension_filter(value.filter)),
        extensions: value.extensions,
    }
}

pub fn decode_filters(
    value: v1::GetViewerFileFiltersResponse,
) -> Result<ViewerFileFilters, ViewerCodecError> {
    Ok(ViewerFileFilters {
        filter: decode_extension_filter(required(value.filter)?)?,
        extensions: decode_extensions(value.extensions)?.into(),
    })
}

#[must_use]
pub fn encode_set(value: SetViewerFileFilters) -> v1::SetViewerFileFiltersRequest {
    v1::SetViewerFileFiltersRequest {
        tab_id: value.tab_id.into_inner(),
        filter: Some(encode_extension_filter(value.filter)),
    }
}

pub fn decode_set(
    value: v1::SetViewerFileFiltersRequest,
) -> Result<SetViewerFileFilters, ViewerCodecError> {
    Ok(SetViewerFileFilters {
        tab_id: ViewerTabId::try_new(value.tab_id).map_err(|_| ViewerCodecError::InvalidMessage)?,
        filter: decode_extension_filter(required(value.filter)?)?,
    })
}
