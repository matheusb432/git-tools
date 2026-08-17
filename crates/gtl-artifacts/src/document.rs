use std::fmt::Write as _;

use anyhow::{Context as _, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtl_application::{
    diffs::View,
    viewer::{RenderOptions, Theme},
};
use gtl_parser::select_bundled_syntax_pack;
use gtl_wire::viewer::{
    VIEWER_ARTIFACT_MANIFEST_ID, VIEWER_ARTIFACT_RUNTIME_ID, VIEWER_ARTIFACT_SYNTAX_ID,
};
use sha2::{Digest as _, Sha256};

use crate::{assets, compression, payload::project_payload};

struct PackedAsset {
    id: String,
    uncompressed_bytes: usize,
    encoded: String,
}

impl PackedAsset {
    fn binary(id: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let compressed = compression::gzip(bytes)?;
        Ok(Self {
            id: id.into(),
            uncompressed_bytes: bytes.len(),
            encoded: STANDARD.encode(compressed),
        })
    }

    fn json(id: impl Into<String>, payload: &(impl serde::Serialize + ?Sized)) -> Result<Self> {
        let bytes = serde_json::to_vec(payload).context("serialize artifact payload")?;
        Self::binary(id, &bytes)
    }
}

fn content_security_policy(runtime: &str, stylesheet: &str) -> String {
    let runtime_sha256 = STANDARD.encode(Sha256::digest(runtime.as_bytes()));
    let stylesheet_sha256 = STANDARD.encode(Sha256::digest(stylesheet.as_bytes()));
    // Dioxus Web 0.7.10 requires string evaluation, and diff rows set one CSS custom property
    // inline. Every script and stylesheet element still requires its exact generated hash.
    format!(
        "default-src 'none'; base-uri 'none'; connect-src 'none'; form-action 'none'; frame-src 'none'; object-src 'none'; script-src 'sha256-{runtime_sha256}' 'unsafe-eval' 'wasm-unsafe-eval'; script-src-attr 'none'; style-src 'sha256-{stylesheet_sha256}'; style-src-attr 'unsafe-inline'; worker-src 'none'"
    )
}

/// Builds one self-contained, client-rendered diff artifact.
pub fn build_html(view: &View, options: RenderOptions, theme: Option<Theme>) -> Result<String> {
    let count = view.commits.len();
    let suffix = if count == 1 { "" } else { "s" };
    let title = format!(
        "{} — {} · {count} commit{suffix}",
        view.repo_name, view.title
    );
    build_document(&title, std::slice::from_ref(view), options, theme)
}

/// Builds one self-contained artifact with a client-rendered tab per diff view.
pub fn build_tabbed_html(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
) -> Result<String> {
    build_document(title, views, options, theme)
}

fn build_document(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
) -> Result<String> {
    let payload = project_payload(title, views, options, theme)?;
    let runtime = assets::inline_runtime()?;
    let syntax_paths = views
        .iter()
        .flat_map(|view| view.files.iter())
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let syntax_pack = select_bundled_syntax_pack(syntax_paths.iter().map(String::as_str))
        .context("select artifact syntax grammars")?;
    let runtime_asset = PackedAsset::binary(VIEWER_ARTIFACT_RUNTIME_ID, assets::wasm())?;
    let syntax_asset = PackedAsset::binary(VIEWER_ARTIFACT_SYNTAX_ID, syntax_pack.as_bytes())?;
    let manifest_asset = PackedAsset::json(VIEWER_ARTIFACT_MANIFEST_ID, &payload.manifest)?;
    let page_assets = payload
        .pages
        .iter()
        .map(|page| PackedAsset::json(page.id.as_str(), page))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !assets::TAILWIND_CSS.contains("</style"),
        "generated Tailwind CSS cannot be embedded safely"
    );
    let content_security_policy = content_security_policy(runtime, assets::TAILWIND_CSS);

    let mut html = String::with_capacity(
        assets::TAILWIND_CSS.len()
            + runtime.len()
            + content_security_policy.len()
            + runtime_asset.encoded.len()
            + syntax_asset.encoded.len()
            + manifest_asset.encoded.len()
            + page_assets
                .iter()
                .map(|asset| asset.encoded.len())
                .sum::<usize>(),
    );
    write!(
        html,
        "<!doctype html><html lang=\"en\" data-theme=\"{}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{}\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark light\"><meta name=\"darkreader-lock\"><title>git-tools diff</title><style>{}</style></head><body><div id=\"main\"></div>",
        payload.manifest.theme.as_str(),
        content_security_policy,
        assets::TAILWIND_CSS,
    )
    .context("write artifact document shell")?;
    write_asset_node(&mut html, &runtime_asset)?;
    write_asset_node(&mut html, &manifest_asset)?;
    write_asset_node(&mut html, &syntax_asset)?;
    for page in &page_assets {
        write_asset_node(&mut html, page)?;
    }
    write!(
        html,
        "<script type=\"module\">{runtime}</script></body></html>"
    )
    .context("write artifact runtime")?;
    Ok(html)
}

