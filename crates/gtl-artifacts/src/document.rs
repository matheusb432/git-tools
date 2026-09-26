use std::fmt::{self, Write as _};

use anyhow::{Context as _, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use gtl_application::diffs::View;
use gtl_models::{
    diffs::CommitIdAbbreviation,
    settings::ViewerLanguage,
    viewer::{RenderOptions, Theme},
};
use sha2::{Digest as _, Sha256};

use super::{labels::Labels, rows};

const STYLESHEET: &str = include_str!("document.css");

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

fn content_security_policy() -> String {
    let stylesheet_sha256 = STANDARD.encode(Sha256::digest(STYLESHEET.as_bytes()));
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
        view.repo_name,
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
    let mut html = String::new();
    write!(html, "<!doctype html><html lang=\"{}\" data-theme=\"{}\"><head><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"{}\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta name=\"color-scheme\" content=\"dark\"><meta name=\"darkreader-lock\"><title>{}</title><style>{STYLESHEET}</style></head><body><main class=\"{}\"><header><p class=\"brand\">git-tools</p><h1>{}</h1></header>", language.as_str(), theme.unwrap_or_default(), content_security_policy(), Escaped(title), if options.wrap_lines() { "wrap" } else { "nowrap" }, Escaped(title))
        .context("write artifact document head")?;
    if views.is_empty() {
        write!(html, "<p>{}</p>", labels.empty)?;
    }
    if views.len() > 1 {
        write!(html, "<nav aria-label=\"{}\"><ul>", labels.repositories)?;
        for (index, view) in views.iter().enumerate() {
            write!(
                html,
                "<li><a href=\"#repository-{index}\">{}</a></li>",
                Escaped(view.repo_name.as_str())
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
        "<section id=\"repository-{index}\" class=\"repository\"><h2>{}</h2><p>{} · <code>{}</code> → <code>{}</code></p><p class=\"command\"><code>{}</code></p>",
        Escaped(view.repo_name.as_str()),
        Escaped(&Labels::title(&view.title)),
        Escaped(&view.branch.to_string()),
        Escaped(view.upstream.as_str()),
        Escaped(&view.foot.cmd)
    )?;
    if !view.commits.is_empty() {
        write!(
            html,
            "<details class=\"commits\"><summary>{} ({})</summary><ol>",
            labels.commits,
            view.commits.len()
        )?;
        for commit in &view.commits {
            write!(
                html,
                "<li><code title=\"{}\">{}</code> <strong>{}</strong> <time>{}</time>",
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
        html.push_str("</ol></details>");
    }
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
    if view.files.is_empty() {
        write!(html, "<p>{}</p>", labels.empty)?;
    } else {
        write!(
            html,
            "<nav class=\"file-index\" aria-label=\"{}\"><h3>{} ({})</h3><ul>",
            labels.files,
            labels.files,
            view.files.len()
        )?;
        for (file_index, file) in view.files.iter().enumerate() {
            write!(
                html,
                "<li><a href=\"#repository-{index}-file-{file_index}\"><code>{}</code></a> <span class=\"added-count\">+{}</span> <span class=\"removed-count\">−{}</span></li>",
                Escaped(&file.path.to_string_lossy()),
                file.added,
                file.removed
            )?;
        }
        html.push_str("</ul></nav>");
        for (file_index, file) in view.files.iter().enumerate() {
            write!(
                html,
                "<details class=\"file\" id=\"repository-{index}-file-{file_index}\" open><summary><span>{}</span> <span class=\"added-count\">+{}</span> <span class=\"removed-count\">−{}</span></summary>",
                Escaped(&file.path.to_string_lossy()),
                file.added,
                file.removed
            )?;
            rows::render(html, file, options.density(), labels)?;
            html.push_str("</details>");
        }
    }
    html.push_str("</section>");
    Ok(())
}

#[cfg(test)]
mod tests;
