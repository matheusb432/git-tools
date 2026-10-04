//! Diffs between two revisions of a repository hosted on github.com.

use std::fmt;

use gtl_models::{diffs::DiffTextId, failure::RemoteDiffFailure, paths::ProjectName};

const GITHUB_HOSTS: [&str; 2] = ["github.com", "www.github.com"];
const COMPARE_DIFF_MEDIA_TYPE: &str = "application/vnd.github.diff";
const GITHUB_OWNER_CHARACTERS_MAX: usize = 39;
const GITHUB_NAME_CHARACTERS_MAX: usize = 100;
const GITHUB_REVISION_CHARACTERS_MAX: usize = 255;
const OBJECT_ID_CHARACTER_COUNT: usize = 40;
const OBJECT_ID_LABEL_CHARACTERS: usize = 12;

/// A github.com repository; GitHub matches owners and names case-insensitively, so both are
/// lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitHubRepository {
    owner: String,
    name: String,
}

impl GitHubRepository {
    /// Reads `https://github.com/OWNER/NAME`, `ssh://git@github.com/OWNER/NAME`, or
    /// `git@github.com:OWNER/NAME`, each with an optional `.git` suffix.
    pub fn parse_origin(origin: &str) -> Result<Self, RemoteDiffFailure> {
        let origin = origin.trim();
        let (authority, path) = match origin.split_once("://") {
            Some((scheme, rest))
                if matches!(scheme.to_ascii_lowercase().as_str(), "https" | "ssh") =>
            {
                rest.split_once('/')
            }
            Some(_) => None,
            None => origin.split_once(':'),
        }
        .ok_or(RemoteDiffFailure::UnsupportedOrigin)?;
        let host = authority
            .rsplit_once('@')
            .map_or(authority, |(_, host)| host);
        let host = host.split_once(':').map_or(host, |(host, _)| host);
        if !GITHUB_HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
            return Err(RemoteDiffFailure::UnsupportedOrigin);
        }
        let path = path.strip_suffix('/').unwrap_or(path);
        let (owner, name) = path
            .split_once('/')
            .ok_or(RemoteDiffFailure::UnsupportedOrigin)?;
        let name = name.strip_suffix(".git").unwrap_or(name);
        if !is_github_owner(owner) || !is_github_name(name) {
            return Err(RemoteDiffFailure::UnsupportedOrigin);
        }
        Ok(Self {
            owner: owner.to_ascii_lowercase(),
            name: name.to_ascii_lowercase(),
        })
    }
}

impl fmt::Display for GitHubRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.owner, self.name)
    }
}

fn is_github_owner(owner: &str) -> bool {
    (1..=GITHUB_OWNER_CHARACTERS_MAX).contains(&owner.len())
        && !owner.starts_with('-')
        && !owner.ends_with('-')
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn is_github_name(name: &str) -> bool {
    (1..=GITHUB_NAME_CHARACTERS_MAX).contains(&name.len())
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// A branch, tag, or commit name that GitHub accepts in a comparison path without escaping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitHubRevision(String);

impl GitHubRevision {
    fn parse(raw: &str) -> Result<Self, RemoteDiffFailure> {
        let valid = (1..=GITHUB_REVISION_CHARACTERS_MAX).contains(&raw.len())
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._/+-".contains(&byte))
            && !raw.starts_with(['-', '/', '.'])
            && !raw.ends_with(['/', '.'])
            && !raw.contains("..")
            && !raw.contains("//");
        if valid {
            Ok(Self(raw.to_owned()))
        } else {
            Err(RemoteDiffFailure::InvalidRange)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn label(&self) -> &str {
        if self.0.len() == OBJECT_ID_CHARACTER_COUNT
            && self.0.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            &self.0[..OBJECT_ID_LABEL_CHARACTERS]
        } else {
            &self.0
        }
    }
}

/// GitHub's three-dot comparison: the changes on `head` since its merge base with `base`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GitHubCompareRange {
    pub base: GitHubRevision,
    pub head: GitHubRevision,
}

impl GitHubCompareRange {
    /// Reads `BASE...HEAD`, rejecting two-dot ranges.
    pub fn parse(range: &str) -> Result<Self, RemoteDiffFailure> {
        let (base, head) = range
            .trim()
            .split_once("...")
            .ok_or(RemoteDiffFailure::InvalidRange)?;
        Ok(Self {
            base: GitHubRevision::parse(base)?,
            head: GitHubRevision::parse(head)?,
        })
    }
}

impl fmt::Display for GitHubCompareRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}...{}", self.base.0, self.head.0)
    }
}

/// Identifies one cached remote diff.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RemoteDiffKey {
    pub repository: GitHubRepository,
    pub range: GitHubCompareRange,
}

