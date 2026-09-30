use std::{
    borrow::Cow,
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

use gtl_application::ports::GitDiffFormat;
use gtl_models::{git::GitDiffSpec, paths::RepositoryRelativePath};

const TEXT_BLOB_BYTES_MAX: usize = 8 * 1024 * 1024;

pub(super) fn expand_binary_text(
    repo_path: &Path,
    args: &[&str],
    index: Option<&Path>,
    spec: &GitDiffSpec,
    format: GitDiffFormat,
    raw: String,
) -> String {
    if !raw.starts_with("diff --git ") || !raw.contains("\nBinary files ") {
        return raw;
    }
    let Some(paths) = changed_paths(repo_path, args, index) else {
        return raw;
    };
    let mut starts = vec![0];
    starts.extend(
        raw.match_indices("\ndiff --git ")
            .map(|(offset, _)| offset + 1),
    );
    if starts.len() != paths.len() {
        return raw;
    }
    starts.push(raw.len());

    let mut expanded = String::with_capacity(raw.len());
    for (bounds, path) in starts.windows(2).zip(&paths) {
        let section = &raw[bounds[0]..bounds[1]];
        match expand_section(repo_path, section, path, spec, format) {
            Some(text) => expanded.push_str(&text),
            None => expanded.push_str(section),
        }
    }
    expanded
}

fn changed_paths(
    repo_path: &Path,
    args: &[&str],
    index: Option<&Path>,
) -> Option<Vec<RepositoryRelativePath>> {
    let mut names_args = vec!["diff", "--name-only", "-z"];
    names_args.extend_from_slice(args.get(1..)?);
    let output = crate::git_process::run_with_index(repo_path, &names_args, index).ok()?;
    if !output.success() {
        return None;
    }
    output
        .stdout
        .split_terminator('\0')
        .map(|path| RepositoryRelativePath::try_new(PathBuf::from(path)).ok())
        .collect()
}

fn expand_section(
    repo_path: &Path,
    section: &str,
    path: &RepositoryRelativePath,
    spec: &GitDiffSpec,
    format: GitDiffFormat,
) -> Option<String> {
    let notice_start = section.find("\nBinary files ")? + 1;
    let notice_end = section[notice_start..]
        .find('\n')
        .map_or(section.len(), |offset| notice_start + offset);
    if !section[notice_start..notice_end].ends_with(" differ") {
        return None;
    }
    let index_line = section
        .lines()
        .find_map(|line| line.strip_prefix("index "))?;
    let (old_id, new_id) = index_line.split_whitespace().next()?.split_once("..")?;
    let old_bytes = read_blob(repo_path, old_id)?;
    let new_bytes = match spec {
        GitDiffSpec::Range(_) => read_blob(repo_path, new_id)?,
        GitDiffSpec::AgainstWorkingTree(_) => read_working_tree(repo_path, path, section)?,
    };
    let (old_text, old_utf16) = decode_text(&old_bytes)?;
    let (new_text, new_utf16) = decode_text(&new_bytes)?;
    if !old_utf16 && !new_utf16 || !diff_allowed(repo_path, path) {
        return None;
    }
    let hunk = decoded_hunks(repo_path, &old_text, &new_text, format)?;
    let mut expanded = String::with_capacity(section.len() + hunk.len());
    expanded.push_str(&section[..notice_start]);
    expanded.push_str(&hunk);
    if notice_end < section.len() {
        expanded.push_str(&section[notice_end + 1..]);
    }
    Some(expanded)
}

fn diff_allowed(repo_path: &Path, path: &RepositoryRelativePath) -> bool {
    let Some(path) = path.as_path().to_str() else {
        return false;
    };
    let Ok(raw) = super::run_git(repo_path, &["check-attr", "-z", "diff", "--", path]) else {
        return false;
    };
    let mut fields = raw.split('\0');
    matches!((fields.next(), fields.next(), fields.next()), (Some(_), Some("diff"), Some(value)) if value != "unset")
}

fn read_blob(repo_path: &Path, object_id: &str) -> Option<Vec<u8>> {
    if object_id.len() < 4
        || object_id.len() > 64
        || !object_id.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }
    if object_id.bytes().all(|byte| byte == b'0') {
        return Some(Vec::new());
    }
    let size = super::run_git(repo_path, &["cat-file", "-s", object_id])
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()?;
    if size > TEXT_BLOB_BYTES_MAX {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["cat-file", "blob", object_id])
        .output()
        .ok()?;
    (output.status.success() && output.stdout.len() <= TEXT_BLOB_BYTES_MAX).then_some(output.stdout)
}

fn read_working_tree(
    repo_path: &Path,
    path: &RepositoryRelativePath,
    section: &str,
) -> Option<Vec<u8>> {
    if section
        .lines()
        .any(|line| line.starts_with("deleted file mode "))
    {
        return Some(Vec::new());
    }
    let file_path = repo_path.join(path.as_path());
    let metadata = std::fs::symlink_metadata(&file_path).ok()?;
    if !metadata.file_type().is_file() || metadata.len() > TEXT_BLOB_BYTES_MAX as u64 {
        return None;
    }
    let mut file = std::fs::File::open(file_path).ok()?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).ok()?);
    file.by_ref()
        .take((TEXT_BLOB_BYTES_MAX + 1) as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= TEXT_BLOB_BYTES_MAX).then_some(bytes)
}

fn decode_text(bytes: &[u8]) -> Option<(Cow<'_, str>, bool)> {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let text = decode_utf16(rest, u16::from_le_bytes)?;
        return (!text.contains('\0')).then_some((Cow::Owned(text), true));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let text = decode_utf16(rest, u16::from_be_bytes)?;
        return (!text.contains('\0')).then_some((Cow::Owned(text), true));
    }
    let text = std::str::from_utf8(bytes).ok()?;
    (!text.contains('\0')).then_some((Cow::Borrowed(text), false))
}

fn decode_utf16(bytes: &[u8], from_bytes: fn([u8; 2]) -> u16) -> Option<String> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return None;
    }
    let units = pairs.iter().copied().map(from_bytes).collect::<Vec<_>>();
    String::from_utf16(&units).ok()
}

fn decoded_hunks(repo_path: &Path, old: &str, new: &str, format: GitDiffFormat) -> Option<String> {
    let directory = tempfile::tempdir().ok()?;
    let old_path = directory.path().join("before");
    let new_path = directory.path().join("after");
    std::fs::write(&old_path, old).ok()?;
    std::fs::write(&new_path, new).ok()?;
    let mut args = vec![
        "diff",
        "--no-index",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
    ];
    if format == GitDiffFormat::FullContext {
        args.push("--unified=2147483647");
    }
    args.extend(["--", old_path.to_str()?, new_path.to_str()?]);
    let output = crate::git_process::run(repo_path, &args).ok()?;
    if output.exit_code != 1 {
        return None;
    }
    let hunk_start = output.stdout.find("\n@@ -")? + 1;
    Some(output.stdout[hunk_start..].to_owned())
}
