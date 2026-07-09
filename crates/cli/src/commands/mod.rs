use std::path::{Path, PathBuf};

use anyhow::Context as _;
use contracts::{
    diffs::{DiffTargetDto, RenderDiffData},
    envelope::{Envelope, Note, NoteLevel, Outcome},
};

use crate::cli::DiffTarget;

pub mod daemon_ctl;
pub mod diff;
pub mod diff_live;
pub mod diff_subrepos;
mod discover;
pub mod git_runner;
pub mod managed;
pub mod merge_diff;
pub mod prune;
pub mod squash_local;
pub mod squash_preview;
pub mod sw;
pub mod sync;
pub mod tag;
pub mod up_subrepos;
pub mod worktree;

fn repo_name(top: impl AsRef<Path>) -> String {
    top.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

/// Announce an artifact from the `--raw` path and the headless degrade fallback — the
/// default render path opens the app via [`forward_recipes`] instead. The desktop GUI
/// never shows raw HTML artifacts, so the terminal's `file://` link is the only way a
/// `--raw`/headless caller learns where the preview landed: always printed to stdout,
/// even under `GIT_TOOLS_NO_OPEN`. Additionally opens the artifact in the OS browser
/// unless `GIT_TOOLS_NO_OPEN` is truthy. Best-effort — never fails the command.
pub(crate) fn open_artifact(path: &Path) {
    use crate::viewer::is_no_open;
    println!("{}", file_url(path));
    if is_no_open(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref()) {
        return;
    }
    gtl_platform::open_in_browser(path);
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

/// Forward a recipe batch to the single-instance viewer as one `gtl-recipe://` argv
/// token — the Phase 5 default render path (`commands::diff`, `commands::diff_subrepos`):
/// no daemon call, no store artifact of our own. Respects `GIT_TOOLS_NO_OPEN`: when set
/// truthy, this resolves nothing further and returns `Ok(())` without spawning.
///
/// # Errors
///
/// Returns an error if the viewer binary can't be resolved (`gtl-viewer` not installed
/// next to this exe or on `PATH`) or the detached spawn fails — the caller decides
/// whether/how to degrade.
pub(crate) fn forward_recipes(batch: &gtl_recipe::OpenRecipes) -> anyhow::Result<()> {
    use crate::viewer::{is_no_open, resolve_viewer_bin};

    if is_no_open(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref()) {
        return Ok(());
    }
    let bin = resolve_viewer_bin()
        .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
    let token = gtl_recipe::encode_token(batch);
    gtl_platform::spawn_detached(&bin, &[token.as_str()])
        .context("failed to spawn gtl-viewer to forward the recipe batch")
}

/// Low-noise stderr note emitted when the default (app) render path can't launch the
/// viewer and degrades to the raw/browser path, i.e. `open_artifact` (FSD A-0002:
/// header-less machines have no viewer, and the CLI must never fail for that).
pub(crate) fn note_viewer_degrade(err: &anyhow::Error) {
    eprintln!("diff: viewer unavailable ({err:#}); rendering via the browser instead");
}

/// Print a daemon envelope's wire [`Note`](contracts::envelope::Note)s: `Info` to
/// stdout, `Warn` to stderr, verbatim. `Error` notes are skipped — the caller
/// turns them into the returned error so the exit path prints them once.
pub(crate) fn print_wire_notes(notes: &[contracts::envelope::Note]) {
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

/// Finish a single-artifact render: print the envelope's wire notes, then on a
/// successful outcome open the artifact and return its path; any non-`Ok` outcome
/// becomes the service-composed error. Shared by `merge-diff` and `squash-preview`,
/// whose success path is identical (both auto-open the freshly written artifact).
pub(crate) fn finish_single_render(envelope: Envelope<RenderDiffData>) -> anyhow::Result<PathBuf> {
    print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            open_artifact(&artifact);
            Ok(artifact)
        }
        _ => Err(anyhow::anyhow!(error_text(&envelope.notes))),
    }
}

/// Map a [`DiffTarget`] onto its wire DTO.
pub(crate) fn to_target_dto(target: &DiffTarget) -> DiffTargetDto {
    match target {
        DiffTarget::Unpushed => DiffTargetDto::Unpushed,
        DiffTarget::Base(rev) => DiffTargetDto::Base { rev: rev.clone() },
        DiffTarget::Range(range) => DiffTargetDto::Range {
            range: range.clone(),
        },
        DiffTarget::Merge(base) => DiffTargetDto::Merge { base: base.clone() },
        DiffTarget::Last(count) => DiffTargetDto::Last { count: count.get() },
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn to_target_dto_maps_every_variant() {
        assert_eq!(
            to_target_dto(&DiffTarget::Unpushed),
            DiffTargetDto::Unpushed
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Base("abc".into())),
            DiffTargetDto::Base { rev: "abc".into() }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Range("a..b".into())),
            DiffTargetDto::Range {
                range: "a..b".into()
            }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Merge("main".into())),
            DiffTargetDto::Merge {
                base: "main".into()
            }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Last(NonZeroU32::new(3).unwrap())),
            DiffTargetDto::Last { count: 3 }
        );
    }

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
