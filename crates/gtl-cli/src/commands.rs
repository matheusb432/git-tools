use std::path::{Path, PathBuf};

use anyhow::Context as _;
use gtl_application::{
    diffs::present_diff::{PresentDiff, PresentDiffOk},
    ports::FileSystemClient,
};
use gtl_contracts::envelope::{Note, NoteLevel};

pub mod daemon_ctl;
pub mod diff;
pub mod diff_live;
pub mod diff_subrepos;
pub mod managed;
pub mod merge_diff;
pub mod prune;
pub mod push_subrepos;
pub mod squash_preview;
pub mod sync;
pub mod tag;
pub mod worktree;

pub(crate) fn canonical_working_directory() -> anyhow::Result<PathBuf> {
    gtl_infra::file_system::LocalFileSystemClient
        .canonical_working_directory()
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

/// Forward one recipe batch to the single-instance viewer as one argv token.
pub(crate) fn forward_recipes(batch: &gtl_contracts::recipes::OpenRecipes) -> anyhow::Result<()> {
    use crate::viewer::{no_open_requested, resolve_viewer_bin};

    if no_open_requested() {
        return Ok(());
    }
    let bin = resolve_viewer_bin()
        .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
    let token = gtl_contracts::recipes::encode_token(batch)
        .context("failed to encode the viewer recipe batch")?;
    gtl_infra::detached_process::spawn(&bin, &[token.as_str()])
        .context("failed to spawn gtl-viewer to forward the recipe batch")
}

/// Renders `path` as a `file://` URL for the terminal. Not full RFC 8089
/// percent-encoding — store artifact paths are built from repo names/content
/// hashes, never arbitrary user input — just forward-slash normalization so a
/// Windows-style `C:\...` path (Git Bash) still yields a well-formed URL.
fn file_url(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(rest) = normalized.strip_prefix('/') {
        format!("file:///{rest}")
    } else {
        format!("file:///{normalized}")
    }
}

pub(crate) fn present(command: PresentDiff) -> anyhow::Result<diff::DiffOutcome> {
    let outcome = gtl_application::diffs::present_diff::execute(
        command,
        &crate::diff_viewer_client::CliDiffViewerClient,
        &gtl_infra::git_client::HybridGitClient,
    )?;
    Ok(finish_presentation(outcome))
}

fn finish_presentation(outcome: PresentDiffOk) -> diff::DiffOutcome {
    crate::diff_viewer_client::print_notes(&outcome.notes);
    match (outcome.surface, outcome.artifact) {
        (gtl_application::diffs::present_diff::DiffSurface::Viewer, None) => {
            diff::DiffOutcome::Forwarded
        }
        (_, Some(artifact)) => {
            println!("{}", file_url(&artifact));
            diff::DiffOutcome::Rendered(artifact)
        }
        (_, None) => diff::DiffOutcome::Empty,
    }
}

/// Print a daemon envelope's wire [`Note`](gtl_contracts::envelope::Note)s: `Info` to
/// stdout, `Warn` to stderr, verbatim. `Error` notes are skipped — the caller
/// turns them into the returned error so the exit path prints them once.
pub(crate) fn print_wire_notes(notes: &[gtl_contracts::envelope::Note]) {
    for note in notes {
        match note.level {
            NoteLevel::Info => println!("{}", note.text),
            NoteLevel::Warn => eprintln!("{}", note.text),
            NoteLevel::Error => {}
        }
    }
}

/// The CLI's exit path prints `{err:#}` to stderr — hand it the service-composed
/// error text so output stays byte-identical to the pre-daemon local path.
///
/// Prefers the last `Error` note (the regular `diff`/`diff --raw` daemon-error path,
/// where a genuine handler `Err` is mapped to an `Error`-level note). Some outcomes —
/// notably a `live_views/save` rejection — carry the human message as a `Warn` note
/// under an `Outcome::Error` (the application layer has no `Error` note level), so fall
/// back to the last `Warn` note before the generic default rather than dropping the real
/// message on the floor.
pub(crate) fn error_text(notes: &[Note]) -> String {
    let by_level = |level: NoteLevel| notes.iter().rev().find(move |n| n.level == level);
    by_level(NoteLevel::Error)
        .or_else(|| by_level(NoteLevel::Warn))
        .map_or_else(
            || "daemon reported an error".to_string(),
            |n| n.text.clone(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_url_builds_a_triple_slash_url_for_a_unix_absolute_path() {
        assert_eq!(
            file_url(Path::new(
                "/home/user/.local/share/git-tools/diffs/r/h.html"
            )),
            "file:///home/user/.local/share/git-tools/diffs/r/h.html"
        );
    }

    #[test]
    fn file_url_normalizes_a_windows_style_path() {
        assert_eq!(
            file_url(Path::new(
                r"C:\Users\me\AppData\Local\git-tools\diffs\r\h.html"
            )),
            "file:///C:/Users/me/AppData/Local/git-tools/diffs/r/h.html"
        );
    }

    #[test]
    fn error_text_finds_the_last_error_note() {
        assert_eq!(
            error_text(&[
                Note {
                    level: NoteLevel::Info,
                    text: "info".into(),
                },
                Note {
                    level: NoteLevel::Error,
                    text: "boom".into(),
                },
            ]),
            "boom"
        );
        assert_eq!(error_text(&[]), "daemon reported an error");
    }

    #[test]
    fn error_text_prefers_error_over_warn() {
        // The regular diff daemon-error path carries both a Warn and an Error note;
        // the Error note must win so that path's message is unchanged.
        assert_eq!(
            error_text(&[
                Note {
                    level: NoteLevel::Warn,
                    text: "just a warning".into(),
                },
                Note {
                    level: NoteLevel::Error,
                    text: "the real error".into(),
                },
            ]),
            "the real error"
        );
    }

    #[test]
    fn error_text_falls_back_to_the_last_warn_when_no_error_note() {
        // A live-view rejection rides as a Warn note under Outcome::Error; without this
        // fallback the CLI would drop the real message for the generic default.
        assert_eq!(
            error_text(&[
                Note {
                    level: NoteLevel::Info,
                    text: "info".into(),
                },
                Note {
                    level: NoteLevel::Warn,
                    text: "The directory `/x` is not a git repository.".into(),
                },
            ]),
            "The directory `/x` is not a git repository."
        );
    }
}
