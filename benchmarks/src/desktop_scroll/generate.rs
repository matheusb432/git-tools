use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::{
    BASE_SUBJECT, BASE_TIMESTAMP, COMMIT_COUNT, DesktopScrollFixtureError,
    DesktopScrollFixtureEvidence, DesktopScrollManifest, FIXTURE_INPUT_BYTES_MAX, FixtureBase,
    FixtureBounds, FixtureCommit, FixtureIdentity, FixtureWorkload, GIT_RANGE, IDENTITY_EMAIL,
    IDENTITY_NAME, PATCH_FILE_BYTES_MAX, file_system_error, invalid,
    repository::{FixtureRepository, copy_tree},
    sha256_hex,
    verify::{inspect_text_tree, measure_repository},
};

const FIXTURE_README: &str = r"# Desktop scroll fixture

This immutable synthetic Git workload reconstructs one base commit followed by exactly ten measured commits. The base snapshot retains full source context, and `patches/` contains the compact applyable commit series.

Regenerate it only with `cargo run --quiet -p xtask -- desktop-scroll-fixture`. The command runs under repository-owned resource bounds, generates two independent candidates, requires byte-for-byte equality, hydrates the result without network access, and verifies `manifest.toml` before replacing these files.
";

#[derive(Clone, Copy)]
enum FixtureFileKind {
    Html,
    JavaScript,
    Lockfile,
    Markdown,
    Rust,
    Toml,
    TypeScript,
    Yaml,
}

impl FixtureFileKind {
    fn line(self, line_number: usize, revision: usize) -> String {
        match self {
            Self::Html => format!(
                "<div data-row=\"{line_number:04}\">fixture-{line_number:04}-r{revision:02}</div>"
            ),
            Self::JavaScript => format!(
                "export const row{line_number:04} = \"fixture-{line_number:04}-r{revision:02}\";"
            ),
            Self::Lockfile => {
                format!("# deterministic lock row {line_number:04} revision {revision:02}")
            }
            Self::Markdown => {
                format!("- Fixture row {line_number:04}, revision {revision:02}.")
            }
            Self::Rust => format!(
                "pub const ROW_{line_number:04}: &str = \"fixture-{line_number:04}-r{revision:02}\";"
            ),
            Self::Toml => {
                format!("row_{line_number:04} = \"fixture-{line_number:04}-r{revision:02}\"")
            }
            Self::TypeScript => format!(
                "export const row{line_number:04}: string = \"fixture-{line_number:04}-r{revision:02}\";"
            ),
            Self::Yaml => format!("row_{line_number:04}: fixture-{line_number:04}-r{revision:02}"),
        }
    }
}

#[derive(Clone, Copy)]
struct FixtureFileSpec {
    path: &'static str,
    kind: FixtureFileKind,
    line_count: usize,
}

const EXISTING_FILES: [FixtureFileSpec; 25] = [
    existing(
        "crates/viewer/src/workspace/scroll_state.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "docs/viewer/old-scroll-protocol.md",
        FixtureFileKind::Markdown,
        700,
    ),
    existing(
        "crates/viewer/src/workspace/files_panel.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/viewer/src/workspace/commits_panel.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/viewer/src/workspace/document.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/viewer/src/workspace/paging.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/viewer/src/workspace/obsolete_cache.rs",
        FixtureFileKind::Rust,
        80,
    ),
    existing(
        "docs/viewer/retired-renderer.md",
        FixtureFileKind::Markdown,
        80,
    ),
    existing("config/retired-viewer.toml", FixtureFileKind::Toml, 80),
    existing(
        "crates/parser/src/languages/rust.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/parser/src/languages/markdown.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/parser/src/languages/toml.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/application/src/diffs/assemble.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/application/src/viewer/project.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/desktop/src/bridge/diff.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing(
        "crates/desktop/src/session/cache.rs",
        FixtureFileKind::Rust,
        700,
    ),
    existing("crates/wire/src/viewer.rs", FixtureFileKind::Rust, 700),
    existing("config/viewer/defaults.toml", FixtureFileKind::Toml, 700),
    existing("config/viewer/themes.toml", FixtureFileKind::Toml, 700),
    existing("docs/viewer/rendering.md", FixtureFileKind::Markdown, 700),
    existing("docs/viewer/paging.md", FixtureFileKind::Markdown, 700),
    existing("docs/benchmarks/desktop.md", FixtureFileKind::Markdown, 700),
    existing(
        "web/metrics/frame_sampler.ts",
        FixtureFileKind::TypeScript,
        700,
    ),
    existing(
        "web/metrics/scroll_runner.js",
        FixtureFileKind::JavaScript,
        700,
    ),
    existing("web/fixtures/viewer.html", FixtureFileKind::Html, 700),
];

