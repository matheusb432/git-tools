//! Gathers the current Git state for one exact tag-bump proposal.

use std::path::PathBuf;

use gtl_models::{
    diffs::CommitId,
    git::{GitHead, GitRevision, TagName},
    paths::RepositoryRoot,
    tags::{Tag, TagPatternName, TagSlot, TagTemplate},
};

use super::{BumpLevel, git_command_error::GitCommandError, version::decide_tag_version};
use crate::ports::{GitClient, GitEffect, UserSettingsLoadError, UserSettingsReader};

/// Requests a tag-bump proposal without changing Git state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunTagBump {
    pub repo_path: PathBuf,
    pub pattern: Option<TagPatternName>,
    pub level: BumpLevel,
    pub message: String,
    pub push: bool,
}

/// Requests a tag-bump proposal for an already-resolved repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRunResolvedTagBump {
    pub repo_root: RepositoryRoot,
    pub pattern: Option<TagPatternName>,
    pub level: BumpLevel,
    pub message: String,
    pub push: bool,
}

/// The exact mutation displayed before a tag bump is confirmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagBumpPreview {
    pub repo_path: RepositoryRoot,
    pub branch: GitHead,
    pub target_id: CommitId,
    pub pattern: TagPatternName,
    pub template: TagTemplate,
    pub slot: TagSlot,
    pub base_tag: Option<TagName>,
    pub next_tag: TagName,
    pub message: String,
    pub push: bool,
}

/// Result of gathering a tag-bump proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DryRunTagBumpOk {
    Ready(TagBumpPreview),
    Rejected { detail: String },
}

/// Reports a failure outside the proposal's own decision.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DryRunTagBumpError {
    #[error(transparent)]
    Settings(#[from] UserSettingsLoadError),
    #[error("{source}")]
    Unexpected {
        #[source]
        source: anyhow::Error,
    },
}

/// Gathers the repository state needed to describe one exact tag mutation.
#[cqrsy::query]
pub fn execute(
    query: DryRunTagBump,
    git: &impl GitClient,
    settings: &impl UserSettingsReader,
) -> Result<DryRunTagBumpOk, DryRunTagBumpError> {
    let DryRunTagBump {
        repo_path,
        pattern,
        level,
        message,
        push,
    } = query;
    if !repo_path.is_absolute() {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: "tag bump repository path must be absolute".into(),
        });
    }
    let Some(repo_root) = git
        .discover_top(&repo_path)
        .map_err(|source| DryRunTagBumpError::Unexpected { source })?
    else {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: format!("{} is not inside a Git repository", repo_path.display()),
        });
    };
    evaluate_preview(
        &repo_root,
        pattern.as_ref(),
        level,
        &message,
        push,
        git,
        settings,
    )
}

/// Gathers a proposal without discarding a resolved repository identity.
pub fn execute_resolved(
    query: DryRunResolvedTagBump,
    git: &impl GitClient,
    settings: &impl UserSettingsReader,
) -> Result<DryRunTagBumpOk, DryRunTagBumpError> {
    let DryRunResolvedTagBump {
        repo_root,
        pattern,
        level,
        message,
        push,
    } = query;
    let Some(repo_root) = git
        .discover_top(repo_root.as_ref())
        .map_err(|source| DryRunTagBumpError::Unexpected { source })?
    else {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: format!("{} is not inside a Git repository", repo_root.display()),
        });
    };
    evaluate_preview(
        &repo_root,
        pattern.as_ref(),
        level,
        &message,
        push,
        git,
        settings,
    )
}

fn evaluate_preview(
    repo_root: &RepositoryRoot,
    pattern: Option<&TagPatternName>,
    level: BumpLevel,
    message: &str,
    push: bool,
    git: &impl GitClient,
    settings: &impl UserSettingsReader,
) -> Result<DryRunTagBumpOk, DryRunTagBumpError> {
    if message.trim().is_empty() {
        return Ok(DryRunTagBumpOk::Rejected {
            detail: "tag message is required".into(),
        });
    }
    let user_settings = settings.load()?;
    let selected = user_settings
        .tag_patterns()
        .for_project_or_default(&repo_root.project_name())
        .select(pattern);
    let (pattern, template) = match selected {
        Ok(selected) => selected,
        Err(error) => {
            return Ok(DryRunTagBumpOk::Rejected {
                detail: error.to_string(),
            });
        }
    };
    match read_preview(repo_root, pattern, template, level, message, push, git) {
        Ok(Ok(preview)) => Ok(DryRunTagBumpOk::Ready(preview)),
        Ok(Err(detail)) | Err(GitCommandError::Rejected { detail, .. }) => {
            Ok(DryRunTagBumpOk::Rejected { detail })
        }
        Err(GitCommandError::Transport { source, .. }) => {
            Err(DryRunTagBumpError::Unexpected { source })
        }
    }
}

