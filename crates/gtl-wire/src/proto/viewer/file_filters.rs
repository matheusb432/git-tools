use gtl_models::{diffs::ExcludedExtensions, paths::ProjectName, viewer::ViewerTabId};

use super::{ViewerCodecError, required};
use crate::{
    v1,
    viewer::{
        FieldUpdate,
        file_filters::{SetViewerFileFilters, UpdateDiffExclusions, ViewerFileFilters},
    },
};

fn encode_extensions(value: ExcludedExtensions) -> v1::ExtensionsValue {
    v1::ExtensionsValue {
        extensions: value.into(),
    }
}

fn decode_extensions(value: v1::ExtensionsValue) -> Result<ExcludedExtensions, ViewerCodecError> {
    if value.extensions.len() > 4096
        || value
            .extensions
            .iter()
            .any(|extension| extension.len() > 255 || extension.contains(['/', '\\', '\0']))
    {
        return Err(ViewerCodecError::InvalidMessage);
    }
    Ok(ExcludedExtensions::new(value.extensions))
}

fn encode_update(value: FieldUpdate<ExcludedExtensions>) -> Option<v1::ExtensionsFieldUpdate> {
    use v1::extensions_field_update::Operation;
    let operation = match value {
        FieldUpdate::Update(value) => Operation::Update(encode_extensions(value)),
        FieldUpdate::Clear => Operation::Clear(v1::ClearSetting {}),
        FieldUpdate::Unchanged => return None,
    };
    Some(v1::ExtensionsFieldUpdate {
        operation: Some(operation),
    })
}

fn decode_update(
    value: Option<v1::ExtensionsFieldUpdate>,
) -> Result<FieldUpdate<ExcludedExtensions>, ViewerCodecError> {
    use v1::extensions_field_update::Operation;
    Ok(match value {
        None => FieldUpdate::Unchanged,
        Some(value) => match required(value.operation)? {
            Operation::Update(value) => FieldUpdate::Update(decode_extensions(value)?),
            Operation::Clear(_) => FieldUpdate::Clear,
        },
    })
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
        excluded: Some(encode_extensions(value.excluded)),
        extensions: value.extensions,
        project: value.project.map(|value| value.to_string()),
        saved: value.saved.map(encode_extensions),
        defaults: Some(encode_extensions(value.defaults)),
    }
}

pub fn decode_filters(
    value: v1::GetViewerFileFiltersResponse,
) -> Result<ViewerFileFilters, ViewerCodecError> {
    Ok(ViewerFileFilters {
        excluded: decode_extensions(required(value.excluded)?)?,
        extensions: decode_extensions(v1::ExtensionsValue {
            extensions: value.extensions,
        })?
        .into(),
        project: value
            .project
            .map(ProjectName::try_new)
            .transpose()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        saved: value.saved.map(decode_extensions).transpose()?,
        defaults: decode_extensions(required(value.defaults)?)?,
    })
}

#[must_use]
pub fn encode_set(value: SetViewerFileFilters) -> v1::SetViewerFileFiltersRequest {
    v1::SetViewerFileFiltersRequest {
        tab_id: value.tab_id.into_inner(),
        exclusions: encode_update(value.exclusions),
        expected: value.expected.map(encode_extensions),
    }
}

pub fn decode_set(
    value: v1::SetViewerFileFiltersRequest,
) -> Result<SetViewerFileFilters, ViewerCodecError> {
    Ok(SetViewerFileFilters {
        tab_id: ViewerTabId::try_new(value.tab_id).map_err(|_| ViewerCodecError::InvalidMessage)?,
        exclusions: decode_update(value.exclusions)?,
        expected: value.expected.map(decode_extensions).transpose()?,
    })
}

#[must_use]
pub fn encode_defaults(value: UpdateDiffExclusions) -> v1::UpdateDiffExclusionsRequest {
    v1::UpdateDiffExclusionsRequest {
        project: value.project.map(|value| value.to_string()),
        extensions: encode_update(value.extensions),
        expected: value.expected.map(encode_extensions),
    }
}

pub fn decode_defaults(
    value: v1::UpdateDiffExclusionsRequest,
) -> Result<UpdateDiffExclusions, ViewerCodecError> {
    Ok(UpdateDiffExclusions {
        project: value
            .project
            .map(ProjectName::try_new)
            .transpose()
            .map_err(|_| ViewerCodecError::InvalidMessage)?,
        extensions: decode_update(value.extensions)?,
        expected: value.expected.map(decode_extensions).transpose()?,
    })
}
