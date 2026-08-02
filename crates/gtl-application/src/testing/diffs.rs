use gtl_models::diffs::Commit;

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
