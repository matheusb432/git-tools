use std::{
    collections::{BTreeSet, VecDeque},
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

const AUTHORED_CSS_ALLOWED: &[&str] = &[
    "crates/preview/src/styles/base.css",
    "crates/preview/src/styles/theme-map.css",
    "crates/preview/src/styles/tokens.css",
];
const AUTHORED_CSS_ROOTS: &[&str] = &["crates/preview/src", "crates/desktop/src"];
const GENERATED_CSS_PATH: &str = "crates/preview/src/embedded/generated/preview.css";
const TRAVERSAL_LIMITS: TraversalLimits = TraversalLimits {
    directory_depth_max: 32,
    directory_entry_count_max: 10_000,
};

#[derive(Clone, Copy)]
struct TraversalLimits {
    directory_depth_max: usize,
    directory_entry_count_max: usize,
}

pub(super) fn run() -> Result<()> {
    let repository = Path::new(".");
    check_authored_css(repository)?;

    let generated_path = Path::new(GENERATED_CSS_PATH);
    let css = fs::read_to_string(generated_path)
        .with_context(|| format!("reading generated CSS file {}", generated_path.display()))?;
    check_compiled_css(generated_path, &css)
}

fn check_authored_css(repository: &Path) -> Result<()> {
    let actual = inventory_authored_css(repository)?;
    let allowed = AUTHORED_CSS_ALLOWED
        .iter()
        .map(PathBuf::from)
        .collect::<BTreeSet<_>>();

    if let Some(path) = actual.difference(&allowed).next() {
        bail!("unexpected authored CSS file: {}", path.display());
    }
    if let Some(path) = allowed.difference(&actual).next() {
        bail!("required authored CSS file is missing: {}", path.display());
    }
    Ok(())
}

fn inventory_authored_css(repository: &Path) -> Result<BTreeSet<PathBuf>> {
    inventory_authored_css_with_limits(repository, TRAVERSAL_LIMITS)
}

fn inventory_authored_css_with_limits(
    repository: &Path,
    limits: TraversalLimits,
) -> Result<BTreeSet<PathBuf>> {
    let mut css_paths = BTreeSet::new();
    let mut directories = AUTHORED_CSS_ROOTS
        .iter()
        .map(|path| (repository.join(path), 0))
        .collect::<VecDeque<_>>();
    let mut directory_entry_count = 0;

    while let Some((directory, directory_depth)) = directories.pop_front() {
        let directory_repository_path = directory
            .strip_prefix(repository)
            .with_context(|| format!("resolving repository path for {}", directory.display()))?;
        let entries = fs::read_dir(&directory)
            .with_context(|| format!("reading source directory {}", directory.display()))?;
        let directory_entry_count_remaining = limits
            .directory_entry_count_max
            .saturating_sub(directory_entry_count);
        let mut entries_bounded = Vec::new();
        for entry in entries {
            let entry = entry
                .with_context(|| format!("reading source entry in {}", directory.display()))?;
            if entries_bounded.len() == directory_entry_count_remaining {
                bail!(
                    "source traversal directory entry count exceeds limit {} while reading {}",
                    limits.directory_entry_count_max,
                    directory_repository_path.display()
                );
            }
            entries_bounded.push(entry);
        }
        directory_entry_count += entries_bounded.len();
        entries_bounded.sort_by_key(fs::DirEntry::path);

        for entry in entries_bounded {
            let path = entry.path();
            let repository_path = path
                .strip_prefix(repository)
                .with_context(|| format!("resolving repository path for {}", path.display()))?;

            if contains_embedded_generated(repository_path) {
                continue;
            }

            let file_type = entry
                .file_type()
                .with_context(|| format!("reading file type for {}", path.display()))?;
            if file_type.is_dir() {
                let directory_depth_next = directory_depth + 1;
                if directory_depth_next > limits.directory_depth_max {
                    bail!(
                        "source traversal directory depth {directory_depth_next} exceeds limit {} at {}",
                        limits.directory_depth_max,
                        repository_path.display()
                    );
                }
                directories.push_back((path, directory_depth_next));
            } else if file_type.is_file() && path.extension() == Some(OsStr::new("css")) {
                css_paths.insert(repository_path.to_path_buf());
            }
        }
    }

    Ok(css_paths)
}

fn contains_embedded_generated(path: &Path) -> bool {
    let components = path.iter().collect::<Vec<_>>();
    components
        .windows(2)
        .any(|parts| parts == [OsStr::new("embedded"), OsStr::new("generated")])
}

fn check_compiled_css(path: &Path, css: &str) -> Result<()> {
    if let Some((declaration, byte_offset)) = forbidden_declaration(css) {
        bail!(
            "{}: forbidden CSS declaration '{declaration}' at byte offset {byte_offset}",
            path.display()
        );
    }
    Ok(())
}

fn forbidden_declaration(css: &str) -> Option<(String, usize)> {
    let (normalized, byte_offsets) = lex_css_code(css);
    let bytes = normalized.as_bytes();

    for (colon_offset, byte) in bytes.iter().enumerate() {
        if *byte != b':' {
            continue;
        }

        let declaration_offset = bytes[..colon_offset]
            .iter()
            .rposition(|byte| matches!(byte, b'{' | b'}' | b';'))
            .map_or(0, |offset| offset + 1);
        let property = &normalized[declaration_offset..colon_offset];

        if property == "transition" || property.starts_with("transition-") {
            return Some((property.to_owned(), byte_offsets[declaration_offset]));
        }
        if property == "scroll-behavior" && normalized[colon_offset + 1..].starts_with("smooth") {
            return Some((
                "scroll-behavior:smooth".to_owned(),
                byte_offsets[declaration_offset],
            ));
        }
    }

    None
}

enum CssLexerState {
    Code,
    Comment,
    Quoted(char),
}

fn lex_css_code(source: &str) -> (String, Vec<usize>) {
    let mut normalized = String::with_capacity(source.len());
    let mut byte_offsets = Vec::with_capacity(source.len());
    let mut characters = source.char_indices().peekable();
    let mut state = CssLexerState::Code;

    while let Some((byte_offset, character)) = characters.next() {
        match state {
            CssLexerState::Code => match character {
                '/' if characters.peek().is_some_and(|(_, next)| *next == '*') => {
                    characters.next();
                    state = CssLexerState::Comment;
                }
                '\'' | '"' => {
                    push_normalized(&mut normalized, &mut byte_offsets, character, byte_offset);
                    state = CssLexerState::Quoted(character);
                }
                '\\' => {
                    push_normalized(&mut normalized, &mut byte_offsets, character, byte_offset);
                    if let Some((escaped_offset, _)) = characters.next() {
                        push_normalized(&mut normalized, &mut byte_offsets, '_', escaped_offset);
                    }
                }
                _ if character.is_ascii_whitespace() => {}
                _ => push_normalized(&mut normalized, &mut byte_offsets, character, byte_offset),
            },
            CssLexerState::Comment => {
                if character == '*' && characters.peek().is_some_and(|(_, next)| *next == '/') {
                    characters.next();
                    state = CssLexerState::Code;
                }
            }
            CssLexerState::Quoted(quote) => {
                if character == '\\' {
                    characters.next();
                } else if character == quote {
                    push_normalized(&mut normalized, &mut byte_offsets, character, byte_offset);
                    state = CssLexerState::Code;
                }
            }
        }
    }

    (normalized, byte_offsets)
}

fn push_normalized(
    normalized: &mut String,
    byte_offsets: &mut Vec<usize>,
    character: char,
    byte_offset: usize,
) {
    normalized.push(character);
    byte_offsets.resize(normalized.len(), byte_offset);
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use tempfile::TempDir;

    use super::{
        TraversalLimits, check_authored_css, check_compiled_css, inventory_authored_css_with_limits,
    };

    const GENERATED_CSS_PATH: &str = "crates/preview/src/embedded/generated/preview.css";

    fn repository_with_allowed_css() -> TempDir {
        let repository = TempDir::new().expect("temporary repository");
        for path in [
            "crates/preview/src/styles/base.css",
            "crates/preview/src/styles/theme-map.css",
            "crates/preview/src/styles/tokens.css",
        ] {
            let path = repository.path().join(path);
            fs::create_dir_all(path.parent().expect("CSS parent directory"))
                .expect("create CSS parent directory");
            fs::write(path, "").expect("write CSS fixture");
        }
        fs::create_dir_all(repository.path().join("crates/desktop/src"))
            .expect("create desktop source directory");
        repository
    }

    fn write_css(repository: &TempDir, path: &str) {
        let path = repository.path().join(path);
        fs::create_dir_all(path.parent().expect("CSS parent directory"))
            .expect("create CSS parent directory");
        fs::write(path, "").expect("write CSS fixture");
    }

    #[test]
    fn authored_css_accepts_the_exact_allowlist() {
        let repository = repository_with_allowed_css();

        assert!(check_authored_css(repository.path()).is_ok());
    }

    #[test]
    fn authored_css_ignores_embedded_generated_files() {
        let repository = repository_with_allowed_css();
        write_css(
            &repository,
            "crates/preview/src/embedded/generated/extra.css",
        );
        write_css(
            &repository,
            "crates/desktop/src/embedded/generated/extra.css",
        );

        assert!(check_authored_css(repository.path()).is_ok());
    }

    #[test]
    fn authored_css_rejects_an_extra_preview_file() {
        let repository = repository_with_allowed_css();
        let path = "crates/preview/src/styles/viewer.css";
        write_css(&repository, path);

        let error = check_authored_css(repository.path())
            .expect_err("extra preview CSS should fail")
            .to_string();

        assert!(error.contains(path), "{error}");
    }

    #[test]
    fn authored_css_rejects_an_extra_desktop_file() {
        let repository = repository_with_allowed_css();
        let path = "crates/desktop/src/render/viewer.css";
        write_css(&repository, path);

        let error = check_authored_css(repository.path())
            .expect_err("extra desktop CSS should fail")
            .to_string();

        assert!(error.contains(path), "{error}");
    }

    #[test]
    fn authored_css_rejects_directory_depth_over_limit() {
        let repository = repository_with_allowed_css();
        let path = "crates/preview/src/nested/deeper";
        fs::create_dir_all(repository.path().join(path)).expect("create nested source directory");
        let limits = TraversalLimits {
            directory_depth_max: 1,
            directory_entry_count_max: 100,
        };

        let error = inventory_authored_css_with_limits(repository.path(), limits)
            .expect_err("source depth should be bounded")
            .to_string();

        assert!(error.contains(path), "{error}");
        assert!(
            error.contains("directory depth 2 exceeds limit 1"),
            "{error}"
        );
    }

    #[test]
    fn authored_css_rejects_total_directory_entries_over_limit() {
        let repository = repository_with_allowed_css();
        let limits = TraversalLimits {
            directory_depth_max: 32,
            directory_entry_count_max: 0,
        };

        let error = inventory_authored_css_with_limits(repository.path(), limits)
            .expect_err("source entry count should be bounded")
            .to_string();

        assert!(error.contains("crates/preview/src"), "{error}");
        assert!(
            error.contains("directory entry count exceeds limit 0"),
            "{error}"
        );
    }

    #[test]
    fn compiled_css_rejects_transition_declarations() {
        for (css, declaration, offset) in [
            (".x { transition : opacity; }", "transition", 5),
            (
                ".x { transition-property : opacity; }",
                "transition-property",
                5,
            ),
        ] {
            let error = check_compiled_css(Path::new(GENERATED_CSS_PATH), css)
                .expect_err("transition declaration should fail")
                .to_string();

            assert!(error.contains(GENERATED_CSS_PATH), "{error}");
            assert!(error.contains(&format!("'{declaration}'")), "{error}");
            assert!(error.contains(&format!("byte offset {offset}")), "{error}");
        }
    }

    #[test]
    fn compiled_css_rejects_smooth_scroll_with_or_without_whitespace() {
        for (css, offset) in [
            (".x{scroll-behavior:smooth}", 3),
            (".x {\n scroll-behavior : smooth ;\n}", 6),
        ] {
            let error = check_compiled_css(Path::new(GENERATED_CSS_PATH), css)
                .expect_err("smooth scrolling should fail")
                .to_string();

            assert!(error.contains(GENERATED_CSS_PATH), "{error}");
            assert!(error.contains("'scroll-behavior:smooth'"), "{error}");
            assert!(error.contains(&format!("byte offset {offset}")), "{error}");
        }
    }

    #[test]
    fn compiled_css_accepts_declaration_text_inside_strings() {
        let css = r#".x{content:";transition:opacity";color:red}"#;

        assert!(check_compiled_css(Path::new(GENERATED_CSS_PATH), css).is_ok());
    }

    #[test]
    fn compiled_css_accepts_escaped_quotes_and_string_separators() {
        let css = r#".x{content:"a\";transition:opacity}";color:red}"#;

        assert!(check_compiled_css(Path::new(GENERATED_CSS_PATH), css).is_ok());
    }

    #[test]
    fn compiled_css_rejects_transition_after_comments() {
        for (css, offset) in [
            (".x{/**/transition:opacity}", 7),
            (".x{/*! keep */transition:opacity}", 14),
        ] {
            let error = check_compiled_css(Path::new(GENERATED_CSS_PATH), css)
                .expect_err("transition after comment should fail")
                .to_string();

            assert!(error.contains("'transition'"), "{error}");
            assert!(error.contains(&format!("byte offset {offset}")), "{error}");
        }
    }

    #[test]
    fn compiled_css_accepts_animation_declarations() {
        let css = "@keyframes pulse{to{opacity:0}}.x{animation:pulse 1s}";

        assert!(check_compiled_css(Path::new(GENERATED_CSS_PATH), css).is_ok());
    }
}
