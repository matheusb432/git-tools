use std::{cell::RefCell, collections::HashMap};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtl_contracts::viewer::{
    LoadViewerDiffLines, ViewerArtifactPage, ViewerArtifactPageId, ViewerDiffLines,
};

thread_local! {
    static PAGE_CACHE: RefCell<HashMap<ViewerArtifactPageId, ViewerDiffLines>> =
        RefCell::new(HashMap::new());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactDiffSourceError {
    Missing,
    Invalid,
}

pub(super) fn load_artifact_diff_lines(
    request: &LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ArtifactDiffSourceError> {
    let id = ViewerArtifactPageId::for_request(request);
    if let Some(page) = PAGE_CACHE.with(|cache| cache.borrow().get(&id).cloned()) {
        return Ok(page);
    }

    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or(ArtifactDiffSourceError::Missing)?;
    let element = document
        .get_element_by_id(id.as_str())
        .ok_or(ArtifactDiffSourceError::Missing)?;
    let encoded = element
        .text_content()
        .ok_or(ArtifactDiffSourceError::Invalid)?;
    let page = decode_page(encoded.trim(), request)?;
    element.remove();
    PAGE_CACHE.with(|cache| {
        cache.borrow_mut().insert(id, page.clone());
    });
    Ok(page)
}

fn decode_page(
    encoded: &str,
    request: &LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ArtifactDiffSourceError> {
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| ArtifactDiffSourceError::Invalid)?;
    let embedded = serde_json::from_slice::<ViewerArtifactPage>(&bytes)
        .map_err(|_| ArtifactDiffSourceError::Invalid)?;
    let expected_id = ViewerArtifactPageId::for_request(request);
    if embedded.id != expected_id
        || embedded.page.identity != request.identity
        || embedded.page.file != request.file
        || embedded.page.cursor != request.cursor
    {
        return Err(ArtifactDiffSourceError::Invalid);
    }
    Ok(embedded.page)
}

#[cfg(test)]
mod tests {
    use gtl_contracts::viewer::{
        ViewerDiffCursor, ViewerDiffDensity, ViewerDiffFileId, ViewerDiffLayout,
        ViewerRenderOptions, ViewerViewIdentity,
    };

    use super::*;

    fn request() -> LoadViewerDiffLines {
        LoadViewerDiffLines {
            identity: ViewerViewIdentity {
                tab_id: 1,
                range_generation: 2,
                selection_generation: 3,
                render_options: ViewerRenderOptions {
                    layout: ViewerDiffLayout::Unified,
                    density: ViewerDiffDensity::Compact,
                },
            },
            file: ViewerDiffFileId::for_index(4),
            cursor: ViewerDiffCursor::new(5),
        }
    }

    #[test]
    fn decoder_rejects_a_page_bound_to_another_request() {
        let request = request();
        let mut page = ViewerDiffLines {
            identity: request.identity,
            file: request.file.clone(),
            cursor: request.cursor,
            lines: vec!["+line".into()],
            next: None,
        };
        page.cursor = ViewerDiffCursor::new(6);
        let encoded = STANDARD.encode(
            serde_json::to_vec(&ViewerArtifactPage {
                id: ViewerArtifactPageId::for_request(&request),
                page,
            })
            .expect("serialize embedded page"),
        );

        assert_eq!(
            decode_page(&encoded, &request),
            Err(ArtifactDiffSourceError::Invalid)
        );
    }
}
