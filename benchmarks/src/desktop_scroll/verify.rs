use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use sha2::{Digest, Sha256};

use super::{
    BASE_SUBJECT, BASE_TIMESTAMP, COMMIT_COUNT, DISTINCT_FILE_COUNT, DesktopScrollFixtureError,
    DesktopScrollFixtureEvidence, DesktopScrollManifest, FILE_TOUCH_COUNT_PER_COMMIT,
    FIXTURE_INPUT_BYTES_MAX, FixtureStatuses, GIT_RANGE, IDENTITY_EMAIL, IDENTITY_NAME,
    PATCH_FILE_BYTES_MAX, file_system_error, hex_bytes, invalid,
    repository::{FixtureRepository, copy_tree},
    sha256_hex,
};

const COMPACT_DIFF_ROWS_MIN: usize = 3_000;
const COMPACT_DIFF_ROWS_MAX: usize = 6_000;
const FULL_CONTEXT_DIFF_ROWS_MIN: usize = 14_000;
const FULL_CONTEXT_DIFF_ROWS_MAX: usize = 22_000;
const FIXTURE_FILE_COUNT_MAX: usize = 100;

pub(super) struct TextTreeEvidence {
    pub(super) file_count: usize,
    pub(super) bytes: u64,
    pub(super) sha256: String,
}

pub(super) struct MeasuredRepository {
    pub(super) commit_count: usize,
    pub(super) distinct_file_count: usize,
    pub(super) file_touch_counts: Vec<usize>,
    pub(super) compact_diff_rows: usize,
    pub(super) compact_diff_bytes: u64,
    pub(super) full_context_diff_rows: usize,
    pub(super) full_context_diff_bytes: u64,
    pub(super) additions: u64,
    pub(super) deletions: u64,
    pub(super) head_commit_id: String,
    pub(super) file_types: BTreeMap<String, usize>,
    pub(super) statuses: FixtureStatuses,
}

pub(super) fn inspect_text_tree(
    root: &Path,
) -> Result<TextTreeEvidence, DesktopScrollFixtureError> {
    let files = collect_text_files(root)?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    for (path, contents) in &files {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid(format!("fixture path escaped root: {}", path.display())))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| invalid(format!("fixture path is not UTF-8: {}", path.display())))?
            .replace('\\', "/");
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update(contents);
        hasher.update([0]);
        bytes = bytes.saturating_add(contents.len() as u64);
    }
    Ok(TextTreeEvidence {
        file_count: files.len(),
        bytes,
        sha256: hex_bytes(&hasher.finalize()),
    })
}