fn write_asset_node(html: &mut String, asset: &PackedAsset) -> Result<()> {
    write!(
        html,
        "<script id=\"{}\" type=\"application/octet-stream\" data-encoding=\"base64\" data-compression=\"gzip\" data-uncompressed-bytes=\"{}\">{}</script>",
        asset.id, asset.uncompressed_bytes, asset.encoded,
    )
    .context("write compressed artifact asset")
}

#[cfg(test)]
mod tests {
    use gtl_application::diffs::FileDiff;
    use gtl_models::diffs::DiffLineCount;
    use gtl_wire::viewer::{ViewerArtifactManifest, ViewerArtifactPage};

    use super::*;
    use crate::tests::{decode_payload, has_disallowed_external_url, sample_view};

    const TINY_BASELINE_BYTES: usize = 4_320_408;
    const LARGE_BASELINE_BYTES: usize = 5_801_973;

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    struct AssetBytes {
        raw: usize,
        gzip: usize,
        base64: usize,
    }

    impl std::ops::AddAssign for AssetBytes {
        fn add_assign(&mut self, other: Self) {
            self.raw += other.raw;
            self.gzip += other.gzip;
            self.base64 += other.base64;
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ArtifactSizeEvidence {
        html: usize,
        runtime_javascript: usize,
        tailwind_css: usize,
        wasm: AssetBytes,
        payload: AssetBytes,
        syntax_pack: AssetBytes,
        pages: usize,
        lines: usize,
    }

    #[test]
    fn document_contains_only_inert_data_and_inline_client_assets() {
        let html = build_html(&sample_view(), RenderOptions::DEFAULT, Some(Theme::Dark))
            .expect("build client-rendered artifact");
        let manifest: ViewerArtifactManifest = decode_payload(&html, VIEWER_ARTIFACT_MANIFEST_ID);
        let active = &manifest.views[0];
        let first_page_id = gtl_wire::viewer::ViewerArtifactPageId::for_request(
            &gtl_wire::viewer::LoadViewerDiffLines {
                identity: active.identity,
                file: active.files[0].id.clone(),
                cursor: gtl_wire::viewer::ViewerDiffCursor::default(),
            },
        );
        let page: ViewerArtifactPage = decode_payload(&html, first_page_id.as_str());
        let policy = html
            .split_once("<meta http-equiv=\"Content-Security-Policy\" content=\"")
            .and_then(|(_, tail)| tail.split_once("\">"))
            .map(|(policy, _)| policy)
            .expect("artifact content security policy");

        assert!(html.starts_with("<!doctype html>"));
        assert!(
            html.find("Content-Security-Policy") < html.find("<style>"),
            "content security policy must precede active content"
        );
        for directive in [
            "default-src 'none'",
            "base-uri 'none'",
            "connect-src 'none'",
            "form-action 'none'",
            "frame-src 'none'",
            "object-src 'none'",
            "script-src 'sha256-",
            "script-src-attr 'none'",
            "style-src 'sha256-",
            "style-src-attr 'unsafe-inline'",
            "worker-src 'none'",
        ] {
            assert!(
                policy.contains(directive),
                "missing CSP directive: {directive}"
            );
        }
        let runtime_sha256 = STANDARD.encode(Sha256::digest(
            assets::inline_runtime()
                .expect("generated runtime")
                .as_bytes(),
        ));
        let stylesheet_sha256 = STANDARD.encode(Sha256::digest(assets::TAILWIND_CSS.as_bytes()));
        assert!(policy.contains(&format!("script-src 'sha256-{runtime_sha256}'")));
        assert!(policy.contains("'unsafe-eval' 'wasm-unsafe-eval'"));
        assert!(policy.contains(&format!("style-src 'sha256-{stylesheet_sha256}'")));
        assert!(html.contains("<style>"));
        assert_eq!(html.matches("<script type=\"module\">").count(), 1);
        assert_eq!(
            html.matches(&format!("id=\"{VIEWER_ARTIFACT_RUNTIME_ID}\""))
                .count(),
            1
        );
        assert!(html.contains("data-compression=\"gzip\""));
        assert!(!html.contains("data:application/wasm"));
        assert!(!html.contains("+client-rendered"));
        assert!(!has_disallowed_external_url(&html));
        assert_eq!(manifest.title, "api — diff · 0 commits");
        assert_eq!(page.page.lines[1], "+client-rendered");
    }

    #[test]
    fn user_content_with_script_terminators_remains_encoded() {
        let mut view = sample_view();
        view.files[0]
            .lines
            .push("+</script><script>alert('unsafe')</script>".to_owned());
        view.files[0].full_lines = Some(view.files[0].lines.clone());

        let html = build_html(&view, RenderOptions::DEFAULT, None).expect("build safe artifact");
        let manifest: ViewerArtifactManifest = decode_payload(&html, VIEWER_ARTIFACT_MANIFEST_ID);
        let page_id = gtl_wire::viewer::ViewerArtifactPageId::for_request(
            &gtl_wire::viewer::LoadViewerDiffLines {
                identity: manifest.views[0].identity,
                file: manifest.views[0].files[0].id.clone(),
                cursor: gtl_wire::viewer::ViewerDiffCursor::default(),
            },
        );
        let page: ViewerArtifactPage = decode_payload(&html, page_id.as_str());

        assert!(!html.contains("alert('unsafe')"));
        assert!(
            page.page
                .lines
                .iter()
                .any(|line| line.contains("alert('unsafe')"))
        );
    }

    #[test]
    fn tiny_artifact_has_a_deterministic_sub_2_5_mb_size_floor() {
        let view = tiny_size_view();
        let first = build_html(&view, RenderOptions::DEFAULT, Some(Theme::Dark))
            .expect("build tiny artifact");
        let second = build_html(&view, RenderOptions::DEFAULT, Some(Theme::Dark))
            .expect("rebuild tiny artifact");
        let evidence = size_evidence(std::slice::from_ref(&view), &first);

        eprintln!("tiny artifact size evidence: {evidence:?}");
        assert_eq!(
            first, second,
            "identical inputs must produce identical HTML"
        );
        assert!(evidence.html <= 2_500_000, "{evidence:?}");
        assert!(is_reduced_by_at_least(
            TINY_BASELINE_BYTES,
            evidence.html,
            40
        ));
        assert!(evidence.wasm.raw <= 2_100_000, "{evidence:?}");
        assert!(evidence.wasm.gzip <= 800_000, "{evidence:?}");
        assert!(evidence.syntax_pack.raw <= 128 * 1024, "{evidence:?}");
        assert_eq!(evidence.lines, 12);
        assert_eq!(evidence.pages, 1);
    }

    #[test]
    fn independently_compressed_large_fixture_stays_below_3_mb() {
        let view = large_size_view();
        let html = build_html(&view, RenderOptions::DEFAULT, Some(Theme::Dark))
            .expect("build representative large artifact");
        let evidence = size_evidence(std::slice::from_ref(&view), &html);

        eprintln!("large artifact size evidence: {evidence:?}");
        assert!(evidence.lines > 20_000, "{evidence:?}");
        assert!(evidence.pages > 200, "{evidence:?}");
        assert!(evidence.html <= 3_000_000, "{evidence:?}");
        assert!(is_reduced_by_at_least(
            LARGE_BASELINE_BYTES,
            evidence.html,
            45
        ));
        assert_eq!(
            html.matches("id=\"gtl-artifact-page-").count(),
            evidence.pages
        );
    }

    #[test]
    fn tabbed_document_assigns_distinct_view_and_page_addresses() {
        let view = sample_view();
        let html = build_tabbed_html(
            "subrepo diff",
            &[view.clone(), view],
            RenderOptions::DEFAULT,
            None,
        )
        .expect("build tabbed client-rendered artifact");
        let manifest: ViewerArtifactManifest = decode_payload(&html, VIEWER_ARTIFACT_MANIFEST_ID);

        assert_eq!(manifest.views.len(), 2);
        assert_ne!(
            manifest.views[0].identity.tab_id,
            manifest.views[1].identity.tab_id
        );
        for active in &manifest.views {
            let id = gtl_wire::viewer::ViewerArtifactPageId::for_request(
                &gtl_wire::viewer::LoadViewerDiffLines {
                    identity: active.identity,
                    file: active.files[0].id.clone(),
                    cursor: gtl_wire::viewer::ViewerDiffCursor::default(),
                },
            );
            let page: ViewerArtifactPage = decode_payload(&html, id.as_str());
            assert_eq!(page.page.identity, active.identity);
        }
    }

    fn tiny_size_view() -> View {
        let mut view = sample_view();
        let mut lines = vec!["@@ -1,5 +1,6 @@".to_owned()];
        for index in 0..11 {
            lines.push(format!(
                "+pub fn compact_fixture_{index}() -> usize {{ {index} * 17 }}"
            ));
        }
        view.files[0].added = DiffLineCount::new(11);
        view.files[0].removed = DiffLineCount::default();
        view.files[0].lines.clone_from(&lines);
        view.files[0].full_lines = Some(lines);
        view
    }

    fn large_size_view() -> View {
        let mut view = sample_view();
        view.files = (0..205)
            .map(|file_index| {
                let mut lines = vec!["@@ -1,100 +1,100 @@".to_owned()];
                lines.extend((0..100).map(|line_index| {
                    format!(
                        "+pub fn fixture_{file_index}_{line_index}() -> usize {{ {file_index} + {line_index} }}"
                    )
                }));
                FileDiff {
                    path: gtl_models::paths::RepositoryRelativePath::try_new(
                        format!("src/generated_{file_index}.rs").into(),
                    )
                    .unwrap(),
                    added: DiffLineCount::new(100),
                    removed: DiffLineCount::default(),
                    full_lines: Some(lines.clone()),
                    lines,
                }
            })
            .collect();
        view
    }

    fn size_evidence(views: &[View], html: &str) -> ArtifactSizeEvidence {
        let payload = project_payload(
            "size fixture",
            views,
            RenderOptions::DEFAULT,
            Some(Theme::Dark),
        )
        .expect("project size fixture payload");
        let mut payload_bytes = measure_asset(html, VIEWER_ARTIFACT_MANIFEST_ID);
        for page in &payload.pages {
            payload_bytes += measure_asset(html, page.id.as_str());
        }
        let lines = payload.pages.iter().map(|page| page.page.lines.len()).sum();
        ArtifactSizeEvidence {
            html: html.len(),
            runtime_javascript: assets::inline_runtime().expect("generated runtime").len(),
            tailwind_css: assets::TAILWIND_CSS.len(),
            wasm: measure_asset(html, VIEWER_ARTIFACT_RUNTIME_ID),
            payload: payload_bytes,
            syntax_pack: measure_asset(html, VIEWER_ARTIFACT_SYNTAX_ID),
            pages: payload.pages.len(),
            lines,
        }
    }

    fn measure_asset(html: &str, id: &str) -> AssetBytes {
        let marker = format!("id=\"{id}\"");
        let (_, tail) = html.split_once(&marker).expect("artifact asset address");
        let (attributes, tail) = tail.split_once('>').expect("artifact asset opening tag");
        let (encoded, _) = tail
            .split_once("</script>")
            .expect("artifact asset closing tag");
        let raw_marker = "data-uncompressed-bytes=\"";
        let raw = attributes
            .split_once(raw_marker)
            .and_then(|(_, tail)| tail.split_once('"'))
            .and_then(|(value, _)| value.parse::<usize>().ok())
            .expect("artifact uncompressed byte count");
        let gzip = STANDARD.decode(encoded).expect("artifact base64 bytes");
        let decoded = compression::gunzip(&gzip).expect("artifact gzip bytes");

        assert_eq!(decoded.len(), raw, "decoded byte metadata for {id}");
        AssetBytes {
            raw,
            gzip: gzip.len(),
            base64: encoded.len(),
        }
    }

    fn is_reduced_by_at_least(baseline: usize, current: usize, percentage: usize) -> bool {
        baseline
            .saturating_sub(current)
            .checked_mul(100)
            .is_some_and(|reduction| reduction >= baseline.saturating_mul(percentage))
    }
}
