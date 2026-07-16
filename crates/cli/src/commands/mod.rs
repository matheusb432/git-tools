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
pub mod managed;
pub mod merge_diff;
pub mod prune;
pub mod squash_preview;
pub mod sync;
pub mod tag;
pub mod worktree;

/// Announce an artifact on the `--raw` path and the headless/viewer-unavailable
/// fallback. The browser
/// shows raw HTML artifacts, so the terminal's `file://` link is the only way a
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

/// Consume a rendered artifact without opening it. Used only after
/// `GIT_TOOLS_NO_OPEN` selected the daemon/store compatibility path.
pub(crate) fn do_not_open(_path: &Path) {}

/// Forward one recipe batch to the single-instance viewer as one argv token.
pub(crate) fn forward_recipes(batch: &gtl_recipe::OpenRecipes) -> anyhow::Result<()> {
    use crate::viewer::{no_open_requested, resolve_viewer_bin};

    if no_open_requested() {
        return Ok(());
    }
    let bin = resolve_viewer_bin()
        .context("gtl-viewer is not installed; cannot forward the recipe batch")?;
    let token = gtl_recipe::encode_token(batch);
    gtl_platform::spawn_detached(&bin, &[token.as_str()])
        .context("failed to spawn gtl-viewer to forward the recipe batch")
}

pub(crate) fn note_viewer_degrade(error: &anyhow::Error) {
    eprintln!("diff: viewer unavailable ({error:#}); rendering via the browser instead");
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
/// successful outcome hand the artifact to `open` and return its path; any non-`Ok`
/// outcome becomes the service-composed error. `open` is the caller's chosen artifact
/// effect: browser open or an intentional no-op. Shared by `diff merge` and `diff
/// squash`, whose success path is identical.
pub(crate) fn finish_single_render(
    envelope: Envelope<RenderDiffData>,
    open: impl FnOnce(&Path),
) -> anyhow::Result<PathBuf> {
    print_wire_notes(&envelope.notes);
    match envelope.outcome {
        Outcome::Ok => {
            let data = envelope.data.context("daemon returned ok without data")?;
            let artifact = PathBuf::from(data.artifact);
            open(&artifact);
            Ok(artifact)
        }
        _ => Err(anyhow::anyhow!(error_text(&envelope.notes))),
    }
}

/// Map a [`DiffTarget`] onto its wire DTO.
pub(crate) fn to_target_dto(target: &DiffTarget) -> DiffTargetDto {
    match target {
        DiffTarget::Unpushed { .. } => DiffTargetDto::Unpushed,
        DiffTarget::Base(rev) => DiffTargetDto::Base { rev: rev.clone() },
        DiffTarget::Range { range, .. } => DiffTargetDto::Range {
            range: range.clone(),
        },
        DiffTarget::Merge { base, .. } => DiffTargetDto::Merge { base: base.clone() },
        DiffTarget::Last { count, .. } => DiffTargetDto::Last { count: count.get() },
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;

    #[test]
    fn to_target_dto_maps_every_variant() {
        assert_eq!(
            to_target_dto(&DiffTarget::Unpushed { pinned: None }),
            DiffTargetDto::Unpushed
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Base("abc".into())),
            DiffTargetDto::Base { rev: "abc".into() }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Range {
                range: "a..b".into(),
                pinned: None
            }),
            DiffTargetDto::Range {
                range: "a..b".into()
            }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Merge {
                base: "main".into(),
                pinned: None
            }),
            DiffTargetDto::Merge {
                base: "main".into()
            }
        );
        assert_eq!(
            to_target_dto(&DiffTarget::Last {
                count: NonZeroU32::new(3).unwrap(),
                pinned: None
            }),
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