const ADDED_FILES: [FixtureFileSpec; 25] = [
    added(
        "crates/viewer/src/metrics/frame_gap.rs",
        FixtureFileKind::Rust,
    ),
    added(
        "crates/viewer/src/metrics/scroll_sample.rs",
        FixtureFileKind::Rust,
    ),
    added(
        "crates/viewer/src/metrics/distribution.rs",
        FixtureFileKind::Rust,
    ),
    added("crates/viewer/src/metrics/report.rs", FixtureFileKind::Rust),
    added("crates/desktop/src/benchmark/mod.rs", FixtureFileKind::Rust),
    added(
        "crates/desktop/src/benchmark/process_memory.rs",
        FixtureFileKind::Rust,
    ),
    added("crates/fixtures/src/manifest.rs", FixtureFileKind::Rust),
    added("crates/fixtures/src/hydrate.rs", FixtureFileKind::Rust),
    added("crates/viewer/Cargo.toml", FixtureFileKind::Toml),
    added("crates/desktop/Cargo.toml", FixtureFileKind::Toml),
    added("crates/fixtures/Cargo.toml", FixtureFileKind::Toml),
    added(
        "config/benchmarks/desktop-scroll.toml",
        FixtureFileKind::Toml,
    ),
    added(
        "config/benchmarks/resource-bounds.toml",
        FixtureFileKind::Toml,
    ),
    added("docs/benchmarks/workload.md", FixtureFileKind::Markdown),
    added("docs/benchmarks/readiness.md", FixtureFileKind::Markdown),
    added("docs/benchmarks/measurement.md", FixtureFileKind::Markdown),
    added("docs/viewer/changed-files.md", FixtureFileKind::Markdown),
    added("docs/viewer/commits-panel.md", FixtureFileKind::Markdown),
    added("Cargo.lock", FixtureFileKind::Lockfile),
    added("config/scenarios/compact.yaml", FixtureFileKind::Yaml),
    added("config/scenarios/full.yaml", FixtureFileKind::Yaml),
    added(
        ".github/workflows/desktop-scroll.yaml",
        FixtureFileKind::Yaml,
    ),
    added("web/metrics/report.ts", FixtureFileKind::TypeScript),
    added("web/metrics/scroll.js", FixtureFileKind::JavaScript),
    added("web/fixtures/scroll-harness.html", FixtureFileKind::Html),
];

