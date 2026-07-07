//! The `render_squash_preview` vertical slice: render the read-only preview of
//! what `squash-local` would collapse the current branch's unpushed commits
//! into, carrying every user-facing message out as [`Note`]s. Mirrors
//! `render_merge_diff`'s shape; the cli's `gtl squash-preview` calls this
//! in-process.

use std::path::{Path, PathBuf};

use domain::diffs::{Cmd, DiffKind, Foot, Mode, View, ranges};

use crate::{
    diffs::util::{DiffData, assemble, repo_name},
    ports::{ArtifactMeta, ArtifactStore, Clock, DiffSource, HtmlRenderer},
    shared::notes::Note,
};

/// Render the squash-preview of the current branch's unpushed commits (base is
/// always the configured upstream) under `store_root`, resolving the repo from
/// `cwd`.
#[derive(Debug, Clone, PartialEq, cqrsy::Command)]
#[command(out = RenderSquashPreviewResponse, err = RenderSquashPreviewError)]
pub struct RenderSquashPreview {
    pub cwd: PathBuf,
    pub store_root: PathBuf,
}

/// The stored artifact plus every message the render wanted surfaced.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderSquashPreviewResponse {
    pub artifact: PathBuf,
    pub reused: bool,
    pub notes: Vec<Note>,
}

/// Everything that can go wrong rendering a squash-preview.
#[derive(Debug, thiserror::Error)]
pub enum RenderSquashPreviewError {
    #[error(transparent)]
    Unexpected(#[from] anyhow::Error),
}

/// Handles [`RenderSquashPreview`] by driving the diff engine through its ports.
#[derive(Clone)]
pub struct RenderSquashPreviewHandler<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> {
    pub source: S,
    pub store: A,
    pub renderer: R,
    pub clock: C,
}

impl<S: DiffSource, A: ArtifactStore, R: HtmlRenderer, C: Clock> cqrsy::Handler<RenderSquashPreview>
    for RenderSquashPreviewHandler<S, A, R, C>
{
    async fn handle(
        &self,
        req: RenderSquashPreview,
    ) -> Result<RenderSquashPreviewResponse, RenderSquashPreviewError> {
        let top = self.source.top_level(&req.cwd)?;
        let upstream = self.source.upstream(Path::new(&top))?;
        let branch = self.source.current_branch(Path::new(&top))?;
        let repo_name = repo_name(&top);
        let ranges = ranges(&upstream, Mode::Unpushed);

        let DiffData { commits, files } = assemble(
            &self.source,
            Path::new(&top),
            &ranges.diff_args,
            &ranges.diff_range,
            &ranges.log_range,
        )?;

        let view = View {
            repo_name: repo_name.clone(),
            repo_root: top.clone(),
            branch,
            upstream: upstream.clone(),
            title: "squash-preview".to_string(),
            cmd: Cmd {
                lead: "git log ".to_string(),
                range: ranges.log_range.clone(),
                trail: " --stat".to_string(),
            },
            commits_label: "# commits — collapse into 1".to_string(),
            foot: Foot {
                cmd: "squash-local".to_string(),
                note: collapse_note(commits.len()),
            },
            commits,
            files,
            theme: None,
        };

        let commit_count = view.commits.len();
        let file_count = view.files.len();
        let html = self.renderer.build_html(&view);

        let meta = ArtifactMeta {
            repo_root: PathBuf::from(&top),
            repo_name: repo_name.clone(),
            // ! WorkTree by design: squash-preview is base→working-tree, not a commit range,
            // ! so it is intentionally excluded from range-dedup in the store.
            kind: DiffKind::WorkTree,
            base_sha: String::new(),
            head_sha: self
                .source
                .resolve_sha(Path::new(&top), "HEAD")
                .unwrap_or_default(),
            range_label: ranges.log_range.clone(),
            head_committed_at: self.source.committed_at(Path::new(&top), "HEAD"),
            generated_at: self.clock.now_iso(),
            title: "squash-preview".to_string(),
        };
        let placed = self.store.place(&req.store_root, &meta, &html)?;

        let notes = vec![
            Note::info(format!(
                "squash-preview: {commit_count} unpushed commit(s), {file_count} file(s)",
            )),
            Note::info(format!("wrote {}", placed.path.display())),
        ];
        Ok(RenderSquashPreviewResponse {
            artifact: placed.path,
            reused: placed.reused,
            notes,
        })
    }
}

fn collapse_note(commit_count: usize) -> String {
    if commit_count == 1 {
        "# would collapse this commit into one — read-only preview".to_string()
    } else {
        format!("# would collapse these {commit_count} commits into one — read-only preview")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cqrsy::send_now;
    use domain::diffs::Commit;

    use super::{RenderSquashPreview, RenderSquashPreviewError, RenderSquashPreviewHandler};
    use crate::{
        shared::notes::Note,
        testing::{FakeDiffSource, FixedClock, InMemoryArtifactStore, StubRenderer},
    };

    const SINGLE_FILE_DIFF: &str = "diff --git a/f.txt b/f.txt\n\
index 111..222 100644\n\
--- a/f.txt\n\
+++ b/f.txt\n\
@@ -1,2 +1,3 @@\n\
 keep\n\
-old line\n\
+new line\n\
+extra line\n";

    fn handler_with(
        source: FakeDiffSource,
    ) -> RenderSquashPreviewHandler<FakeDiffSource, InMemoryArtifactStore, StubRenderer, FixedClock>
    {
        RenderSquashPreviewHandler {
            source,
            store: InMemoryArtifactStore::default(),
            renderer: StubRenderer,
            clock: FixedClock("2026-07-02T00:00:00Z".into()),
        }
    }

    fn one_commit(sha: &str) -> Commit {
        Commit {
            sha: sha.to_string(),
            subject: "feat: work".into(),
            ..Default::default()
        }
    }

    fn req(source_top: &str) -> RenderSquashPreview {
        RenderSquashPreview {
            cwd: PathBuf::from(source_top),
            store_root: PathBuf::from("/store"),
        }
    }

    #[test]
    fn renders_with_the_plural_collapse_note() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: Some("origin/main".into()),
            known_revs: vec!["origin/main".into()],
            commits: vec![one_commit("abc1234"), one_commit("def5678")],
            diff_output: SINGLE_FILE_DIFF.into(),
            ..Default::default()
        };
        let handler = handler_with(source);

        let response = send_now(&(), &handler, req("/repo")).expect("render succeeds");

        assert_eq!(
            response.artifact,
            PathBuf::from("/store/diffs/fake/artifact.html")
        );
        assert!(!response.reused);
        assert_eq!(
            response.notes,
            vec![
                Note::info("squash-preview: 2 unpushed commit(s), 1 file(s)"),
                Note::info("wrote /store/diffs/fake/artifact.html"),
            ]
        );
        let placed = handler.store.placed.lock().unwrap();
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0.title, "squash-preview");
    }

    #[test]
    fn no_upstream_is_an_error() {
        let source = FakeDiffSource {
            top_level: Some("/repo".into()),
            branch: "feature".into(),
            upstream: None,
            ..Default::default()
        };
        let handler = handler_with(source);

        let error = send_now(&(), &handler, req("/repo")).expect_err("missing upstream errors");

        let RenderSquashPreviewError::Unexpected(err) = error;
        assert_eq!(format!("{err:#}"), "no upstream");
    }
}
