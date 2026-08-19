use anyhow::{Result, ensure};

pub(crate) const TAILWIND_CSS: &str = include_str!("../../gtl-web/assets/tailwind.css");

const ARTIFACT_RUNTIME: &str =
    include_str!("../../../target/generated/gtl-artifacts/artifact-runtime.js");
const ARTIFACT_WASM_BASE64: &str =
    include_str!("../../../target/generated/gtl-artifacts/artifact-runtime.wasm.gz.base64");
const ARTIFACT_WASM_BYTES: &str =
    include_str!("../../../target/generated/gtl-artifacts/artifact-runtime.wasm.bytes");

pub(crate) fn inline_runtime() -> Result<&'static str> {
    ensure!(
        ARTIFACT_RUNTIME
            .matches("globalThis.__gtlLoadCompressedAsset")
            .count()
            >= 2,
        "generated artifact runtime must install and use the compressed asset loader"
    );
    ensure!(
        !ARTIFACT_RUNTIME.to_ascii_lowercase().contains("</script"),
        "generated artifact runtime cannot be embedded safely"
    );
    ensure!(
        !ARTIFACT_RUNTIME.contains("data:application/wasm"),
        "generated artifact runtime retained an uncompressed WASM data URL"
    );
    ensure!(
        !ARTIFACT_RUNTIME.contains("/./assets/") && !ARTIFACT_RUNTIME.contains("import("),
        "generated artifact runtime retained an external dependency"
    );
    Ok(ARTIFACT_RUNTIME)
}

pub(crate) fn wasm() -> Result<(&'static str, usize)> {
    ensure!(
        !ARTIFACT_WASM_BASE64.is_empty(),
        "generated artifact WASM is empty"
    );
    let uncompressed_bytes = ARTIFACT_WASM_BYTES
        .parse::<usize>()
        .map_err(anyhow::Error::from)?;
    ensure!(
        uncompressed_bytes > 0,
        "generated artifact WASM size is zero"
    );
    Ok((ARTIFACT_WASM_BASE64, uncompressed_bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_runtime_has_one_compressed_offline_wasm_entrypoint() {
        let runtime = inline_runtime().expect("generated runtime is structurally valid");

        assert_eq!(
            runtime
                .matches("globalThis.__gtlLoadCompressedAsset(\"gtl-artifact-runtime\"")
                .count(),
            1
        );
        assert!(!runtime.contains("data:application/wasm"));
        assert!(!runtime.contains("/./assets/"));
        assert!(!runtime.contains("import("));
        assert!(runtime.contains("DecompressionStream"));
        assert!(runtime.contains("__gtlShowArtifactInitializationError"));
        assert!(runtime.contains("This browser cannot decompress this offline diff artifact."));
        assert!(runtime.contains("gzip stream is corrupt"));
        let (encoded_wasm, uncompressed_bytes) = wasm().expect("generated WASM metadata");
        assert!(!encoded_wasm.is_empty());
        assert!(uncompressed_bytes > 0);
    }
}