const COMMITS: [CommitSpec; COMMIT_COUNT] = [
    commit(
        "0001-viewer-workspace-foundation.diff",
        "viewer: add deterministic workspace fixture foundations",
        "Create the first synthetic viewer files and exercise both side rails.",
        "2026-01-02T12:00:00+00:00",
    ),
    commit(
        "0002-parser-language-inputs.diff",
        "parser: add representative Rust and Markdown fixture inputs",
        "Extend the source mix without importing repository or author metadata.",
        "2026-01-03T12:00:00+00:00",
    ),
    commit(
        "0003-desktop-measurement-model.diff",
        "desktop: model bounded frame and process measurements",
        "Keep measurement setup separate from the requestAnimationFrame interval.",
        "2026-01-04T12:00:00+00:00",
    ),
    commit(
        "0004-configure-density-scenarios.diff",
        "viewer: configure compact and full-context density scenarios",
        "Use one reconstructed history for both production density choices.",
        "2026-01-05T12:00:00+00:00",
    ),
    commit(
        "0005-document-readiness-boundary.diff",
        "docs: define the desktop viewer readiness boundary",
        "Declare the observable state that remains outside timed scrolling.",
        "2026-01-06T12:00:00+00:00",
    ),
    commit(
        "0006-expand-rendering-workload.diff",
        "viewer: expand rendering work across production modules",
        "Distribute bounded edits through the retained source hierarchy.",
        "2026-01-07T12:00:00+00:00",
    ),
    commit(
        "0007-cover-paging-and-syntax.diff",
        "parser: cover paging and syntax work across large files",
        "Place separated hunks throughout files retained for full context.",
        "2026-01-08T12:00:00+00:00",
    ),
    commit(
        "0008-rename-scroll-contracts.diff",
        "viewer: rename scroll state to the measured journey contract",
        "Retain rename detection while keeping the final sidebar deterministic.",
        "2026-01-09T12:00:00+00:00",
    ),
    commit(
        "0009-remove-retired-rendering-inputs.diff",
        "viewer: remove retired cache and rendering fixture inputs",
        "Exercise deleted paths without adding binary benchmark data.",
        "2026-01-10T12:00:00+00:00",
    ),
    commit(
        "0010-finalize-scroll-reporting.diff",
        "benchmarks: finalize reviewable desktop scroll reporting",
        "Complete the ten-commit range with stable manifest-facing content.",
        "2026-01-11T12:00:00+00:00",
    ),
];

#[derive(Clone, Copy)]
struct CommitSpec {
    patch: &'static str,
    subject: &'static str,
    body: &'static str,
    authored_at: &'static str,
}

struct GeneratedCommit {
    spec: CommitSpec,
    commit_id: String,
    patch_sha256: String,
}

const fn existing(path: &'static str, kind: FixtureFileKind, line_count: usize) -> FixtureFileSpec {
    FixtureFileSpec {
        path,
        kind,
        line_count,
    }
}

const fn added(path: &'static str, kind: FixtureFileKind) -> FixtureFileSpec {
    FixtureFileSpec {
        path,
        kind,
        line_count: 55,
    }
}

const fn commit(
    patch: &'static str,
    subject: &'static str,
    body: &'static str,
    authored_at: &'static str,
) -> CommitSpec {
    CommitSpec {
        patch,
        subject,
        body,
        authored_at,
    }
}

pub(super) fn refresh_fixture(
    root: &Path,
) -> Result<DesktopScrollFixtureEvidence, DesktopScrollFixtureError> {
    let parent = root
        .parent()
        .ok_or_else(|| invalid(format!("fixture root has no parent: {}", root.display())))?;
    fs::create_dir_all(parent)
        .map_err(file_system_error("create fixture parent directory", parent))?;
    let first_guard = tempfile::Builder::new()
        .prefix(".desktop-scroll-first-")
        .tempdir_in(parent)
        .map_err(file_system_error(
            "create first fixture staging directory",
            parent,
        ))?;
    let second_guard = tempfile::Builder::new()
        .prefix(".desktop-scroll-second-")
        .tempdir_in(parent)
        .map_err(file_system_error(
            "create second fixture staging directory",
            parent,
        ))?;
    let first = first_guard.path().join("desktop-scroll");
    let second = second_guard.path().join("desktop-scroll");
    generate_candidate(&first)?;
    generate_candidate(&second)?;
    if collect_files(&first)? != collect_files(&second)? {
        return Err(invalid(
            "two independent fixture generations produced different bytes",
        ));
    }
    let staged_evidence = super::verify_fixture(&first)?;

    let backup = first_guard.path().join("previous");
    let had_previous = root.exists();
    if had_previous {
        fs::rename(root, &backup).map_err(file_system_error(
            "stage existing fixture for replacement",
            root,
        ))?;
    }
    if let Err(source) = fs::rename(&first, root) {
        if had_previous {
            let _ = fs::rename(&backup, root);
        }
        return Err(DesktopScrollFixtureError::FileSystem {
            operation: "install generated fixture",
            path: root.to_path_buf(),
            source,
        });
    }
    match super::verify_fixture(root) {
        Ok(installed_evidence) if installed_evidence == staged_evidence => Ok(installed_evidence),
        Ok(_) => {
            restore_previous_fixture(root, &backup, had_previous)?;
            Err(invalid(
                "installed fixture differs from the verified staged fixture",
            ))
        }
        Err(error) => {
            restore_previous_fixture(root, &backup, had_previous)?;
            Err(error)
        }
    }
}

