use std::{collections::HashSet, io::Write as _};

use flate2::{Compression, GzBuilder};
use gtl_application::diffs::FileDiff;
use gtl_models::{
    diffs::{
        AppliedExtensionFilter, DiffLineCount, ExtensionFilter, ExtensionFilterMode, FileExtensions,
    },
    viewer::{DiffDensity, DiffLayout},
};

use super::*;
use crate::tests::sample_view;

#[derive(Debug)]
struct ArtifactSizeEvidence {
    html: usize,
    gzip: usize,
    stylesheet: usize,
    files: usize,
    rows: usize,
}

#[test]
fn document_is_complete_without_scripts_or_external_assets() {
    for theme in [
        None,
        Some(Theme::Dark),
        Some(Theme::Mirage),
        Some(Theme::Glacier),
        Some(Theme::Graphite),
        Some(Theme::Carbon),
    ] {
        let html = build_html(
            &sample_view(),
            RenderOptions::DEFAULT,
            theme,
            ViewerLanguage::EnUs,
        )
        .unwrap();
        assert!(html.starts_with(&format!(
            "<!doctype html><html lang=\"en-US\" data-theme=\"{}\">",
            theme.unwrap_or_default()
        )));
        assert!(html.contains("<title>api - diff · 0 commits</title>"));
        assert!(html.contains("static_rendered"));
        assert_eq!(html.matches("<style>").count(), 1);
        assert!(!stylesheet(&html).to_ascii_lowercase().contains("</style"));
        assert!(html.contains(&format!(
            "content=\"{}\"",
            content_security_policy(stylesheet(&html))
        )));
        for forbidden in [
            "<script",
            " onclick=",
            " onerror=",
            " style=",
            " src=",
            " srcset=",
            "url(",
            "@import",
            "data-dioxus",
            ".wasm",
            "unsafe-inline",
            "unsafe-eval",
        ] {
            assert!(
                !html.contains(forbidden),
                "unexpected active content: {forbidden}"
            );
        }
        assert!(
            attribute_values(&html, " href=\"")
                .iter()
                .all(|link| link.starts_with('#'))
        );
    }
}

#[test]
fn repository_content_is_escaped_in_text_and_attributes() {
    let mut view = sample_view();
    view.repo_name =
        gtl_models::paths::ProjectName::try_new("<script>repository</script>").unwrap();
    view.files[0].path = gtl_models::paths::RepositoryRelativePath::try_new(
        "src/\" onmouseover=\"attack&<file>.rs".into(),
    )
    .unwrap();
    view.files[0].lines = ["@@ -1 +1 @@", "+<img src=x onerror=attack()></script>"]
        .into_iter()
        .collect();
    view.files[0].full_lines = None;
    let html = build_tabbed_html(
        "</title><script>head_attack()</script>",
        &[view],
        RenderOptions::DEFAULT,
        None,
        ViewerLanguage::EnUs,
    )
    .unwrap();
    assert!(!html.contains("<script"));
    assert!(!html.contains("<img"));
    assert!(!html.contains("\" onmouseover="));
    assert!(html.contains("&lt;/title&gt;&lt;script&gt;head_attack()&lt;/script&gt;"));
    assert!(html.contains("&quot; onmouseover=&quot;attack&amp;&lt;file&gt;.rs"));
    assert!(html.contains("&lt;script&gt;repository&lt;/script&gt;"));
}

#[test]
fn long_lines_have_a_unicode_safe_preview_and_complete_collapsed_content() {
    let mut view = sample_view();
    let body = format!("{}<complete-tail>", "é🦀".repeat(125_000));
    view.files[0].lines = vec!["@@ -1 +1 @@".into(), format!("+{body}")].into();
    view.files[0].full_lines = None;
    let html = build_html(&view, RenderOptions::DEFAULT, None, ViewerLanguage::EnUs).unwrap();
    assert!(html.contains("<details class=\"long-line\"><summary><code>"));
    assert!(html.contains(&format!("{}…</code>", "é🦀".repeat(250))));
    assert!(html.contains(&format!(
        "<code class=\"line-full\">{}</code>",
        Escaped(&body)
    )));
    assert!(html.contains("Show complete line"));
}

