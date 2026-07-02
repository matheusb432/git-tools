use std::path::Path;

use contracts::{
    diffs::DiffTargetDto,
    envelope::{Note, NoteLevel},
};

use crate::cli::DiffTarget;

pub mod daemon_ctl;
pub mod diff;
pub mod diff_subrepos;
mod discover;
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

/// Open an artifact according to the user's `viewer` config. `app` spawns the
/// desktop viewer detached on the `diff://` url; if the app or a display is
/// missing it degrades to the browser. Best-effort — never fails the command.
pub(crate) fn open_artifact(path: &Path) {
    use crate::viewer::{
        ViewerAction, diff_url_from_path, is_no_open, resolve_viewer_action, resolve_viewer_bin,
    };
    let has_display =
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    let no_open = is_no_open(std::env::var("GIT_TOOLS_NO_OPEN").ok().as_deref());
    let action = resolve_viewer_action(crate::config::load().diff.viewer, has_display, no_open);
    match action {
        ViewerAction::Nothing => {}
        ViewerAction::Browser => gtl_platform::open_in_browser(path),
        ViewerAction::SpawnApp => {
            match (resolve_viewer_bin(), diff_url_from_path(path)) {
                (Some(bin), Some(url)) => {
                    if gtl_platform::spawn_detached(&bin, &[url.as_str()]).is_err() {
                        gtl_platform::open_in_browser(path); // spawn failed → browser
                    }
                }
                // Viewer not installed or unparseable path → browser fallback.
                _ => gtl_platform::open_in_browser(path),
            }
        }
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
pub(crate) fn error_text(notes: &[Note]) -> String {
    notes
        .iter()
        .rev()
        .find(|n| n.level == NoteLevel::Error)
        .map_or_else(
            || "daemon reported an error".to_string(),
            |n| n.text.clone(),
        )
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
}