fn restore_previous_fixture(
    root: &Path,
    backup: &Path,
    had_previous: bool,
) -> Result<(), DesktopScrollFixtureError> {
    fs::remove_dir_all(root)
        .map_err(file_system_error("remove invalid generated fixture", root))?;
    if had_previous {
        fs::rename(backup, root).map_err(file_system_error(
            "restore previous fixture after failure",
            backup,
        ))?;
    }
    Ok(())
}

fn generate_candidate(root: &Path) -> Result<(), DesktopScrollFixtureError> {
    let base = root.join("base");
    let patches = root.join("patches");
    fs::create_dir_all(&base)
        .map_err(file_system_error("create generated base directory", &base))?;
    fs::create_dir_all(&patches).map_err(file_system_error(
        "create generated patches directory",
        &patches,
    ))?;
    write_text(&root.join("README.md"), FIXTURE_README)?;
    for file in EXISTING_FILES {
        write_fixture_file(&base, file, 0)?;
    }

    let repository_guard = tempfile::Builder::new()
        .prefix("desktop-scroll-repository-")
        .tempdir()
        .map_err(file_system_error(
            "create generated repository staging directory",
            root,
        ))?;
    let repository_path = repository_guard.path().join("repository");
    let repository = FixtureRepository::initialize(&repository_path)?;
    copy_tree(&base, repository.root())?;
    let base_commit_id = repository.commit_base()?;

    let mut commits = Vec::with_capacity(COMMIT_COUNT);
    let mut patch_series_bytes = 0_u64;
    for (index, commit) in COMMITS.into_iter().enumerate() {
        apply_commit_changes(&repository, index)?;
        let commit_id = repository.commit_all(commit.subject, commit.body, commit.authored_at)?;
        let patch_bytes = repository.format_head_patch()?;
        if patch_bytes.len() as u64 > PATCH_FILE_BYTES_MAX {
            return Err(invalid(format!(
                "generated patch {} has {} bytes, above {PATCH_FILE_BYTES_MAX}",
                commit.patch,
                patch_bytes.len()
            )));
        }
        let patch_path = patches.join(commit.patch);
        fs::write(&patch_path, &patch_bytes).map_err(file_system_error(
            "write generated fixture patch",
            &patch_path,
        ))?;
        patch_series_bytes = patch_series_bytes.saturating_add(patch_bytes.len() as u64);
        commits.push(GeneratedCommit {
            spec: commit,
            commit_id,
            patch_sha256: sha256_hex(&patch_bytes),
        });
    }

    let base_evidence = inspect_text_tree(&base)?;
    let measured = measure_repository(&repository)?;
    if measured.file_touch_counts.len() != commits.len() {
        return Err(invalid(
            "measured commit count differs from generated patches",
        ));
    }
    let manifest = manifest_from_generation(
        base_commit_id,
        base_evidence,
        measured,
        patch_series_bytes,
        commits,
    );
    let manifest_text = toml::to_string_pretty(&manifest)
        .map_err(|source| DesktopScrollFixtureError::EncodeManifest { source })?;
    write_text(
        &root.join("manifest.toml"),
        &(manifest_text.trim_end_matches('\n').to_owned() + "\n"),
    )?;
    Ok(())
}