pub(super) fn measure_repository(
    repository: &FixtureRepository,
) -> Result<MeasuredRepository, DesktopScrollFixtureError> {
    let commit_ids_output = repository.output_text(
        "list measured commits",
        ["rev-list", "--reverse", GIT_RANGE],
    )?;
    let commit_ids = nonempty_lines(&commit_ids_output);
    let file_touch_counts = commit_ids
        .iter()
        .map(|commit_id| {
            repository
                .output_text(
                    "count files touched by commit",
                    [
                        "diff-tree",
                        "--no-commit-id",
                        "--name-only",
                        "-r",
                        "-M",
                        commit_id,
                    ],
                )
                .map(|output| nonempty_lines(&output).len())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let compact = repository.output_text("capture compact workload", ["diff", GIT_RANGE])?;
    let full_context = repository.output_text(
        "capture full-context workload",
        ["diff", "--unified=2147483647", GIT_RANGE],
    )?;
    let names = repository.output_text(
        "list distinct changed files",
        ["diff", "--name-only", GIT_RANGE],
    )?;
    let numstat =
        repository.output_text("count changed lines", ["diff", "--numstat", GIT_RANGE])?;
    let name_status = repository.output_text(
        "classify changed files",
        ["diff", "--name-status", "-M", GIT_RANGE],
    )?;
    let (additions, deletions) = parse_numstat(&numstat)?;
    let (statuses, file_types) = parse_name_status(&name_status)?;
    let head_commit_id = repository
        .output_text("read fixture head commit ID", ["rev-parse", "HEAD"])?
        .trim()
        .to_owned();

    Ok(MeasuredRepository {
        commit_count: commit_ids.len(),
        distinct_file_count: nonempty_lines(&names).len(),
        file_touch_counts,
        compact_diff_rows: compact.lines().count(),
        compact_diff_bytes: compact.len() as u64,
        full_context_diff_rows: full_context.lines().count(),
        full_context_diff_bytes: full_context.len() as u64,
        additions,
        deletions,
        head_commit_id,
        file_types,
        statuses,
    })
}

pub(super) fn hydrate_fixture(
    root: &Path,
    destination: &Path,
) -> Result<DesktopScrollManifest, DesktopScrollFixtureError> {
    let manifest = load_manifest(root)?;
    validate_manifest_shape(&manifest)?;
    prepare_empty_destination(destination)?;
    copy_tree(&root.join("base"), destination)?;
    let repository = FixtureRepository::initialize(destination)?;
    let base_commit_id = repository.commit_base()?;
    if base_commit_id != manifest.base.commit_id {
        return Err(invalid(format!(
            "hydrated base commit is {base_commit_id}, expected {}",
            manifest.base.commit_id
        )));
    }
    for commit in &manifest.commits {
        let patch = safe_fixture_path(root, &commit.patch)?;
        repository.apply_patch(&patch)?;
        let commit_id = repository
            .output_text("read applied fixture commit ID", ["rev-parse", "HEAD"])?
            .trim()
            .to_owned();
        if commit_id != commit.commit_id {
            return Err(invalid(format!(
                "patch {} produced commit {commit_id}, expected {}",
                commit.patch, commit.commit_id
            )));
        }
    }
    Ok(manifest)
}

pub(super) fn verify_fixture(
    root: &Path,
) -> Result<DesktopScrollFixtureEvidence, DesktopScrollFixtureError> {
    let manifest = load_manifest(root)?;
    validate_manifest_shape(&manifest)?;
    let all_files = collect_text_files(root)?;
    if all_files.len() > FIXTURE_FILE_COUNT_MAX {
        return Err(invalid(format!(
            "fixture contains {} files, above {FIXTURE_FILE_COUNT_MAX}",
            all_files.len()
        )));
    }
    for (path, contents) in &all_files {
        validate_sanitized_text(path, contents)?;
    }
    validate_fixture_paths(root, &manifest, &all_files)?;

    let base_evidence = inspect_text_tree(&root.join("base"))?;
    compare(
        "base source file count",
        &base_evidence.file_count,
        &manifest.base.source_file_count,
    )?;
    compare(
        "base source bytes",
        &base_evidence.bytes,
        &manifest.base.source_bytes,
    )?;
    compare(
        "base tree SHA-256",
        &base_evidence.sha256,
        &manifest.base.tree_sha256,
    )?;

    let mut patch_series_bytes = 0_u64;
    for commit in &manifest.commits {
        let patch = safe_fixture_path(root, &commit.patch)?;
        let bytes = fs::read(&patch).map_err(file_system_error("read fixture patch", &patch))?;
        if bytes.len() as u64 > manifest.bounds.patch_file_bytes_max {
            return Err(invalid(format!(
                "patch {} has {} bytes, above {}",
                commit.patch,
                bytes.len(),
                manifest.bounds.patch_file_bytes_max
            )));
        }
        compare(
            &format!("patch {} SHA-256", commit.patch),
            &sha256_hex(&bytes),
            &commit.patch_sha256,
        )?;
        patch_series_bytes = patch_series_bytes.saturating_add(bytes.len() as u64);
    }
    compare(
        "patch series bytes",
        &patch_series_bytes,
        &manifest.workload.patch_series_bytes,
    )?;
    let fixture_input_bytes = base_evidence.bytes.saturating_add(patch_series_bytes);
    if fixture_input_bytes > manifest.bounds.fixture_input_bytes_max {
        return Err(invalid(format!(
            "fixture inputs contain {fixture_input_bytes} bytes, above {}",
            manifest.bounds.fixture_input_bytes_max
        )));
    }

    let hydrated = tempfile::Builder::new()
        .prefix("desktop-scroll-verify-")
        .tempdir()
        .map_err(file_system_error(
            "create fixture verification directory",
            root,
        ))?;
    let repository_path = hydrated.path().join(&manifest.repository_name);
    hydrate_fixture(root, &repository_path)?;
    let repository = FixtureRepository::initialize_existing(&repository_path)?;
    let measured = measure_repository(&repository)?;
    compare_measured_workload(&manifest, &measured)?;
    verify_commit_metadata(&manifest, &repository)?;

    Ok(DesktopScrollFixtureEvidence {
        manifest,
        fixture_input_bytes,
    })
}

fn load_manifest(root: &Path) -> Result<DesktopScrollManifest, DesktopScrollFixtureError> {
    let path = root.join("manifest.toml");
    let text = fs::read_to_string(&path)
        .map_err(file_system_error("read desktop scroll manifest", &path))?;
    toml::from_str(&text)
        .map_err(|source| DesktopScrollFixtureError::DecodeManifest { path, source })
}

fn validate_manifest_shape(
    manifest: &DesktopScrollManifest,
) -> Result<(), DesktopScrollFixtureError> {
    validate_manifest_identity(manifest)?;
    validate_manifest_commits(manifest)?;
    validate_manifest_distributions(manifest)?;
    validate_manifest_rows(manifest)
}

fn validate_manifest_identity(
    manifest: &DesktopScrollManifest,
) -> Result<(), DesktopScrollFixtureError> {
    compare("manifest format version", &manifest.format_version, &1)?;
    compare(
        "fixture name",
        manifest.fixture_name.as_str(),
        "desktop-scroll-v1",
    )?;
    compare(
        "fixture repository name",
        manifest.repository_name.as_str(),
        "desktop-scroll-fixture",
    )?;
    compare("fixture Git range", manifest.git_range.as_str(), GIT_RANGE)?;
    compare(
        "fixture identity name",
        manifest.identity.name.as_str(),
        IDENTITY_NAME,
    )?;
    compare(
        "fixture identity email",
        manifest.identity.email.as_str(),
        IDENTITY_EMAIL,
    )?;
    compare("base subject", manifest.base.subject.as_str(), BASE_SUBJECT)?;
    compare(
        "base timestamp",
        manifest.base.authored_at.as_str(),
        BASE_TIMESTAMP,
    )?;
    compare(
        "commit count",
        &manifest.workload.commit_count,
        &COMMIT_COUNT,
    )?;
    compare(
        "distinct changed file count",
        &manifest.workload.distinct_file_count,
        &DISTINCT_FILE_COUNT,
    )?;
    compare(
        "manifest commit entries",
        &manifest.commits.len(),
        &COMMIT_COUNT,
    )?;
    compare(
        "fixture input byte bound",
        &manifest.bounds.fixture_input_bytes_max,
        &FIXTURE_INPUT_BYTES_MAX,
    )?;
    compare(
        "patch file byte bound",
        &manifest.bounds.patch_file_bytes_max,
        &PATCH_FILE_BYTES_MAX,
    )?;
    Ok(())
}

fn validate_manifest_commits(
    manifest: &DesktopScrollManifest,
) -> Result<(), DesktopScrollFixtureError> {
    for (index, commit) in manifest.commits.iter().enumerate() {
        compare("commit sequence", &commit.sequence, &(index + 1))?;
        compare(
            &format!("commit {} file touches", commit.sequence),
            &commit.file_touch_count,
            &FILE_TOUCH_COUNT_PER_COMMIT,
        )?;
        if commit.body.trim().is_empty() {
            return Err(invalid(format!(
                "commit {} has an empty deterministic body",
                commit.sequence
            )));
        }
    }
    compare(
        "total file touches",
        &manifest.workload.file_touch_count_total,
        &(COMMIT_COUNT * FILE_TOUCH_COUNT_PER_COMMIT),
    )?;
    Ok(())
}

fn validate_manifest_distributions(
    manifest: &DesktopScrollManifest,
) -> Result<(), DesktopScrollFixtureError> {
    let file_type_count = manifest.file_types.values().sum::<usize>();
    compare(
        "file type distribution total",
        &file_type_count,
        &DISTINCT_FILE_COUNT,
    )?;
    for required in ["lockfile", "markdown", "rust", "toml"] {
        if manifest
            .file_types
            .get(required)
            .copied()
            .unwrap_or_default()
            == 0
        {
            return Err(invalid(format!(
                "file type distribution contains no {required} files"
            )));
        }
    }
    let status_count = manifest.statuses.added
        + manifest.statuses.modified
        + manifest.statuses.deleted
        + manifest.statuses.renamed;
    compare(
        "file status distribution total",
        &status_count,
        &DISTINCT_FILE_COUNT,
    )?;
    for (label, count) in [
        ("added", manifest.statuses.added),
        ("modified", manifest.statuses.modified),
        ("deleted", manifest.statuses.deleted),
        ("renamed", manifest.statuses.renamed),
    ] {
        if count == 0 {
            return Err(invalid(format!(
                "file status distribution contains no {label} paths"
            )));
        }
    }
    Ok(())
}

fn validate_manifest_rows(
    manifest: &DesktopScrollManifest,
) -> Result<(), DesktopScrollFixtureError> {
    validate_row_range(
        "compact diff rows",
        manifest.workload.compact_diff_rows,
        COMPACT_DIFF_ROWS_MIN,
        COMPACT_DIFF_ROWS_MAX,
    )?;
    validate_row_range(
        "full-context diff rows",
        manifest.workload.full_context_diff_rows,
        FULL_CONTEXT_DIFF_ROWS_MIN,
        FULL_CONTEXT_DIFF_ROWS_MAX,
    )?;
    if manifest.workload.full_context_diff_bytes <= manifest.workload.compact_diff_bytes {
        return Err(invalid(
            "full-context diff bytes do not exceed compact diff bytes",
        ));
    }
    Ok(())
}

fn validate_row_range(
    label: &str,
    actual: usize,
    minimum: usize,
    maximum: usize,
) -> Result<(), DesktopScrollFixtureError> {
    if (minimum..=maximum).contains(&actual) {
        return Ok(());
    }
    Err(invalid(format!(
        "{label} is {actual}, expected {minimum}..={maximum}"
    )))
}

fn validate_fixture_paths(
    root: &Path,
    manifest: &DesktopScrollManifest,
    files: &BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), DesktopScrollFixtureError> {
    let actual_patches = files
        .keys()
        .filter_map(|path| {
            path.strip_prefix(root)
                .ok()
                .filter(|relative| relative.starts_with("patches"))
                .map(Path::to_path_buf)
        })
        .collect::<BTreeSet<_>>();
    let expected_patches = manifest
        .commits
        .iter()
        .map(|commit| PathBuf::from(&commit.patch))
        .collect::<BTreeSet<_>>();
    compare(
        "fixture patch path inventory",
        &actual_patches,
        &expected_patches,
    )?;
    for required in ["README.md", "manifest.toml"] {
        if !files.contains_key(&root.join(required)) {
            return Err(invalid(format!("fixture is missing {required}")));
        }
    }
    for path in files.keys() {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| invalid(format!("fixture path escaped root: {}", path.display())))?;
        if !(relative == Path::new("README.md")
            || relative == Path::new("manifest.toml")
            || relative.starts_with("base")
            || relative.starts_with("patches"))
        {
            return Err(invalid(format!(
                "unexpected fixture path: {}",
                relative.display()
            )));
        }
    }
    Ok(())
}

fn compare_measured_workload(
    manifest: &DesktopScrollManifest,
    measured: &MeasuredRepository,
) -> Result<(), DesktopScrollFixtureError> {
    compare(
        "measured commit count",
        &measured.commit_count,
        &manifest.workload.commit_count,
    )?;
    compare(
        "measured distinct file count",
        &measured.distinct_file_count,
        &manifest.workload.distinct_file_count,
    )?;
    let expected_touches = manifest
        .commits
        .iter()
        .map(|commit| commit.file_touch_count)
        .collect::<Vec<_>>();
    compare(
        "per-commit file touches",
        &measured.file_touch_counts,
        &expected_touches,
    )?;
    compare(
        "compact diff rows",
        &measured.compact_diff_rows,
        &manifest.workload.compact_diff_rows,
    )?;
    compare(
        "compact diff bytes",
        &measured.compact_diff_bytes,
        &manifest.workload.compact_diff_bytes,
    )?;
    compare(
        "full-context diff rows",
        &measured.full_context_diff_rows,
        &manifest.workload.full_context_diff_rows,
    )?;
    compare(
        "full-context diff bytes",
        &measured.full_context_diff_bytes,
        &manifest.workload.full_context_diff_bytes,
    )?;
    compare(
        "expected additions",
        &measured.additions,
        &manifest.workload.additions,
    )?;
    compare(
        "expected deletions",
        &measured.deletions,
        &manifest.workload.deletions,
    )?;
    compare(
        "head commit ID",
        measured.head_commit_id.as_str(),
        manifest.workload.head_commit_id.as_str(),
    )?;
    compare(
        "file type distribution",
        &measured.file_types,
        &manifest.file_types,
    )?;
    compare(
        "file status distribution",
        &measured.statuses,
        &manifest.statuses,
    )
}

fn verify_commit_metadata(
    manifest: &DesktopScrollManifest,
    repository: &FixtureRepository,
) -> Result<(), DesktopScrollFixtureError> {
    let output = repository.output_text(
        "read measured commit metadata",
        [
            "log",
            "--reverse",
            "--format=%H%x1f%an%x1f%ae%x1f%aI%x1f%cn%x1f%ce%x1f%cI%x1f%s%x1f%b%x1f%P%x1e",
            GIT_RANGE,
        ],
    )?;
    let records = output
        .split('\x1e')
        .map(|record| record.trim_matches('\n'))
        .filter(|record| !record.is_empty())
        .collect::<Vec<_>>();
    compare(
        "commit metadata record count",
        &records.len(),
        &manifest.commits.len(),
    )?;
    for (record, expected) in records.into_iter().zip(&manifest.commits) {
        let fields = record.split('\x1f').collect::<Vec<_>>();
        if fields.len() != 10 {
            return Err(invalid(format!(
                "commit {} metadata has {} fields, expected 10",
                expected.sequence,
                fields.len()
            )));
        }
        compare("commit ID", fields[0], expected.commit_id.as_str())?;
        compare("author name", fields[1], IDENTITY_NAME)?;
        compare("author email", fields[2], IDENTITY_EMAIL)?;
        compare("author timestamp", fields[3], expected.authored_at.as_str())?;
        compare("committer name", fields[4], IDENTITY_NAME)?;
        compare("committer email", fields[5], IDENTITY_EMAIL)?;
        compare(
            "committer timestamp",
            fields[6],
            expected.authored_at.as_str(),
        )?;
        compare("commit subject", fields[7], expected.subject.as_str())?;
        compare("commit body", fields[8].trim(), expected.body.as_str())?;
        compare(
            "commit parent count",
            &fields[9].split_whitespace().count(),
            &1,
        )?;
    }
    Ok(())
}

fn prepare_empty_destination(destination: &Path) -> Result<(), DesktopScrollFixtureError> {
    fs::create_dir_all(destination).map_err(file_system_error(
        "create fixture hydration destination",
        destination,
    ))?;
    let mut entries = fs::read_dir(destination).map_err(file_system_error(
        "inspect fixture hydration destination",
        destination,
    ))?;
    if entries
        .next()
        .transpose()
        .map_err(file_system_error(
            "inspect fixture hydration destination entry",
            destination,
        ))?
        .is_some()
    {
        return Err(invalid(format!(
            "fixture hydration destination is not empty: {}",
            destination.display()
        )));
    }
    Ok(())
}

fn safe_fixture_path(root: &Path, relative: &str) -> Result<PathBuf, DesktopScrollFixtureError> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(invalid(format!(
            "unsafe fixture path: {}",
            relative.display()
        )));
    }
    Ok(root.join(relative))
}

