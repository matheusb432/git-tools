use gtl_models::{
    diffs::{Commit, DiffViewTitle},
    git::GitHead,
    timestamps::MachineTimestamp,
};

use crate::{
    diffs::{Cmd, Foot, View},
    utils::commit_id_fixture,
};

pub(crate) const DIFF_SINGLE_FILE: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

/// Tags a test view with a distinguishable title.
pub(crate) fn view_title(name: &str) -> DiffViewTitle {
    DiffViewTitle::Named {
        name: crate::utils::project_name(name),
    }
}

pub(crate) fn commit(id_prefix: &str) -> Commit {
    commit_with(id_prefix, "feat: work", &[])
}

pub(crate) fn commit_with(id: &str, subject: &str, parents: &[&str]) -> Commit {
    Commit {
        id: commit_id_fixture(id),
        subject: subject.into(),
        body: String::new(),
        committed_at: MachineTimestamp::try_from("2026-01-01T00:00:00Z").unwrap(),
        parents: parents
            .iter()
            .map(|parent| commit_id_fixture(parent))
            .collect(),
    }
}

pub(crate) fn view() -> View {
    View {
        file_filter: crate::diffs::file_filter::DiffFileFilter::default(),
        repo_name: crate::utils::project_name("repo"),
        repo_root: crate::utils::repository_root("/repo"),
        branch: GitHead::Branch(crate::utils::branch_name("feature")),
        upstream: crate::utils::git_revision("origin/main"),
        commits: Vec::new(),
        files: Vec::new(),
        title: DiffViewTitle::Diff,
        cmd: Cmd {
            lead: String::new(),
            range: String::new(),
            trail: String::new(),
        },
        foot: Foot {
            cmd: "git diff".into(),
        },
        full_context: crate::diffs::FullContextDiffState::Unavailable,
        exclusions: None,
    }
}