fn manifest_from_generation(
    base_commit_id: String,
    base_evidence: super::verify::TextTreeEvidence,
    measured: super::verify::MeasuredRepository,
    patch_series_bytes: u64,
    commits: Vec<GeneratedCommit>,
) -> DesktopScrollManifest {
    let commits = commits
        .into_iter()
        .zip(measured.file_touch_counts.iter().copied())
        .enumerate()
        .map(|(index, (commit, file_touch_count))| FixtureCommit {
            sequence: index + 1,
            patch: format!("patches/{}", commit.spec.patch),
            patch_sha256: commit.patch_sha256,
            commit_id: commit.commit_id,
            subject: commit.spec.subject.to_owned(),
            body: commit.spec.body.to_owned(),
            authored_at: commit.spec.authored_at.to_owned(),
            file_touch_count,
        })
        .collect::<Vec<_>>();
    DesktopScrollManifest {
        format_version: 1,
        fixture_name: "desktop-scroll-v1".to_owned(),
        repository_name: "desktop-scroll-fixture".to_owned(),
        git_range: GIT_RANGE.to_owned(),
        identity: FixtureIdentity {
            name: IDENTITY_NAME.to_owned(),
            email: IDENTITY_EMAIL.to_owned(),
        },
        base: FixtureBase {
            commit_id: base_commit_id,
            subject: BASE_SUBJECT.to_owned(),
            authored_at: BASE_TIMESTAMP.to_owned(),
            source_file_count: base_evidence.file_count,
            source_bytes: base_evidence.bytes,
            tree_sha256: base_evidence.sha256,
        },
        workload: FixtureWorkload {
            commit_count: measured.commit_count,
            distinct_file_count: measured.distinct_file_count,
            file_touch_count_total: measured.file_touch_counts.iter().sum(),
            patch_series_bytes,
            compact_diff_rows: measured.compact_diff_rows,
            compact_diff_bytes: measured.compact_diff_bytes,
            full_context_diff_rows: measured.full_context_diff_rows,
            full_context_diff_bytes: measured.full_context_diff_bytes,
            additions: measured.additions,
            deletions: measured.deletions,
            head_commit_id: measured.head_commit_id,
        },
        bounds: FixtureBounds {
            fixture_input_bytes_max: FIXTURE_INPUT_BYTES_MAX,
            patch_file_bytes_max: PATCH_FILE_BYTES_MAX,
        },
        file_types: measured.file_types,
        statuses: measured.statuses,
        commits,
    }
}

fn apply_commit_changes(
    repository: &FixtureRepository,
    commit_index: usize,
) -> Result<(), DesktopScrollFixtureError> {
    if commit_index < 5 {
        for file in &ADDED_FILES[commit_index * 5..commit_index * 5 + 5] {
            write_fixture_file(repository.root(), *file, commit_index + 1)?;
        }
        modify_existing(repository.root(), commit_index * 2, commit_index + 1)?;
        modify_existing(repository.root(), commit_index * 2 + 1, commit_index + 1)?;
        return Ok(());
    }

    match commit_index {
        5 => modify_indices(repository.root(), &[10, 11, 12, 13, 14, 15, 16], 6),
        6 => modify_indices(repository.root(), &[17, 18, 19, 20, 21, 22, 23], 7),
        7 => {
            rename_file(
                repository.root(),
                EXISTING_FILES[0].path,
                "crates/viewer/src/workspace/scroll_journey.rs",
            )?;
            rename_file(
                repository.root(),
                EXISTING_FILES[1].path,
                "docs/viewer/desktop-scroll-protocol.md",
            )?;
            modify_indices(repository.root(), &[24, 2, 3, 4, 5], 8)
        }
        8 => {
            for index in [6, 7, 8] {
                let path = repository.root().join(EXISTING_FILES[index].path);
                fs::remove_file(&path)
                    .map_err(file_system_error("delete generated fixture file", &path))?;
            }
            modify_indices(repository.root(), &[9, 10, 11, 12], 9)
        }
        9 => modify_indices(repository.root(), &[13, 14, 15, 16, 17, 18, 19], 10),
        _ => Err(invalid(format!(
            "commit index {commit_index} exceeds the generated workload"
        ))),
    }
}

