use std::time::Instant;

use gix::bstr::ByteSlice as _;
use gtl_application::{
    ports::GitClient as _, viewer::search_viewer_commits::ActiveBranchCommitReader,
};
use gtl_models::{
    diffs::Commit,
    git::{GitHead, GitHeadState},
    paths::RepositoryRoot,
    timestamps::MachineTimestamp,
};

impl ActiveBranchCommitReader for super::HybridGitClient {
    fn visit_commits(
        &self,
        path: &RepositoryRoot,
        deadline: Instant,
        visitor: &mut dyn FnMut(Commit),
    ) -> anyhow::Result<GitHead> {
        let state = self.head_state(path)?;
        let (head, id) = match state {
            GitHeadState::Unborn { branch } => return Ok(GitHead::Branch(branch)),
            GitHeadState::Commit {
                head: GitHead::Detached,
                ..
            } => return Ok(GitHead::Detached),
            GitHeadState::Commit { head, id } => (head, id),
        };
        let repository = gix::open(path.as_ref())?;
        let tip = repository.find_commit(gix::ObjectId::from_hex(id.as_ref().as_bytes())?)?;
        for entry in tip.ancestors().all()? {
            anyhow::ensure!(
                Instant::now() < deadline,
                "commit search exceeded its deadline"
            );
            let entry = entry?;
            let object = entry.object()?;
            let message = object.message_raw()?.to_str_lossy();
            let (subject, body) = message.split_once('\n').unwrap_or((&message, ""));
            visitor(Commit {
                id: entry.id.to_string().try_into()?,
                subject: subject.to_owned(),
                body: body.trim().to_owned(),
                committed_at: MachineTimestamp::from_unix_seconds(
                    object.author()?.time()?.seconds,
                )?,
                parents: entry
                    .parent_ids
                    .iter()
                    .map(|id| id.to_string().try_into())
                    .collect::<Result<_, _>>()?,
            });
        }
        Ok(head)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestRepository;

    #[test]
    fn search_visits_only_local_active_branch_ancestry_and_handles_unborn_and_detached_heads() {
        let repository = TestRepository::new();
        let git = super::super::HybridGitClient;
        let mut commits = Vec::new();
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        git.visit_commits(&repository.root(), deadline, &mut |commit| {
            commits.push(commit);
        })
        .unwrap();
        assert_eq!(commits, Vec::<gtl_models::diffs::Commit>::new());
        repository.git(&["commit", "--allow-empty", "-m", "shared ancestor"]);
        repository.git(&["checkout", "-b", "other"]);
        repository.git(&["commit", "--allow-empty", "-m", "other branch"]);
        repository.git(&["update-ref", "refs/remotes/origin/other", "HEAD"]);
        repository.git(&["tag", "other-tag"]);
        repository.git(&["checkout", "main"]);
        repository.git(&["commit", "--allow-empty", "-m", "active branch"]);
        git.visit_commits(&repository.root(), deadline, &mut |commit| {
            commits.push(commit);
        })
        .unwrap();
        assert_eq!(
            commits
                .iter()
                .map(|commit| commit.subject.as_str())
                .collect::<Vec<_>>(),
            ["active branch", "shared ancestor"]
        );
        commits.clear();
        repository.git(&["checkout", "--detach"]);
        git.visit_commits(&repository.root(), deadline, &mut |commit| {
            commits.push(commit);
        })
        .unwrap();
        assert_eq!(commits, Vec::<gtl_models::diffs::Commit>::new());
    }
}
