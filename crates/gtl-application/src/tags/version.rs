//! Semantic-version selection shared by tag-bump operations.

use gtl_models::git::TagName;
use semver::Version;

/// The `SemVer` component advanced by a tag bump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpLevel {
    Major,
    Minor,
    Patch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagVersionDecision {
    pub(super) base_tag: TagName,
    pub(super) next_tag: TagName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TagVersionRejection {
    NoCanonicalTag { noncanonical_tag: Option<String> },
    NewerNoncanonicalTag { base: Version, tag: String },
    ComponentOverflow { component: &'static str },
}

impl std::fmt::Display for TagVersionRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCanonicalTag {
                noncanonical_tag: None,
            } => formatter.write_str("cannot bump tags: no canonical vN.N.N tag exists"),
            Self::NoCanonicalTag {
                noncanonical_tag: Some(tag),
            } => write!(
                formatter,
                "cannot bump tags: no canonical vN.N.N tag exists; found noncanonical tag {tag}"
            ),
            Self::NewerNoncanonicalTag { base, tag } => write!(
                formatter,
                "cannot bump from v{base}: newer noncanonical version tag {tag} is ambiguous"
            ),
            Self::ComponentOverflow { component } => {
                write!(
                    formatter,
                    "cannot bump tags: {component} version component overflowed"
                )
            }
        }
    }
}

pub(super) fn decide_tag_version<'tag>(
    tag_names: impl IntoIterator<Item = &'tag str>,
    level: BumpLevel,
) -> Result<TagVersionDecision, TagVersionRejection> {
    let mut canonical = None::<Version>;
    let mut noncanonical = None::<(Version, String)>;

    for name in tag_names {
        if let Some(version) = canonical_version(name) {
            canonical = canonical.max(Some(version));
        } else if let Some(version) = version_like(name) {
            noncanonical = noncanonical.max(Some((version, name.to_string())));
        }
    }

    let Some(base) = canonical else {
        return Err(TagVersionRejection::NoCanonicalTag {
            noncanonical_tag: noncanonical.map(|(_, name)| name),
        });
    };
    if let Some((version, tag)) = noncanonical
        && version > base
    {
        return Err(TagVersionRejection::NewerNoncanonicalTag { base, tag });
    }

    let next = bump_version(base.clone(), level)?;
    Ok(TagVersionDecision {
        base_tag: TagName::semantic_version(base.major, base.minor, base.patch),
        next_tag: TagName::semantic_version(next.major, next.minor, next.patch),
    })
}

fn canonical_version(name: &str) -> Option<Version> {
    let raw = name.strip_prefix('v')?;
    let version = stable_version(raw)?;
    (format!("v{version}") == name).then_some(version)
}

fn version_like(name: &str) -> Option<Version> {
    stable_version(name.strip_prefix('v').unwrap_or(name))
}

fn stable_version(raw: &str) -> Option<Version> {
    let version = Version::parse(raw).ok()?;
    (version.pre.is_empty() && version.build.is_empty()).then_some(version)
}

fn bump_version(mut version: Version, level: BumpLevel) -> Result<Version, TagVersionRejection> {
    match level {
        BumpLevel::Major => {
            version.major = checked_increment(version.major, "major")?;
            version.minor = 0;
            version.patch = 0;
        }
        BumpLevel::Minor => {
            version.minor = checked_increment(version.minor, "minor")?;
            version.patch = 0;
        }
        BumpLevel::Patch => {
            version.patch = checked_increment(version.patch, "patch")?;
        }
    }
    Ok(version)
}

fn checked_increment(
    component: u64,
    component_name: &'static str,
) -> Result<u64, TagVersionRejection> {
    component
        .checked_add(1)
        .ok_or(TagVersionRejection::ComponentOverflow {
            component: component_name,
        })
}

#[cfg(test)]
mod tests {
    use gtl_models::git::TagName;

    use super::{BumpLevel, TagVersionDecision, TagVersionRejection, decide_tag_version};

    #[test]
    fn bumps_each_semver_component_and_resets_lower_components() {
        let expected = [
            (BumpLevel::Patch, "v0.30.1"),
            (BumpLevel::Minor, "v0.31.0"),
            (BumpLevel::Major, "v1.0.0"),
        ];

        for (level, next_tag) in expected {
            assert_eq!(
                decide_tag_version(["v0.30.0"], level),
                Ok(TagVersionDecision {
                    base_tag: TagName::try_new("v0.30.0").unwrap(),
                    next_tag: TagName::try_new(next_tag).unwrap(),
                })
            );
        }
    }

    #[test]
    fn selects_the_semantic_maximum_and_ignores_an_older_missing_v_typo() {
        assert_eq!(
            decide_tag_version(["v0.9.0", "0.29.1", "v0.30.0", "v0.10.0"], BumpLevel::Patch,),
            Ok(TagVersionDecision {
                base_tag: TagName::try_new("v0.30.0").unwrap(),
                next_tag: TagName::try_new("v0.30.1").unwrap(),
            })
        );
    }

    #[test]
    fn rejects_a_newer_noncanonical_version_as_ambiguous() {
        assert_eq!(
            decide_tag_version(["v0.30.0", "0.31.0"], BumpLevel::Patch),
            Err(TagVersionRejection::NewerNoncanonicalTag {
                base: semver::Version::new(0, 30, 0),
                tag: "0.31.0".into(),
            })
        );
    }

    #[test]
    fn requires_a_canonical_stable_version_tag() {
        assert_eq!(
            decide_tag_version(["0.29.1"], BumpLevel::Patch),
            Err(TagVersionRejection::NoCanonicalTag {
                noncanonical_tag: Some("0.29.1".into()),
            })
        );
        assert_eq!(
            decide_tag_version(["v1.2.3-rc.1"], BumpLevel::Patch),
            Err(TagVersionRejection::NoCanonicalTag {
                noncanonical_tag: None,
            })
        );
    }

    #[test]
    fn rejects_component_overflow() {
        assert_eq!(
            decide_tag_version(["v1.2.18446744073709551615"], BumpLevel::Patch),
            Err(TagVersionRejection::ComponentOverflow { component: "patch" })
        );
    }
}