fn modify_indices(
    root: &Path,
    indices: &[usize],
    revision: usize,
) -> Result<(), DesktopScrollFixtureError> {
    for index in indices {
        modify_existing(root, *index, revision)?;
    }
    Ok(())
}

fn modify_existing(
    root: &Path,
    file_index: usize,
    revision: usize,
) -> Result<(), DesktopScrollFixtureError> {
    let file = EXISTING_FILES[file_index];
    let path = root.join(file.path);
    let source = fs::read_to_string(&path)
        .map_err(file_system_error("read generated fixture file", &path))?;
    let mut lines = source.lines().map(str::to_owned).collect::<Vec<_>>();
    let hunk_count = 10_usize.min(lines.len() / 4);
    for hunk_index in 0..hunk_count {
        let line_index = (hunk_index + 1) * lines.len() / (hunk_count + 1);
        for offset in 0..2 {
            let index = (line_index + offset).min(lines.len() - 1);
            lines[index] = file.kind.line(index + 1, revision);
        }
    }
    write_text(&path, &(lines.join("\n") + "\n"))
}

fn rename_file(root: &Path, from: &str, to: &str) -> Result<(), DesktopScrollFixtureError> {
    let source = root.join(from);
    let destination = root.join(to);
    let parent = destination
        .parent()
        .ok_or_else(|| invalid(format!("renamed fixture path has no parent: {to}")))?;
    fs::create_dir_all(parent)
        .map_err(file_system_error("create renamed fixture parent", parent))?;
    fs::rename(&source, &destination)
        .map_err(file_system_error("rename generated fixture file", &source))
}

fn write_fixture_file(
    root: &Path,
    file: FixtureFileSpec,
    revision: usize,
) -> Result<(), DesktopScrollFixtureError> {
    let mut text = String::new();
    for line_number in 1..=file.line_count {
        text.push_str(&file.kind.line(line_number, revision));
        text.push('\n');
    }
    write_text(&root.join(file.path), &text)
}

fn write_text(path: &Path, text: &str) -> Result<(), DesktopScrollFixtureError> {
    let parent = path.parent().ok_or_else(|| {
        invalid(format!(
            "generated fixture file has no parent: {}",
            path.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(file_system_error(
        "create generated fixture file parent",
        parent,
    ))?;
    fs::write(path, text).map_err(file_system_error("write generated fixture file", path))
}

fn collect_files(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, DesktopScrollFixtureError> {
    fn collect(
        root: &Path,
        current: &Path,
        files: &mut BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<(), DesktopScrollFixtureError> {
        let entries = fs::read_dir(current).map_err(file_system_error(
            "read generated fixture directory",
            current,
        ))?;
        for entry in entries {
            let entry = entry.map_err(file_system_error(
                "read generated fixture directory entry",
                current,
            ))?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(file_system_error(
                "read generated fixture entry type",
                &path,
            ))?;
            if file_type.is_dir() {
                collect(root, &path, files)?;
            } else if file_type.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| invalid(format!("fixture path escaped root: {}", path.display())))?
                    .to_path_buf();
                let bytes = fs::read(&path)
                    .map_err(file_system_error("read generated fixture file", &path))?;
                files.insert(relative, bytes);
            } else {
                return Err(invalid(format!(
                    "generated fixture contains a non-file entry: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    let mut files = BTreeMap::new();
    collect(root, root, &mut files)?;
    Ok(files)
}
