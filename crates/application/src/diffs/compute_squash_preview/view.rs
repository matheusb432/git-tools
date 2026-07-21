use std::path::Path;

use domain::diffs::AppliedExclusions;

use crate::{
    diffs::{
        Cmd, Foot, PinnedRange, View,
        range::DiffRanges,
        util::{DiffData, assemble, repo_name},
    },
    ports::DiffSource,
};

pub(crate) struct SquashViewBuild {
    pub view: View,
    pub top: String,
    pub log_range: String,
}

pub(super) fn build(
    source: &impl DiffSource,
    cwd: &Path,
    pinned: Option<&PinnedRange>,
    exclusions: &domain::diffs::DiffExclusions,
) -> anyhow::Result<SquashViewBuild> {
    let top = source.top_level(cwd)?;
    let branch = source.current_branch(Path::new(&top))?;
    let repo_name = repo_name(&top);
    let excluded = exclusions.for_project_or_default(&repo_name);

    let (upstream, io_ranges, view_ranges) = if let Some(pin) = pinned {
        (
            pin.display_base(),
            DiffRanges::exact(pin.git_range()),
            DiffRanges::exact(pin.display_range()),
        )
    } else {
        let upstream = source.upstream(Path::new(&top))?;
        let symbolic = DiffRanges::unpushed(&upstream);
        (upstream, symbolic.clone(), symbolic)
    };

    let DiffData {
        commits,
        files,
        hidden_paths,
    } = assemble(
        source,
        Path::new(&top),
        &io_ranges.diff,
        &io_ranges.log,
        excluded,
    )?;

    let view = View {
        repo_name,
        repo_root: top.clone(),
        branch,
        upstream,
        title: "squash-preview".to_string(),
        cmd: Cmd {
            lead: "git log ".to_string(),
            range: view_ranges.log.clone(),
            trail: " --stat".to_string(),
        },
        commits_label: "# commits \u{2014} collapse into 1".to_string(),
        foot: Foot {
            cmd: "squash-local".to_string(),
            note: collapse_note(commits.len()),
        },
        commits,
        files,
        exclusions: AppliedExclusions::from_hidden(excluded, hidden_paths),
    };
    Ok(SquashViewBuild {
        view,
        top,
        log_range: io_ranges.log,
    })
}

fn collapse_note(commit_count: usize) -> String {
    if commit_count == 1 {
        "# would collapse this commit into one \u{2014} read-only preview".to_string()
    } else {
        format!("# would collapse these {commit_count} commits into one \u{2014} read-only preview")
    }
}