fn collect_text_files(
    root: &Path,
) -> Result<BTreeMap<PathBuf, Vec<u8>>, DesktopScrollFixtureError> {
    let mut files = BTreeMap::new();
    collect_text_files_in_directory(root, &mut files)?;
    Ok(files)
}

fn collect_text_files_in_directory(
    current: &Path,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), DesktopScrollFixtureError> {
    let entries =
        fs::read_dir(current).map_err(file_system_error("read fixture directory", current))?;
    for entry in entries {
        let entry = entry.map_err(file_system_error("read fixture directory entry", current))?;
        collect_text_file_entry(&entry, files)?;
    }
    Ok(())
}

fn collect_text_file_entry(
    entry: &fs::DirEntry,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<(), DesktopScrollFixtureError> {
    let path = entry.path();
    let file_type = entry
        .file_type()
        .map_err(file_system_error("read fixture entry type", &path))?;
    if file_type.is_dir() {
        return collect_text_files_in_directory(&path, files);
    }
    if !file_type.is_file() {
        return Err(invalid(format!(
            "fixture contains a symlink or special entry: {}",
            path.display()
        )));
    }

    let bytes = fs::read(&path).map_err(file_system_error("read fixture text file", &path))?;
    if bytes.contains(&0) {
        return Err(invalid(format!(
            "fixture file contains a NUL byte: {}",
            path.display()
        )));
    }
    std::str::from_utf8(&bytes).map_err(|error| {
        invalid(format!(
            "fixture file is not UTF-8 at byte {}: {}",
            error.valid_up_to(),
            path.display()
        ))
    })?;
    files.insert(path, bytes);
    Ok(())
}

fn validate_sanitized_text(path: &Path, bytes: &[u8]) -> Result<(), DesktopScrollFixtureError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| invalid(format!("non-UTF-8 fixture {}: {error}", path.display())))?;
    let lowercase = text.to_ascii_lowercase();
    for private_shape in ["/home/", "/users/", "c:\\\\users\\", "c:/users/"] {
        if lowercase.contains(private_shape) {
            return Err(invalid(format!(
                "fixture contains private path shape {private_shape}: {}",
                path.display()
            )));
        }
    }
    for token in text
        .split_whitespace()
        .filter(|token| token.bytes().filter(|byte| *byte == b'@').count() == 1)
    {
        let token = token.trim_matches(|character: char| {
            matches!(character, '<' | '>' | '(' | ')' | '[' | ']' | ',' | '"')
        });
        if !token.ends_with("@example.invalid") {
            return Err(invalid(format!(
                "fixture contains a non-synthetic email token `{token}`: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn parse_numstat(output: &str) -> Result<(u64, u64), DesktopScrollFixtureError> {
    let mut additions = 0_u64;
    let mut deletions = 0_u64;
    for line in nonempty_lines(output) {
        let mut fields = line.split('\t');
        let added = fields
            .next()
            .ok_or_else(|| invalid(format!("numstat row has no additions: {line}")))?;
        let deleted = fields
            .next()
            .ok_or_else(|| invalid(format!("numstat row has no deletions: {line}")))?;
        additions = additions.saturating_add(
            added
                .parse::<u64>()
                .map_err(|_| invalid(format!("numstat additions are not numeric: {line}")))?,
        );
        deletions = deletions.saturating_add(
            deleted
                .parse::<u64>()
                .map_err(|_| invalid(format!("numstat deletions are not numeric: {line}")))?,
        );
    }
    Ok((additions, deletions))
}

fn parse_name_status(
    output: &str,
) -> Result<(FixtureStatuses, BTreeMap<String, usize>), DesktopScrollFixtureError> {
    let mut statuses = FixtureStatuses {
        added: 0,
        modified: 0,
        deleted: 0,
        renamed: 0,
    };
    let mut file_types = BTreeMap::new();
    for line in nonempty_lines(output) {
        let fields = line.split('\t').collect::<Vec<_>>();
        let status = fields
            .first()
            .and_then(|field| field.chars().next())
            .ok_or_else(|| invalid(format!("name-status row has no status: {line}")))?;
        let path = match status {
            'A' => {
                statuses.added += 1;
                fields.get(1)
            }
            'M' => {
                statuses.modified += 1;
                fields.get(1)
            }
            'D' => {
                statuses.deleted += 1;
                fields.get(1)
            }
            'R' => {
                statuses.renamed += 1;
                fields.get(2)
            }
            _ => return Err(invalid(format!("unsupported name-status row: {line}"))),
        }
        .ok_or_else(|| invalid(format!("name-status row has no path: {line}")))?;
        *file_types.entry(file_type(path).to_owned()).or_default() += 1;
    }
    Ok((statuses, file_types))
}

fn file_type(path: &str) -> &'static str {
    if path.ends_with("Cargo.lock") {
        return "lockfile";
    }
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("html") => "html",
        Some("js") => "javascript",
        Some("md") => "markdown",
        Some("rs") => "rust",
        Some("toml") => "toml",
        Some("ts") => "typescript",
        Some("yaml" | "yml") => "yaml",
        _ => "other",
    }
}

fn nonempty_lines(output: &str) -> Vec<&str> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

fn compare<T>(label: &str, actual: &T, expected: &T) -> Result<(), DesktopScrollFixtureError>
where
    T: PartialEq + std::fmt::Debug + ?Sized,
{
    if actual == expected {
        return Ok(());
    }
    Err(invalid(format!(
        "{label} is {actual:?}, expected {expected:?}"
    )))
}