impl RemoteDiffKey {
    pub fn parse(origin: &str, range: &str) -> Result<Self, RemoteDiffFailure> {
        Ok(Self {
            repository: GitHubRepository::parse_origin(origin)?,
            range: GitHubCompareRange::parse(range)?,
        })
    }

    /// The one request that returns this comparison as diff text.
    #[must_use]
    pub fn compare_request(&self) -> GitHubApiRequest {
        GitHubApiRequest {
            path: format!(
                "repos/{}/{}/compare/{}",
                self.repository.owner, self.repository.name, self.range
            ),
            accept: COMPARE_DIFF_MEDIA_TYPE,
        }
    }

    /// Names the diff where a repository name would appear, abbreviating full commit IDs.
    #[must_use]
    pub fn label(&self) -> ProjectName {
        let label = format!(
            "{} {}...{}",
            self.repository,
            self.range.base.label(),
            self.range.head.label()
        );
        ProjectName::try_new(label).unwrap_or_default()
    }
}

/// One GET request to the github.com REST API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubApiRequest {
    /// The path below the API root, such as `repos/OWNER/NAME/compare/BASE...HEAD`.
    pub path: String,
    /// The media type GitHub encodes the response body in.
    pub accept: &'static str,
}

/// GitHub's answer to a [`GitHubApiRequest`], including refusals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubApiResponse {
    pub status: u16,
    pub rate_limit_remaining: Option<u64>,
    pub body: Vec<u8>,
}

/// A remote diff stored as diff text, with the label its views show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRemoteDiff {
    pub id: DiffTextId,
    pub label: ProjectName,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(origin: &str) -> Result<String, RemoteDiffFailure> {
        GitHubRepository::parse_origin(origin).map(|repository| repository.to_string())
    }

    #[test]
    fn https_ssh_and_scp_origins_name_the_same_repository() {
        for origin in [
            "https://github.com/Example-Org/Widget.rs",
            "https://github.com/example-org/widget.rs.git",
            "https://github.com/example-org/widget.rs/",
            "https://token@www.github.com/example-org/widget.rs",
            "ssh://git@github.com/example-org/widget.rs.git",
            "ssh://git@github.com:22/example-org/widget.rs",
            "git@github.com:example-org/widget.rs.git",
            "  git@GitHub.com:example-org/widget.rs  ",
            "github.com:example-org/widget.rs",
        ] {
            assert_eq!(
                repository(origin).as_deref(),
                Ok("example-org/widget.rs"),
                "{origin}"
            );
        }
    }

    #[test]
    fn origins_outside_github_or_beyond_a_repository_are_unsupported() {
        for origin in [
            "",
            "example-org/widget",
            "http://github.com/example-org/widget",
            "git://github.com/example-org/widget",
            "https://gitlab.com/example-org/widget",
            "https://github.com.example.invalid/example-org/widget",
            "https://github.com/example-org",
            "https://github.com/example-org/widget/tree/main",
            "https://github.com/-org/widget",
            "https://github.com/example-org/..",
            "https://github.com/example-org/wid get",
            "C:/example-org/widget",
        ] {
            assert_eq!(
                repository(origin),
                Err(RemoteDiffFailure::UnsupportedOrigin),
                "{origin}"
            );
        }
    }

    #[test]
    fn ranges_must_use_three_dots_between_two_plain_revisions() {
        let range = GitHubCompareRange::parse("v1.2.0...feature/login+fix").unwrap();
        assert_eq!(range.base.as_str(), "v1.2.0");
        assert_eq!(range.head.as_str(), "feature/login+fix");

        for raw in [
            "v1..v2",
            "v1",
            "...v2",
            "v1...",
            "v1....v2",
            "v1...v2...v3",
            "HEAD~1...HEAD",
            "v1...-v2",
            "v1...a//b",
            "v1...v2.",
            "v1...a b",
            "v1...a?b",
        ] {
            assert_eq!(
                GitHubCompareRange::parse(raw),
                Err(RemoteDiffFailure::InvalidRange),
                "{raw}"
            );
        }
    }

    #[test]
    fn key_builds_the_compare_request_and_abbreviates_commit_ids_in_its_label() {
        let base = "0123456789abcdef0123456789abcdef01234567";
        let key = RemoteDiffKey::parse(
            "git@github.com:Example-Org/Widget.git",
            &format!("{base}...v2.0.0"),
        )
        .unwrap();

        assert_eq!(
            key.compare_request(),
            GitHubApiRequest {
                path: format!("repos/example-org/widget/compare/{base}...v2.0.0"),
                accept: "application/vnd.github.diff",
            }
        );
        assert_eq!(
            key.label().as_ref(),
            "example-org/widget 0123456789ab...v2.0.0"
        );
    }
}
