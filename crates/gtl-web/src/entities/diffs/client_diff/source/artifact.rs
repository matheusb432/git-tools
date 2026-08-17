use std::{cell::RefCell, collections::HashMap};

use gtl_wire::viewer::{
    LoadViewerDiffLines, ViewerArtifactPage, ViewerArtifactPageId, ViewerDiffLines,
};

use crate::artifact_asset::{self, ArtifactAssetError, ArtifactAssetKind, PAGE_MAX_BYTES};

thread_local! {
    static PAGE_CACHE: RefCell<HashMap<ViewerArtifactPageId, ViewerDiffLines>> =
        RefCell::new(HashMap::new());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactDiffSourceError {
    Asset(ArtifactAssetError),
    InvalidPayload,
    IdentityMismatch,
}

impl ArtifactDiffSourceError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Asset(error) => error.message(ArtifactAssetKind::DiffPage),
            Self::InvalidPayload => {
                "The embedded diff page is invalid. Recreate this artifact to view it."
            }
            Self::IdentityMismatch => {
                "The embedded diff page has a mismatched identity. Recreate this artifact to view it."
            }
        }
    }
}

pub(super) async fn load_artifact_diff_lines(
    request: &LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ArtifactDiffSourceError> {
    let id = ViewerArtifactPageId::for_request(request);
    if let Some(page) = PAGE_CACHE.with(|cache| cache.borrow().get(&id).cloned()) {
        return Ok(page);
    }

    let bytes = artifact_asset::load(id.as_str(), PAGE_MAX_BYTES)
        .await
        .map_err(ArtifactDiffSourceError::Asset)?;
    let page = decode_page(&bytes, request)?;
    artifact_asset::remove(id.as_str());
    PAGE_CACHE.with(|cache| {
        cache.borrow_mut().insert(id, page.clone());
    });
    Ok(page)
}

fn decode_page(
    bytes: &[u8],
    request: &LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ArtifactDiffSourceError> {
    let embedded = serde_json::from_slice::<ViewerArtifactPage>(bytes)
        .map_err(|_| ArtifactDiffSourceError::InvalidPayload)?;
    let expected_id = ViewerArtifactPageId::for_request(request);
    if embedded.id != expected_id
        || embedded.page.identity != request.identity
        || embedded.page.file != request.file
        || embedded.page.cursor != request.cursor
    {
        return Err(ArtifactDiffSourceError::IdentityMismatch);
    }
    Ok(embedded.page)
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use gtl_models::viewer::{ViewerRangeGeneration, ViewerSelectionGeneration, ViewerTabId};
    use gtl_wire::viewer::{
        ViewerDiffCursor, ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout,
        ViewerRenderOptions, ViewerViewIdentity,
    };

    use super::*;

    fn request() -> Result<LoadViewerDiffLines, Box<dyn Error>> {
        Ok(LoadViewerDiffLines {
            identity: ViewerViewIdentity {
                tab_id: ViewerTabId::try_new(1)?,
                range_generation: ViewerRangeGeneration::new(2),
                selection_generation: ViewerSelectionGeneration::new(3),
                render_options: ViewerRenderOptions {
                    layout: ViewerDiffLayout::Unified,
                    density: ViewerDiffDensity::Compact,
                },
            },
            file: ViewerDiffFileId::for_index(4),
            cursor: ViewerDiffCursor::new(5),
        })
    }

    #[test]
    fn decoder_rejects_a_page_bound_to_another_request() -> Result<(), Box<dyn Error>> {
        let request = request()?;
        let mut page = ViewerDiffLines {
            identity: request.identity,
            file: request.file.clone(),
            cursor: request.cursor,
            lines: vec!["+line".into()],
            next: None,
        };
        page.cursor = ViewerDiffCursor::new(6);
        let bytes = serde_json::to_vec(&ViewerArtifactPage {
            id: ViewerArtifactPageId::for_request(&request),
            page,
        })?;

        assert_eq!(
            decode_page(&bytes, &request),
            Err(ArtifactDiffSourceError::IdentityMismatch)
        );
        Ok(())
    }

    #[test]
    fn decoder_rejects_corrupt_json() -> Result<(), Box<dyn Error>> {
        assert_eq!(
            decode_page(b"not JSON", &request()?),
            Err(ArtifactDiffSourceError::InvalidPayload)
        );
        Ok(())
    }
}