fn read_preview(
    repo_path: &RepositoryRoot,
    pattern: &TagPatternName,
    template: &TagTemplate,
    level: BumpLevel,
    message: &str,
    push: bool,
    git: &impl GitClient,
) -> Result<Result<TagBumpPreview, String>, GitCommandError> {
    let local_tags = local_tags(git, repo_path)?;
    let decision = match decide_tag_version(local_tags.keys().map(AsRef::as_ref), template, level) {
        Ok(decision) => decision,
        Err(rejection) => return Ok(Err(rejection.to_string())),
    };
    let branch = git.current_branch(repo_path)?;
    let target_id = git.resolve_commit_id(repo_path, &GitRevision::head())?;

    Ok(Ok(TagBumpPreview {
        repo_path: repo_path.clone(),
        branch,
        target_id,
        pattern: pattern.clone(),
        template: template.clone(),
        slot: decision.slot,
        base_tag: decision.base_tag,
        next_tag: decision.next_tag,
        message: message.to_owned(),
        push,
    }))
}

fn local_tags(
    git: &impl GitClient,
    repo_path: &RepositoryRoot,
) -> Result<std::collections::BTreeMap<TagName, Tag>, GitCommandError> {
    match git.local_tags(repo_path)? {
        GitEffect::Applied(tags) => Ok(tags),
        GitEffect::Rejected(detail) => Err(GitCommandError::rejected(format!(
            "git for-each-ref failed: {detail}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        paths::ProjectName,
        tags::{TagPatternName, TagPatternSet, TagPatternSettings, TagSlot},
    };

    use super::{DryRunTagBump, DryRunTagBumpOk};
    use crate::{
        tags::{BumpLevel, dry_run_tag_bump},
        utils::{FixedUserSettingsStore, ScriptedGitClient, default_user_settings},
    };

    fn scripted_repo(tags: &str) -> ScriptedGitClient {
        ScriptedGitClient::new(vec![
            ScriptedGitClient::applied("/repo/sample_project"),
            ScriptedGitClient::applied(tags),
            ScriptedGitClient::applied("main"),
            ScriptedGitClient::applied("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        ])
    }

    fn sample_project_settings() -> FixedUserSettingsStore {
        let patterns = TagPatternSet::try_new(
            [
                (
                    TagPatternName::try_new("dev").unwrap(),
                    "{major}.{minor}.{patch}".parse().unwrap(),
                ),
                (
                    TagPatternName::try_new("release").unwrap(),
                    "release-{major}.{minor}.{patch}".parse().unwrap(),
                ),
            ],
            Some(TagPatternName::try_new("dev").unwrap()),
        )
        .unwrap();
        FixedUserSettingsStore::new(default_user_settings().with_tag_patterns(
            TagPatternSettings::new(
                None,
                [(ProjectName::try_new("sample_project".to_owned()).unwrap(), patterns)],
            ),
        ))
    }

    fn query(pattern: Option<&str>) -> DryRunTagBump {
        DryRunTagBump {
            repo_path: "/repo/sample_project".into(),
            pattern: pattern.map(|name| TagPatternName::try_new(name).unwrap()),
            level: BumpLevel::Slot(TagSlot::RIGHTMOST),
            message: "release".into(),
            push: true,
        }
    }

    fn ready(result: DryRunTagBumpOk) -> super::TagBumpPreview {
        match result {
            DryRunTagBumpOk::Ready(preview) => Some(preview),
            DryRunTagBumpOk::Rejected { .. } => None,
        }
        .unwrap()
    }

    #[test]
    fn pushed_preview_uses_local_tags_without_querying_origin() {
        let git = scripted_repo("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\tv1.2.3\t\t100");

        let preview = ready(
            dry_run_tag_bump::execute(query(None), &git, &FixedUserSettingsStore::default())
                .unwrap(),
        );

        assert_eq!(preview.pattern.as_ref(), "semver");
        assert_eq!(preview.next_tag.as_ref(), "v1.2.4");
    }

    #[test]
    fn the_project_default_pattern_applies_when_none_is_requested() {
        let git = scripted_repo(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\t0.19.1\t\t100\naaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\trelease-0.2.0\t\t100",
        );

        let preview = ready(dry_run_tag_bump::execute(query(None), &git, &sample_project_settings()).unwrap());

        assert_eq!(preview.pattern.as_ref(), "dev");
        assert_eq!(preview.base_tag.as_ref().unwrap().as_ref(), "0.19.1");
        assert_eq!(preview.next_tag.as_ref(), "0.19.2");
    }

    #[test]
    fn a_requested_pattern_uses_its_own_lineage() {
        let git = scripted_repo("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t\t0.19.1\t\t100");

        let preview = ready(
            dry_run_tag_bump::execute(query(Some("release")), &git, &sample_project_settings()).unwrap(),
        );

        assert_eq!(preview.base_tag, None);
        assert_eq!(preview.next_tag.as_ref(), "release-0.0.1");
    }

    #[test]
    fn an_unknown_pattern_is_rejected_before_git_runs() {
        let git = ScriptedGitClient::new(vec![ScriptedGitClient::applied("/repo/sample_project")]);

        let result =
            dry_run_tag_bump::execute(query(Some("nightly")), &git, &sample_project_settings()).unwrap();

        assert_eq!(
            result,
            DryRunTagBumpOk::Rejected {
                detail:
                    "tag pattern `nightly` is not configured; configured patterns: dev, release"
                        .into()
            }
        );
    }
}
