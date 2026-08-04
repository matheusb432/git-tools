use gtl_models::diffs::ExcludedExtensions;

use crate::{
    diffs::{FileDiff, View},
    shared::notes::Note,
};

pub(crate) fn filter_excluded_files(
    files: Vec<FileDiff>,
    excluded: &ExcludedExtensions,
) -> (Vec<FileDiff>, Vec<String>) {
    if excluded.is_empty() {
        return (files, Vec::new());
    }
    let (hidden, kept): (Vec<FileDiff>, Vec<FileDiff>) = files
        .into_iter()
        .partition(|file| excluded.matches(&file.path));
    (kept, hidden.into_iter().map(|file| file.path).collect())
}

pub(crate) fn note(label: &str, view: &View) -> Option<Note> {
    view.exclusions.as_ref().map(|applied| {
        Note::info(format!(
            "{label}: {} file(s) hidden by config [diff.exclude] ({})",
            applied.hidden_paths.len(),
            applied.extensions_label(),
        ))
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::{AppliedExclusions, ExcludedExtensions};

    use super::{filter_excluded_files, note};
    use crate::diffs::{Cmd, FileDiff, Foot, View};

    #[test]
    fn filtering_returns_visible_files_and_hidden_paths_in_diff_order() {
        let files = vec![
            FileDiff {
                path: "src/main.rs".into(),
                added: 1,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
            FileDiff {
                path: "Cargo.lock".into(),
                added: 1,
                removed: 0,
                lines: Vec::new(),
                full_lines: None,
            },
        ];
        let excluded = ExcludedExtensions::new([".lock"]);

        let (visible, hidden) = filter_excluded_files(files, &excluded);

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].path, "src/main.rs");
        assert_eq!(hidden, ["Cargo.lock"]);
    }

    #[test]
    fn note_describes_the_applied_exclusion() {
        let view = View {
            repo_name: "repo".into(),
            repo_root: "/repo".into(),
            branch: "main".into(),
            upstream: "origin/main".into(),
            commits: Vec::new(),
            files: Vec::new(),
            title: "diff".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: "commits".into(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
            exclusions: Some(AppliedExclusions {
                hidden_paths: vec!["Cargo.lock".into()],
                extensions: vec!["lock".into()],
            }),
        };

        assert_eq!(
            note("diff-preview", &view).map(|note| note.text),
            Some("diff-preview: 1 file(s) hidden by config [diff.exclude] (lock)".into())
        );
    }
}
