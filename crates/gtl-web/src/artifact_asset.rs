use js_sys::{Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast as _, JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;

pub(crate) const MANIFEST_MAX_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const SYNTAX_PACK_MAX_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const PAGE_MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactAssetKind {
    Manifest,
    SyntaxCatalog,
    DiffPage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArtifactAssetError {
    DecompressionUnavailable,
    Missing,
    InvalidMetadata,
    InvalidBase64,
    InvalidGzip,
    TooLarge,
    InvalidResult,
}

impl ArtifactAssetError {
    pub(crate) const fn message(self, kind: ArtifactAssetKind) -> &'static str {
        match self {
            Self::DecompressionUnavailable => {
                "This browser cannot decompress this offline diff artifact."
            }
            Self::Missing => match kind {
                ArtifactAssetKind::Manifest => {
                    "This artifact does not contain its compressed diff manifest."
                }
                ArtifactAssetKind::SyntaxCatalog => {
                    "This artifact does not contain its compressed syntax catalog."
                }
                ArtifactAssetKind::DiffPage => {
                    "This artifact does not contain the requested compressed diff page."
                }
            },
            Self::TooLarge => "An embedded artifact asset exceeds its safe decoded size limit.",
            Self::InvalidMetadata
            | Self::InvalidBase64
            | Self::InvalidGzip
            | Self::InvalidResult => match kind {
                ArtifactAssetKind::Manifest => {
                    "This artifact contains a corrupt compressed diff manifest."
                }
                ArtifactAssetKind::SyntaxCatalog => {
                    "This artifact contains a corrupt compressed syntax catalog."
                }
                ArtifactAssetKind::DiffPage => {
                    "This artifact contains a corrupt compressed diff page."
                }
            },
        }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch, js_name = __gtlLoadCompressedAsset)]
    fn load_compressed_asset(id: &str, max_bytes: u32) -> Result<Promise, JsValue>;
}

pub(crate) async fn load(id: &str, max_bytes: usize) -> Result<Vec<u8>, ArtifactAssetError> {
    let max_bytes = u32::try_from(max_bytes).map_err(|_| ArtifactAssetError::TooLarge)?;
    let promise = load_compressed_asset(id, max_bytes).map_err(|error| classify_error(&error))?;
    let value = JsFuture::from(promise)
        .await
        .map_err(|error| classify_error(&error))?;
    if !value.is_instance_of::<Uint8Array>() {
        return Err(ArtifactAssetError::InvalidResult);
    }
    let bytes = Uint8Array::new(&value);
    if bytes.length() > max_bytes {
        return Err(ArtifactAssetError::TooLarge);
    }
    let mut output = vec![0_u8; bytes.length() as usize];
    bytes.copy_to(&mut output);
    Ok(output)
}

pub(crate) fn remove(id: &str) {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
    {
        element.remove();
    }
}

fn classify_error(error: &JsValue) -> ArtifactAssetError {
    let code = Reflect::get(error, &JsValue::from_str("code"))
        .ok()
        .and_then(|value| value.as_string());
    classify_code(code.as_deref())
}

fn classify_code(code: Option<&str>) -> ArtifactAssetError {
    match code {
        Some("UNAVAILABLE") => ArtifactAssetError::DecompressionUnavailable,
        Some("MISSING") => ArtifactAssetError::Missing,
        Some("INVALID_METADATA") => ArtifactAssetError::InvalidMetadata,
        Some("INVALID_BASE64") => ArtifactAssetError::InvalidBase64,
        Some("INVALID_GZIP") => ArtifactAssetError::InvalidGzip,
        Some("TOO_LARGE") => ArtifactAssetError::TooLarge,
        _ => ArtifactAssetError::InvalidResult,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_error_codes_remain_typed_and_bounded() {
        assert_eq!(
            classify_code(Some("UNAVAILABLE")),
            ArtifactAssetError::DecompressionUnavailable
        );
        assert_eq!(
            classify_code(Some("INVALID_BASE64")),
            ArtifactAssetError::InvalidBase64
        );
        assert_eq!(classify_code(Some("MISSING")), ArtifactAssetError::Missing);
        assert_eq!(
            ArtifactAssetError::Missing.message(ArtifactAssetKind::DiffPage),
            "This artifact does not contain the requested compressed diff page."
        );
        assert_eq!(
            classify_code(Some("INVALID_GZIP")),
            ArtifactAssetError::InvalidGzip
        );
        assert_eq!(
            classify_code(Some("TOO_LARGE")),
            ArtifactAssetError::TooLarge
        );
        assert_eq!(
            classify_code(Some("unknown")),
            ArtifactAssetError::InvalidResult
        );
    }
}
