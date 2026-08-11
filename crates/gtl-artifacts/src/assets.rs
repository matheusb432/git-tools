use anyhow::{Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub(crate) const TAILWIND_CSS: &str = include_str!("../../gtl-web/assets/tailwind.css");

const ARTIFACT_RUNTIME: &str = include_str!("embedded/generated/artifact-runtime.js");
const ARTIFACT_WASM: &[u8] = include_bytes!("embedded/generated/artifact-runtime.wasm");
const WASM_DATA_URL_MARKER: &str = "__GTL_ARTIFACT_WASM_DATA_URL__";

pub(crate) fn inline_runtime() -> Result<String> {
    ensure!(
        ARTIFACT_RUNTIME.matches(WASM_DATA_URL_MARKER).count() == 1,
        "generated artifact runtime must contain exactly one WASM data URL marker"
    );
    ensure!(
        !ARTIFACT_RUNTIME.contains("</script"),
        "generated artifact runtime cannot be embedded safely"
    );

    let data_url = format!(
        "data:application/wasm;base64,{}",
        STANDARD.encode(ARTIFACT_WASM)
    );
    Ok(ARTIFACT_RUNTIME.replace(WASM_DATA_URL_MARKER, &data_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_runtime_has_one_offline_wasm_entrypoint() {
        let runtime = inline_runtime().expect("generated runtime is structurally valid");

        assert_eq!(runtime.matches("data:application/wasm;base64,").count(), 1);
        assert!(!runtime.contains(WASM_DATA_URL_MARKER));
        assert!(!runtime.contains("/./assets/"));
    }
}
