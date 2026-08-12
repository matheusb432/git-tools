use gtl_models::diffs::Commit;

use crate::diffs::{Cmd, Foot, View};

pub(crate) const DIFF_SINGLE_FILE: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

pub(crate) fn commit(sha: &str) -> Commit {
    Commit {
        sha: sha.to_owned(),
        subject: "feat: work".into(),
        ..Default::default()
    }
}

pub(crate) fn view() -> View {
    View {
        repo_name: "repo".into(),
        repo_root: "/repo".into(),
        branch: "feature".into(),
        upstream: "origin/main".into(),
        commits: Vec::new(),
        files: Vec::new(),
        title: "Diff".into(),
        cmd: Cmd {
            lead: String::new(),
            range: String::new(),
            trail: String::new(),
        },
        commits_label: "Commits".into(),
        foot: Foot {
            cmd: "git diff".into(),
        },
        exclusions: None,
    }
}
