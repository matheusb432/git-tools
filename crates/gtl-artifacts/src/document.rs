use std::fmt::{self, Write as _};

use anyhow::{Context as _, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtl_application::diffs::{FileDiff, View};
use gtl_models::{
    diffs::{Commit, CommitIdAbbreviation, DiffLineCount},
    settings::ViewerLanguage,
    viewer::{RenderOptions, Theme},
};
use sha2::{Digest as _, Sha256};

use super::{labels::Labels, rows};

const STYLESHEET: &str = include_str!("document.css");
const BRAND_ICON: &str = "<svg aria-hidden=\"true\" viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"currentColor\"><path d=\"m7 6 6 6-6 6m6-6h5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"3\" stroke-linejoin=\"round\"/><rect x=\"2\" y=\"2\" width=\"7\" height=\"7\" rx=\"1.5\"/><rect x=\"2\" y=\"15\" width=\"7\" height=\"7\" rx=\"1.5\"/><rect x=\"15\" y=\"8.5\" width=\"7\" height=\"7\" rx=\"1.5\"/></svg>";

/// Escapes both HTML text and quoted attributes at the document boundary.
pub(super) struct Escaped<'a>(pub &'a str);

impl fmt::Display for Escaped<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        for character in self.0.chars() {
            match character {
                '&' => output.write_str("&amp;")?,
                '<' => output.write_str("&lt;")?,
                '>' => output.write_str("&gt;")?,
                '"' => output.write_str("&quot;")?,
                '\'' => output.write_str("&#39;")?,
                _ => output.write_char(character)?,
            }
        }
        Ok(())
    }
}

const fn theme_stylesheet(theme: Theme) -> &'static str {
    match theme {
        Theme::Dark => include_str!("themes/dark.css"),
        Theme::Mirage => include_str!("themes/mirage.css"),
        Theme::Glacier => include_str!("themes/glacier.css"),
        Theme::Graphite => include_str!("themes/graphite.css"),
        Theme::Carbon => include_str!("themes/carbon.css"),
    }
}

fn content_security_policy(stylesheet: &str) -> String {
    let stylesheet_sha256 = STANDARD.encode(Sha256::digest(stylesheet.as_bytes()));
    format!(
        "default-src 'none'; base-uri 'none'; connect-src 'none'; form-action 'none'; frame-src 'none'; object-src 'none'; script-src 'none'; style-src 'sha256-{stylesheet_sha256}'; style-src-attr 'none'; worker-src 'none'"
    )
}

/// Builds one self-contained unified diff document with its copy in `language`.
/// Density and wrapping are selected at generation; layout is always unified.
pub fn build_html(
    view: &View,
    options: RenderOptions,
    theme: Option<Theme>,
    language: ViewerLanguage,
) -> Result<String> {
    let labels = Labels::new(language);
    let title = format!(
        "{} - {} · {}",
        view.origin.name(),
        Labels::title(&view.title),
        labels.commit_count(view.commits.len())
    );
    build_document(&title, std::slice::from_ref(view), options, theme, language)
}

/// Builds one self-contained document with a linked section for each repository.
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
    let labels = Labels::new(language);
    let theme = theme.unwrap_or_default();
    let stylesheet = format!("{}{STYLESHEET}", theme_stylesheet(theme));
    let mut html = String::new();
    write!(html, "<!doctype html><html lang=\"{}\" data-theme=\"{theme}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{}\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark\"><meta name=\"darkreader-lock\"><title>{}</title><style>{stylesheet}</style></head><body><main class=\"{}\"><header class=\"document-header\"><span class=\"brand\" title=\"git-tools\" aria-label=\"git-tools\">{BRAND_ICON}</span><h1 title=\"{}\">{}</h1></header>", language.as_str(), content_security_policy(&stylesheet), Escaped(title), if options.wrap_lines() { "wrap" } else { "nowrap" }, Escaped(title), Escaped(title))
        .context("write artifact document head")?;
    if views.is_empty() {
        write!(html, "<p class=\"empty\">{}</p>", labels.empty)?;
    }
    if views.len() > 1 {
        write!(
            html,
            "<nav class=\"repository-index\" aria-label=\"{}\"><ul>",
            labels.repositories
        )?;
        for (index, view) in views.iter().enumerate() {
            write!(
                html,
                "<li><a href=\"#repository-{index}\">{}</a></li>",
                Escaped(view.origin.name().as_str())
            )?;
        }
        html.push_str("</ul></nav>");
    }
    for (index, view) in views.iter().enumerate() {
        render_repository(&mut html, index, view, options, &labels)
            .context("render artifact repository")?;
    }
    html.push_str("</main></body></html>");
    Ok(html)
}

