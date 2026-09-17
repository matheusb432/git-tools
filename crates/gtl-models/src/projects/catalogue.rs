use std::{collections::BTreeSet, path::Path};

use nutype::nutype;

use crate::{git::RemoteUrl, paths::RepositoryRoot};

pub const PROJECTS_MAX: u16 = 4096;
pub const PROJECT_MUTATIONS_MAX: usize = 256;

#[nutype(
    validate(predicate = |value| (2..=4).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_uppercase())),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Display, TryFrom, Serialize, Deserialize)
)]
pub struct ProjectId(String);

#[nutype(
    validate(predicate = |value| !value.is_empty() && value.trim() == value),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, TryFrom)
)]
pub struct ProjectTitle(String);

#[nutype(
    validate(predicate = |value| value.strip_prefix("~/").is_some_and(|path| !path.contains(['~', '\\', ':', '\0']) && path.split('/').all(|segment| !matches!(segment, "" | "." | "..")))),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, TryFrom)
)]
pub struct ProjectDirectorySource(String);

impl ProjectDirectorySource {
    pub fn resolve(
        &self,
        home: &Path,
    ) -> Result<RepositoryRoot, crate::paths::RepositoryRootError> {
        RepositoryRoot::try_new(home.join(&self.as_ref()[2..]))
    }
}

#[nutype(
    validate(predicate = |value| (1..=64).contains(&value.chars().count())),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, TryFrom)
)]
pub struct ProjectMuxSessionName(String);

#[nutype(
    validate(predicate = |value| value.len() == 7 && value.starts_with('#') && value.as_bytes()[1..].iter().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))),
    derive(Debug, Clone, PartialEq, Eq, AsRef, Display, TryFrom)
)]
pub struct ProjectColor(String);

#[nutype(
    validate(predicate = |value| !value.is_empty() && value.trim() == value),
    derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, AsRef, Display, TryFrom)
)]
pub struct ProjectGroupName(String);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectGroups(Vec<ProjectGroupName>);

impl ProjectGroups {
    pub fn try_new(groups: Vec<ProjectGroupName>) -> Result<Self, ProjectCollectionError> {
        if groups.len() > 64 {
            return Err(ProjectCollectionError::TooMany);
        }
        let count = groups.len();
        let unique: BTreeSet<_> = groups.into_iter().collect();
        if unique.len() != count {
            return Err(ProjectCollectionError::Duplicate);
        }
        Ok(Self(unique.into_iter().collect()))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ProjectGroupName] {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct ProjectIds(Vec<ProjectId>);

impl ProjectIds {
    pub fn try_new(ids: Vec<ProjectId>) -> Result<Self, ProjectCollectionError> {
        if ids.is_empty() {
            return Err(ProjectCollectionError::Empty);
        }
        if ids.len() > PROJECT_MUTATIONS_MAX {
            return Err(ProjectCollectionError::TooMany);
        }
        if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
            return Err(ProjectCollectionError::Duplicate);
        }
        Ok(Self(ids))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ProjectId] {
        &self.0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectCollectionError {
    #[error("project collection must not be empty")]
    Empty,
    #[error("project collection contains duplicates")]
    Duplicate,
    #[error("project collection exceeds its limit")]
    TooMany,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectAffiliation {
    Personal,
    Work,
}

impl ProjectAffiliation {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Work => "work",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectStatus {
    Active,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMetadata {
    pub title: ProjectTitle,
    pub source: ProjectDirectorySource,
    pub git_remote: Option<RemoteUrl>,
    pub mux_session_name: ProjectMuxSessionName,
    pub affiliation: ProjectAffiliation,
    pub color: Option<ProjectColor>,
    pub groups: ProjectGroups,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub id: ProjectId,
    pub metadata: ProjectMetadata,
    pub status: ProjectStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectOperationMode {
    Preview,
    Apply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectMutationOutcome {
    Changed,
    Unchanged,
}

#[derive(Clone, Debug)]
pub struct ProjectMutation {
    pub id: ProjectId,
    pub outcome: ProjectMutationOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_paths_cannot_escape_home_on_supported_platforms() {
        for path in [
            "/absolute",
            "~",
            "~/",
            "~/../outside",
            "~/a/./b",
            "~/a//b",
            "~/C:/outside",
            "~/a\\..\\outside",
        ] {
            assert!(ProjectDirectorySource::try_new(path).is_err(), "{path}");
        }
        let source = ProjectDirectorySource::try_new("~/tools/git tools").unwrap();
        assert_eq!(
            source.resolve(Path::new("/home/u")).unwrap().as_ref(),
            Path::new("/home/u/tools/git tools")
        );
    }

    #[test]
    fn mutation_ids_and_groups_reject_duplicates_and_enforce_bounds() {
        let id = ProjectId::try_new("GTL").unwrap();
        assert!(ProjectIds::try_new(Vec::new()).is_err());
        assert!(ProjectIds::try_new(vec![id.clone(), id.clone()]).is_err());
        assert!(ProjectIds::try_new(vec![id; PROJECT_MUTATIONS_MAX + 1]).is_err());
        let group = ProjectGroupName::try_new("tools").unwrap();
        assert!(ProjectGroups::try_new(vec![group.clone(), group]).is_err());
        assert!(
            ProjectGroups::try_new(
                (0..65)
                    .map(|index| ProjectGroupName::try_new(index.to_string()).unwrap())
                    .collect()
            )
            .is_err()
        );
    }
}
