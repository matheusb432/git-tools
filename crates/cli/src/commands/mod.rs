use std::path::{Path, PathBuf};

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

pub use application::ports::ArtifactMeta;
pub use domain::diffs::{Mode, Ranges, ranges};

fn repo_name(top: impl AsRef<Path>) -> String {
    top.as_ref()
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("repo")
        .to_string()
}

/// Place a rendered artifact in the central store (never the repo) via the
/// [`ArtifactStore`](application::ports::ArtifactStore) adapter. Returns the
/// artifact path. Idempotent on identical content.
pub(crate) fn store_artifact(meta: &ArtifactMeta, html: &str) -> anyhow::Result<PathBuf> {
    use application::ports::ArtifactStore;

    let store_root = gtl_platform::paths::store_root()?;
    Ok(infra::artifact_store::StoreArtifacts
        .place(&store_root, meta, html)?
        .path)
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

/// Print a slice's [`Note`](application::shared::notes::Note)s: `Info` to stdout,
/// `Warn` to stderr, text verbatim.
pub(crate) fn print_notes(notes: &[application::shared::notes::Note]) {
    for note in notes {
        match note.level {
            application::shared::notes::NoteLevel::Info => println!("{}", note.text),
            application::shared::notes::NoteLevel::Warn => eprintln!("{}", note.text),
        }
    }
}

/// Print a daemon envelope's wire [`Note`](contracts::envelope::Note)s: `Info` to
/// stdout, `Warn` to stderr, verbatim. `Error` notes are skipped — the caller
/// turns them into the returned error so the exit path prints them once.
pub(crate) fn print_wire_notes(notes: &[contracts::envelope::Note]) {
    use contracts::envelope::NoteLevel;
    for note in notes {
        match note.level {
            NoteLevel::Info => println!("{}", note.text),
            NoteLevel::Warn => eprintln!("{}", note.text),
            NoteLevel::Error => {}
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn legacy_count_label(count: usize, noun: &str) -> String {
    format!("{count} {noun}(s)")
}

fn legacy_unpushed_commit_label(count: usize) -> String {
    format!("{count} unpushed commit(s)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_count_label_keeps_node_literal_plural_marker() {
        assert_eq!(legacy_count_label(1, "file"), "1 file(s)");
        assert_eq!(legacy_count_label(2, "commit"), "2 commit(s)");
        assert_eq!(legacy_unpushed_commit_label(1), "1 unpushed commit(s)");
    }
}