#[test]
fn unified_layout_preserves_syntax_density_wrapping_and_theme() {
    let mut view = sample_view();
    view.files[0].lines = ["@@ -1 +1 @@", "+pub fn changed() -> usize { 42 }"]
        .into_iter()
        .collect();
    view.files[0].full_lines = Some(
        [
            "@@ -1 +1,2 @@",
            "+pub fn changed() -> usize { 42 }",
            " full_context_marker",
        ]
        .into_iter()
        .collect(),
    );
    let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full).with_wrap_lines(true);
    let html = build_html(&view, options, Some(Theme::Carbon), ViewerLanguage::EnUs).unwrap();
    let unified = build_html(
        &view,
        RenderOptions::new(DiffLayout::Unified, DiffDensity::Full).with_wrap_lines(true),
        Some(Theme::Carbon),
        ViewerLanguage::EnUs,
    )
    .unwrap();
    assert_eq!(html, unified, "viewer layout must not change the artifact");
    assert!(html.contains("data-theme=\"carbon\""));
    assert!(html.contains("<main class=\"wrap\">"));
    assert!(html.contains("class=\"keyword\""));
    assert!(html.contains("full_context_marker"));
    let compact = build_html(&view, RenderOptions::DEFAULT, None, ViewerLanguage::EnUs).unwrap();
    assert!(!compact.contains("full_context_marker"));
}

#[test]
fn every_repository_is_reachable_and_file_anchors_are_unique() {
    let first = sample_view();
    let mut second = sample_view();
    second.repo_name = gtl_models::paths::ProjectName::try_from("worker").unwrap();
    let html = build_tabbed_html(
        "Repositories",
        &[first, second],
        RenderOptions::DEFAULT,
        None,
        ViewerLanguage::EnUs,
    )
    .unwrap();
    let ids = attribute_values(&html, " id=\"");
    let seen = ids.iter().copied().collect::<HashSet<_>>();
    assert_eq!(seen.len(), ids.len());
    for href in attribute_values(&html, " href=\"") {
        assert!(seen.contains(&href[1..]), "missing link target: {href}");
    }
    assert_eq!(html.matches("class=\"repository\"").count(), 2);
    assert_eq!(html.matches("<details class=\"file\"").count(), 2);
    assert!(!html.contains(" hidden"));
    assert!(html.contains("worker"));
}

#[test]
fn document_localizes_copy_and_discloses_filtered_files() {
    let mut view = sample_view();
    view.extension_filter = Some(AppliedExtensionFilter {
        filter: ExtensionFilter::new(ExtensionFilterMode::Hide, FileExtensions::new(["lock"])),
        hidden_paths: vec![
            gtl_models::paths::RepositoryRelativePath::try_new("Cargo.lock".into()).unwrap(),
        ],
    });
    let html = build_html(&view, RenderOptions::DEFAULT, None, ViewerLanguage::PtBr).unwrap();
    assert!(html.contains("lang=\"pt-BR\""));
    assert!(html.contains("<title>api - diff · nenhum commit</title>"));
    assert!(html.contains("Arquivos alterados"));
    assert!(html.contains("Arquivos ocultos pelo filtro de extensões salvo: 1 (ocultar lock)"));
    assert!(html.contains("Cargo.lock"));
}

#[test]
fn tiny_artifact_is_deterministic_and_records_size_evidence() {
    let view = tiny_size_view();
    let html = build_html(
        &view,
        RenderOptions::DEFAULT,
        Some(Theme::Dark),
        ViewerLanguage::EnUs,
    )
    .unwrap();
    assert_eq!(
        html,
        build_html(
            &view,
            RenderOptions::DEFAULT,
            Some(Theme::Dark),
            ViewerLanguage::EnUs
        )
        .unwrap()
    );
    let evidence = size_evidence(&html);
    eprintln!("tiny static artifact size evidence: {evidence:?}");
    assert_eq!(evidence.files, 1);
    assert_eq!(evidence.rows, 12);
    assert!(evidence.stylesheet < 10_000, "{evidence:?}");
    assert!(evidence.html < 25_000, "{evidence:?}");
    assert!(evidence.gzip < 5_000, "{evidence:?}");
}

#[test]
fn representative_large_artifact_records_complete_document_size() {
    let html = build_html(
        &large_size_view(),
        RenderOptions::DEFAULT,
        Some(Theme::Dark),
        ViewerLanguage::EnUs,
    )
    .unwrap();
    let evidence = size_evidence(&html);
    eprintln!("large static artifact size evidence: {evidence:?}");
    assert_eq!(evidence.files, 205);
    assert_eq!(evidence.rows, 20_705);
    assert!(evidence.html < 12_000_000, "{evidence:?}");
    assert!(evidence.gzip < 350_000, "{evidence:?}");
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
        stylesheet: stylesheet(html).len(),

        files: html.matches("<details class=\"file\"").count(),
        rows: html.matches("<tr class=").count(),
    }
}

fn attribute_values<'html>(html: &'html str, prefix: &str) -> Vec<&'html str> {
    html.split(prefix)
        .skip(1)
        .filter_map(|tail| tail.split_once('"').map(|(value, _)| value))
        .collect()
}

fn stylesheet(html: &str) -> &str {
    html.split_once("<style>")
        .unwrap()
        .1
        .split_once("</style>")
        .unwrap()
        .0
}
