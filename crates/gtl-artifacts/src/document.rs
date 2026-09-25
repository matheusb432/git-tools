use std::fmt::Write as _;

use anyhow::{Context as _, Result, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtl_application::{
    diffs::View,
    viewer::{RenderOptions, Theme},
};
use gtl_models::settings::ViewerLanguage;
use gtl_web::{
    StaticArtifactView, render_static_artifact_body, static_artifact_document_title,
    static_artifact_enhancement_script,
};
use sha2::{Digest as _, Sha256};

use crate::{assets, payload::project_payload};

fn content_security_policy(script: &str, stylesheet: &str) -> String {
    let script_sha256 = STANDARD.encode(Sha256::digest(script.as_bytes()));
    let stylesheet_sha256 = STANDARD.encode(Sha256::digest(stylesheet.as_bytes()));
    format!(
        "default-src 'none'; base-uri 'none'; connect-src 'none'; form-action 'none'; frame-src 'none'; object-src 'none'; script-src 'sha256-{script_sha256}'; script-src-attr 'none'; style-src 'sha256-{stylesheet_sha256}'; style-src-attr 'unsafe-inline'; worker-src 'none'"
    )
}

/// Builds one self-contained static diff artifact with its copy in `language`.
pub fn build_html(
    view: &View,
    options: RenderOptions,
    theme: Option<Theme>,
    language: ViewerLanguage,
) -> Result<String> {
    let title =
        static_artifact_document_title(&view.repo_name, &view.title, view.commits.len(), language);
    build_document(&title, std::slice::from_ref(view), options, theme, language)
}

/// Builds one self-contained static artifact with a tab per diff view and its copy in `language`.
pub fn build_tabbed_html(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
    language: ViewerLanguage,
) -> Result<String> {
    build_document(title, views, options, theme, language)
}

fn build_document(
    title: &str,
    views: &[View],
    options: RenderOptions,
    theme: Option<Theme>,
    language: ViewerLanguage,
) -> Result<String> {
    let payload = project_payload(views, options, theme)?;
    let static_views = payload
        .views
        .into_iter()
        .map(|projected| StaticArtifactView::try_new(projected.view, projected.rows))
        .collect::<Result<Vec<_>, _>>()
        .context("construct static artifact views")?;
    let body = render_static_artifact_body(static_views, language);
    let script = static_artifact_enhancement_script();
    ensure!(
        !assets::TAILWIND_CSS
            .to_ascii_lowercase()
            .contains("</style"),
        "generated Tailwind CSS cannot be embedded safely"
    );
    ensure!(
        !script.to_ascii_lowercase().contains("</script"),
        "artifact enhancement script cannot be embedded safely"
    );
    let content_security_policy = content_security_policy(script, assets::TAILWIND_CSS);

    let mut html = String::with_capacity(
        body.len()
            + assets::TAILWIND_CSS.len()
            + script.len()
            + content_security_policy.len()
            + title.len()
            + 512,
    );
    write!(
        html,
        "<!doctype html><html lang=\"{}\" data-theme=\"{}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{}\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark\"><meta name=\"darkreader-lock\"><title>",
        language.as_str(),
        payload.theme.as_str(),
        content_security_policy,
    )
    .context("write artifact document head")?;
    write_escaped_text(&mut html, title).context("write artifact title")?;
    write!(
        html,
        "</title><style>{}</style></head><body>{body}<script>{script}</script></body></html>",
        assets::TAILWIND_CSS,
    )
    .context("write static artifact document")?;
    Ok(html)
}

fn write_escaped_text(output: &mut String, value: &str) -> std::fmt::Result {
    for character in value.chars() {
        match character {
            '&' => output.write_str("&amp;")?,
            '<' => output.write_str("&lt;")?,
            '>' => output.write_str("&gt;")?,
            _ => output.write_char(character)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, io::Write as _};

    use flate2::{Compression, GzBuilder};
    use gtl_application::{
        diffs::FileDiff,
        viewer::{DiffDensity, DiffLayout},
    };
    use gtl_models::diffs::DiffLineCount;

    use super::*;
    use crate::tests::{has_disallowed_external_url, sample_view};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ArtifactSizeEvidence {
        html: usize,
        gzip: usize,
        tailwind_css: usize,
        enhancement_javascript: usize,
        files: usize,
        rows: usize,
    }

    #[test]
    fn document_declares_its_language_and_renders_copy_in_it() {
        let html = build_html(
            &sample_view(),
            RenderOptions::DEFAULT,
            None,
            ViewerLanguage::PtBr,
        )
        .unwrap();

        assert!(html.starts_with("<!doctype html><html lang=\"pt-BR\""));
        assert!(html.contains("<title>api - diff · nenhum commit</title>"));
        assert!(html.contains("data-gtl-label-copied=\"Copiado\""));
        assert!(html.contains(
            "data-gtl-label-copied-context-lines=\"Copiado com contexto - linhas {lines}\""
        ));
    }

    #[test]
    fn document_contains_complete_static_markup() {
        let html = build_html(
            &sample_view(),
            RenderOptions::DEFAULT,
            None,
            ViewerLanguage::EnUs,
        )
        .unwrap();
        let policy =
            content_security_policy(static_artifact_enhancement_script(), assets::TAILWIND_CSS);

        assert!(html.starts_with("<!doctype html><html lang=\"en-US\" data-theme=\"dark\">"));
        assert!(html.contains("<title>api - diff · 0 commits</title>"));
        assert!(html.contains("static_rendered"));
        assert!(html.contains("data-gtl-diff-file"));
        assert!(html.contains("data-gtl-diff-row"));
        assert_eq!(html.matches("<style>").count(), 1);
        assert_eq!(html.matches("<script>").count(), 1);
        assert_eq!(html.matches("</script>").count(), 1);
        assert!(html.contains(&format!("content=\"{policy}\"")));
        for retired in [
            "application/octet-stream",
            "data-compression",
            "gtl-artifact-manifest",
            "gtl-artifact-page-",
            "gtl-artifact-runtime",
            "WebAssembly",
            "DecompressionStream",
            ".wasm",
            "unsafe-eval",
            "wasm-unsafe-eval",
            "type=\"module\"",
            "<!--node-id",
            "data-dioxus",
            " onclick=",
            " oninput=",
            " onkeydown=",
        ] {
            assert!(!html.contains(retired), "retained retired asset: {retired}");
        }
        let markup = html.replace(static_artifact_enhancement_script(), "");
        assert!(!has_disallowed_external_url(&markup));
    }

    #[test]
    fn user_content_is_literal_and_escaped() {
        let mut view = sample_view();
        view.title = gtl_models::diffs::DiffViewTitle::Named {
            name: gtl_models::paths::ProjectName::try_new(
                "diff </title><script>title_attack()</script>",
            )
            .unwrap(),
        };
        view.files[0].path = gtl_models::paths::RepositoryRelativePath::try_new(
            "src/<script>path_attack()</script>.rs".into(),
        )
        .unwrap();
        view.files[0].lines = [
            "@@ -1 +1,2 @@",
            "+static_rendered",
            "+<img src=x onerror=line_attack()></script>",
        ]
        .into_iter()
        .collect();
        view.files[0].full_lines = Some(view.files[0].lines.clone());

        let html = build_tabbed_html(
            "artifact </title><script>head_attack()</script>",
            &[view],
            RenderOptions::DEFAULT,
            None,
            ViewerLanguage::EnUs,
        )
        .unwrap();

        assert_eq!(html.matches("<script>").count(), 1);
        assert!(!html.contains("<script>head_attack()"));
        assert!(!html.contains("<script>title_attack()"));
        assert!(!html.contains("<script>path_attack()"));
        assert!(!html.contains("<img src=x"));
        assert!(html.contains("&lt;/title&gt;&lt;script&gt;head_attack()&lt;/script&gt;"));
        assert!(html.contains("path_attack()"));
        assert!(html.contains("line_attack"));
    }

    #[test]
    fn static_document_preserves_typed_rendering() {
        let mut view = sample_view();
        let long_body = "x".repeat(2_001);
        view.files[0].lines = vec![
            "@@ -1,2 +1,3 @@".to_owned(),
            "-pub fn old() {}".to_owned(),
            "+pub fn current() -> usize { 42 }".to_owned(),
            format!("+{long_body}"),
        ]
        .into();
        view.files[0].full_lines = Some(view.files[0].lines.clone());

        let unified = build_html(
            &view,
            RenderOptions::new(DiffLayout::Unified, DiffDensity::Compact),
            Some(Theme::Carbon),
            ViewerLanguage::EnUs,
        )
        .unwrap();
        let split = build_html(
            &view,
            RenderOptions::new(DiffLayout::Split, DiffDensity::Compact).with_wrap_lines(true),
            Some(Theme::Carbon),
            ViewerLanguage::EnUs,
        )
        .unwrap();

        assert!(unified.contains("data-wrap-lines=\"false\""));
        assert!(split.contains("data-wrap-lines=\"true\""));
        assert!(unified.contains("data-layout=\"unified\""));
        assert!(unified.contains("text-[var(--sy-kw)]"));
        for html in [&unified, &split] {
            assert!(html.contains("data-theme=\"carbon\""));
            assert!(html.contains("(+1501 characters omitted)"));
            assert!(html.contains(&"x".repeat(500)));
            assert!(!html.contains(&long_body));
            assert!(!html.contains("toggle-long-line"));
        }
        assert!(split.contains("data-layout=\"split\""));
        assert!(split.contains("class=\"diff-row-split\""));
        assert!(split.contains("grid-template-columns:44px minmax(0,1fr) 44px minmax(0,1fr)"));
        assert!(split.contains("current"));
        assert!(split.contains(">2</span>"));
    }

    #[test]
    fn tabbed_document_contains_every_view() {
        let first = sample_view();
        let mut second = sample_view();
        second.repo_name = gtl_models::paths::ProjectName::try_from("worker").unwrap();
        second.files[0].lines = ["@@ -1 +1 @@", "+second-view-marker"].into_iter().collect();
        second.files[0].full_lines = Some(second.files[0].lines.clone());

        let html = build_tabbed_html(
            "subrepo diff",
            &[first, second],
            RenderOptions::DEFAULT,
            None,
            ViewerLanguage::EnUs,
        )
        .unwrap();

        assert_eq!(html.matches("role=\"tab\"").count(), 2);
        assert_eq!(html.matches("role=\"tabpanel\"").count(), 2);
        assert_eq!(html.matches("data-gtl-artifact-panel").count(), 2);
        assert!(html.contains("api"));
        assert!(html.contains("worker"));
        assert!(html.contains("static_rendered"));
        assert!(html.contains("second-view-marker"));
        let panel_start_tags = html
            .split("<section")
            .skip(1)
            .filter_map(|tail| tail.split_once('>').map(|(start_tag, _)| start_tag))
            .filter(|start_tag| start_tag.contains("role=\"tabpanel\""))
            .collect::<Vec<_>>();
        assert_eq!(panel_start_tags.len(), 2);
        assert!(!panel_start_tags[0].contains(" hidden=true"));
        assert!(panel_start_tags[1].contains(" hidden=true"));
        let ids = attribute_values(&html, " id=\"");
        let mut seen = HashSet::new();
        let duplicate_ids = ids
            .iter()
            .copied()
            .filter(|id| !seen.insert(*id))
            .collect::<Vec<_>>();
        assert!(
            duplicate_ids.is_empty(),
            "static tab panels must not contain duplicate IDs: {duplicate_ids:?}"
        );
    }

    #[test]
    fn tiny_artifact_is_deterministic_and_records_size_evidence() {
        let view = tiny_size_view();
        let first = build_html(
            &view,
            RenderOptions::DEFAULT,
            Some(Theme::Dark),
            ViewerLanguage::EnUs,
        )
        .unwrap();
        let second = build_html(
            &view,
            RenderOptions::DEFAULT,
            Some(Theme::Dark),
            ViewerLanguage::EnUs,
        )
        .unwrap();
        let evidence = size_evidence(&first);

        eprintln!("tiny static artifact size evidence: {evidence:?}");
        assert_eq!(
            first, second,
            "identical inputs must produce identical HTML"
        );
        assert_eq!(evidence.files, 1);
        assert_eq!(evidence.rows, 12);
        assert!(evidence.html <= 120_000, "{evidence:?}");
        assert!(evidence.gzip <= 22_000, "{evidence:?}");
    }

    #[test]
    fn representative_large_artifact_records_complete_document_size() {
        let view = large_size_view();
        let html = build_html(
            &view,
            RenderOptions::DEFAULT,
            Some(Theme::Dark),
            ViewerLanguage::EnUs,
        )
        .unwrap();
        let evidence = size_evidence(&html);

        eprintln!("large static artifact size evidence: {evidence:?}");
        assert_eq!(evidence.files, 205, "{evidence:?}");
        assert_eq!(evidence.rows, 20_705, "{evidence:?}");
        assert!(evidence.html <= 30_000_000, "{evidence:?}");
        assert!(evidence.gzip <= 750_000, "{evidence:?}");
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
        view.files[0].lines = lines.into();
        view.files[0].full_lines = Some(view.files[0].lines.clone());
        view
    }

    fn large_size_view() -> View {
        let mut view = sample_view();
        view.files = (0..205).map(large_file).collect();
        view
    }

    fn large_file(file_index: usize) -> FileDiff {
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
            full_lines: Some(lines.clone().into()),
            lines: lines.into(),
        }
    }

    fn size_evidence(html: &str) -> ArtifactSizeEvidence {
        let mut encoder = GzBuilder::new()
            .mtime(0)
            .write(Vec::new(), Compression::best());
        encoder.write_all(html.as_bytes()).unwrap();
        let gzip = encoder.finish().unwrap().len();
        ArtifactSizeEvidence {
            html: html.len(),
            gzip,
            tailwind_css: assets::TAILWIND_CSS.len(),
            enhancement_javascript: static_artifact_enhancement_script().len(),
            files: html.matches("data-gtl-diff-file=\"\"").count(),
            rows: html.matches("data-gtl-diff-row=\"\"").count(),
        }
    }

    fn attribute_values<'html>(html: &'html str, prefix: &str) -> Vec<&'html str> {
        html.split(prefix)
            .skip(1)
            .filter_map(|tail| tail.split_once('"').map(|(value, _)| value))
            .collect()
    }
}