fn render_repository(
    html: &mut String,
    index: usize,
    view: &View,
    options: RenderOptions,
    labels: &Labels,
) -> fmt::Result {
    write!(
        html,
        "<section id=\"repository-{index}\" class=\"repository\"><header class=\"repository-heading\"><h2>{}</h2><p class=\"range\">{}",
        Escaped(view.origin.name().as_str()),
        Escaped(&Labels::title(&view.title)),
    )?;
    if let Some(repository) = view.origin.repository() {
        write!(
            html,
            " · <code>{}</code> → <code>{}</code>",
            Escaped(&repository.branch.to_string()),
            Escaped(repository.upstream.as_str()),
        )?;
    }
    write!(
        html,
        "</p><p class=\"command\"><code>{}</code></p></header><div class=\"repository-grid\"><aside class=\"files-sidebar\"><details class=\"sidebar-content\"><summary>{} <span class=\"count\">{}</span></summary>",
        Escaped(&view.foot.cmd),
        labels.files,
        view.files.len()
    )?;
    render_file_index(html, index, &view.files, labels)?;
    if let Some(applied) = &view.extension_filter {
        write!(
            html,
            "<details class=\"filter-note\"><summary>{}: {} ({} {})</summary><ul>",
            labels.hidden_files,
            applied.hidden_paths.len(),
            labels.filter_mode(applied.filter.mode()),
            Escaped(&applied.extensions_label())
        )?;
        for path in &applied.hidden_paths {
            write!(
                html,
                "<li><code>{}</code></li>",
                Escaped(&path.to_string_lossy())
            )?;
        }
        html.push_str("</ul></details>");
    }
    html.push_str("</details></aside><aside class=\"commits-sidebar\">");
    render_commits(html, &view.commits, labels)?;
    html.push_str("</aside><div class=\"files\">");
    if view.files.is_empty() {
        write!(html, "<p class=\"empty\">{}</p>", labels.empty)?;
    } else {
        for (file_index, file) in view.files.iter().enumerate() {
            write!(
                html,
                "<details class=\"file\" id=\"repository-{index}-file-{file_index}\" open><summary><span class=\"file-path\">{}</span><span class=\"file-stats\"><span class=\"added-count\">+{}</span> <span class=\"removed-count\">−{}</span></span></summary>",
                Escaped(&file.path.to_string_lossy()),
                file.added,
                file.removed
            )?;
            rows::render(html, file, options.density(), labels)?;
            html.push_str("</details>");
        }
    }
    html.push_str("</div></div></section>");
    Ok(())
}

fn render_file_index(
    html: &mut String,
    index: usize,
    files: &[FileDiff],
    labels: &Labels,
) -> fmt::Result {
    let (added, removed) = files.iter().fold(
        (DiffLineCount::default(), DiffLineCount::default()),
        |(added, removed), file| {
            (
                added.saturating_add(file.added),
                removed.saturating_add(file.removed),
            )
        },
    );
    write!(
        html,
        "<nav class=\"file-index\" aria-label=\"{}\"><p class=\"totals\"><span class=\"added-count\">+{added}</span> <span class=\"removed-count\">−{removed}</span></p><ul>",
        labels.files,
    )?;
    for (file_index, file) in files.iter().enumerate() {
        write!(
            html,
            "<li><a href=\"#repository-{index}-file-{file_index}\"><code>{}</code></a><span class=\"file-stats\"><span class=\"added-count\">+{}</span> <span class=\"removed-count\">−{}</span></span></li>",
            Escaped(&file.path.to_string_lossy()),
            file.added,
            file.removed
        )?;
    }
    html.push_str("</ul></nav>");
    Ok(())
}

fn render_commits(html: &mut String, commits: &[Commit], labels: &Labels) -> fmt::Result {
    write!(
        html,
        "<details class=\"commits sidebar-content\"><summary>{} <span class=\"count\">{}</span></summary>",
        labels.commits,
        commits.len()
    )?;
    if commits.is_empty() {
        write!(html, "<p class=\"empty\">{}</p>", labels.no_commits)?;
    } else {
        html.push_str("<ol>");
        for commit in commits {
            write!(
                html,
                "<li><code class=\"commit-id\" title=\"{}\">{}</code><strong>{}</strong><time>{}</time>",
                Escaped(commit.id.as_ref()),
                Escaped(&commit.id.abbreviated(CommitIdAbbreviation::TenCharacters)),
                Escaped(&commit.subject),
                Escaped(commit.committed_at.as_ref())
            )?;
            if !commit.body.is_empty() {
                write!(html, "<pre>{}</pre>", Escaped(&commit.body))?;
            }
            html.push_str("</li>");
        }
        html.push_str("</ol>");
    }
    html.push_str("</details>");
    Ok(())
}

#[cfg(test)]
mod tests;
