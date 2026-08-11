use anyhow::{Result, ensure};

pub(crate) const TAILWIND_CSS: &str = include_str!("../../gtl-web/assets/tailwind.css");

const ARTIFACT_RUNTIME: &str = include_str!("embedded/generated/artifact-runtime.js");
const ARTIFACT_WASM: &[u8] = include_bytes!("embedded/generated/artifact-runtime.wasm");

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

pub(crate) const fn wasm() -> &'static [u8] {
    ARTIFACT_WASM
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
        assert!(ARTIFACT_WASM.len() <= 2_100_000);

        let universal_pack = include_bytes!("../../gtl-parser/assets/syntaxes.packdump");
        assert!(
            !ARTIFACT_WASM
                .windows(universal_pack.len())
                .any(|window| window == universal_pack),
            "artifact WASM retained the universal syntax pack"
        );
    }
}
