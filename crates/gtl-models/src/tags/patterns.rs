//! Named tag templates configured per user and per project.

use std::collections::BTreeMap;

use nutype::nutype;

use super::template::TagTemplate;
use crate::{git::known_valid, paths::ProjectName};

#[nutype(
    validate(with = validate_pattern_name, error = TagPatternNameError),
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        AsRef,
        Deref,
        Display,
        TryFrom,
        FromStr
    )
)]
pub struct TagPatternName(String);

fn validate_pattern_name(value: &str) -> Result<(), TagPatternNameError> {
    if value.is_empty() {
        Err(TagPatternNameError::Empty)
    } else if value.chars().any(char::is_whitespace) {
        Err(TagPatternNameError::Whitespace)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TagPatternNameError {
    #[error("tag pattern name must not be empty")]
    Empty,
    #[error("tag pattern name must not contain whitespace")]
    Whitespace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPatternSet {
    patterns: BTreeMap<TagPatternName, TagTemplate>,
    default: Option<TagPatternName>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TagPatternSetError {
    #[error("at least one tag pattern is required")]
    Empty,
    #[error("default tag pattern `{name}` is not one of the configured patterns")]
    UnknownDefault { name: TagPatternName },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TagPatternSelectionError {
    #[error("tag pattern `{requested}` is not configured; configured patterns: {available}")]
    Unknown {
        requested: TagPatternName,
        available: String,
    },
    #[error("no default tag pattern is configured; select one of: {available}")]
    NoDefault { available: String },
}

impl TagPatternSet {
    pub fn try_new(
        patterns: impl IntoIterator<Item = (TagPatternName, TagTemplate)>,
        default: Option<TagPatternName>,
    ) -> Result<Self, TagPatternSetError> {
        let patterns: BTreeMap<_, _> = patterns.into_iter().collect();
        if patterns.is_empty() {
            return Err(TagPatternSetError::Empty);
        }
        if let Some(name) = &default
            && !patterns.contains_key(name)
        {
            return Err(TagPatternSetError::UnknownDefault { name: name.clone() });
        }
        Ok(Self { patterns, default })
    }

    #[must_use]
    pub fn semver_with_v_prefix() -> Self {
        Self {
            patterns: BTreeMap::from([(
                known_valid(TagPatternName::try_new("semver")),
                TagTemplate::semver_with_v_prefix(),
            )]),
            default: None,
        }
    }

    pub fn select(
        &self,
        requested: Option<&TagPatternName>,
    ) -> Result<(&TagPatternName, &TagTemplate), TagPatternSelectionError> {
        let name = match requested {
            Some(requested) => self
                .patterns
                .get_key_value(requested)
                .map(|(name, _)| name)
                .ok_or_else(|| TagPatternSelectionError::Unknown {
                    requested: requested.clone(),
                    available: self.available(),
                })?,
            None => self
                .default_name()
                .ok_or_else(|| TagPatternSelectionError::NoDefault {
                    available: self.available(),
                })?,
        };
        self.patterns
            .get_key_value(name)
            .ok_or_else(|| TagPatternSelectionError::NoDefault {
                available: self.available(),
            })
    }

    #[must_use]
    pub fn default_name(&self) -> Option<&TagPatternName> {
        self.default.as_ref().or_else(|| {
            let mut names = self.patterns.keys();
            let single = names.next();
            names.next().map_or(single, |_| None)
        })
    }

    pub fn patterns(&self) -> impl ExactSizeIterator<Item = (&TagPatternName, &TagTemplate)> {
        self.patterns.iter()
    }

    fn available(&self) -> String {
        self.patterns
            .keys()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPatternSettings {
    default: TagPatternSet,
    projects: BTreeMap<ProjectName, TagPatternSet>,
}

impl Default for TagPatternSettings {
    fn default() -> Self {
        Self {
            default: TagPatternSet::semver_with_v_prefix(),
            projects: BTreeMap::new(),
        }
    }
}

impl TagPatternSettings {
    #[must_use]
    pub fn new(
        default: Option<TagPatternSet>,
        projects: impl IntoIterator<Item = (ProjectName, TagPatternSet)>,
    ) -> Self {
        Self {
            default: default.unwrap_or_else(TagPatternSet::semver_with_v_prefix),
            projects: projects.into_iter().collect(),
        }
    }

    #[must_use]
    pub fn for_project_or_default(&self, project: &ProjectName) -> &TagPatternSet {
        self.projects.get(project).unwrap_or(&self.default)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TagPatternName, TagPatternSelectionError, TagPatternSet, TagPatternSetError,
        TagPatternSettings,
    };
    use crate::{paths::ProjectName, tags::TagTemplate};

    fn name(value: &str) -> TagPatternName {
        TagPatternName::try_new(value).unwrap()
    }

    fn template(source: &str) -> TagTemplate {
        source.parse().unwrap()
    }

    fn project_tag_patterns(default: Option<&str>) -> TagPatternSet {
        TagPatternSet::try_new(
            [
                (name("dev"), template("{major}.{minor}.{patch}")),
                (name("release"), template("release-{major}.{minor}.{patch}")),
            ],
            default.map(name),
        )
        .unwrap()
    }

    #[test]
    fn a_single_pattern_is_its_own_default() {
        let set = TagPatternSet::semver_with_v_prefix();

        let (selected, template) = set.select(None).unwrap();
        assert_eq!(selected.as_ref(), "semver");
        assert_eq!(template.to_string(), "v{major}.{minor}.{patch}");
    }

    #[test]
    fn several_patterns_without_a_default_require_a_request() {
        let set = project_tag_patterns(None);

        assert_eq!(
            set.select(None),
            Err(TagPatternSelectionError::NoDefault {
                available: "dev, release".into()
            })
        );
        assert_eq!(
            set.select(Some(&name("release"))).unwrap().0.as_ref(),
            "release"
        );
    }

    #[test]
    fn the_configured_default_and_unknown_requests_are_reported() {
        let set = project_tag_patterns(Some("dev"));

        assert_eq!(set.select(None).unwrap().0.as_ref(), "dev");
        assert_eq!(
            set.select(Some(&name("nightly"))),
            Err(TagPatternSelectionError::Unknown {
                requested: name("nightly"),
                available: "dev, release".into()
            })
        );
    }

    #[test]
    fn construction_rejects_empty_sets_and_unknown_defaults() {
        assert_eq!(
            TagPatternSet::try_new([], None),
            Err(TagPatternSetError::Empty)
        );
        assert_eq!(
            TagPatternSet::try_new([(name("dev"), template("{n}"))], Some(name("release"))),
            Err(TagPatternSetError::UnknownDefault {
                name: name("release")
            })
        );
    }

    #[test]
    fn project_sets_replace_the_user_default_entirely() {
        let project = ProjectName::try_new("example-project".to_owned()).unwrap();
        let settings =
            TagPatternSettings::new(None, [(project.clone(), project_tag_patterns(Some("dev")))]);

        assert_eq!(
            settings.for_project_or_default(&project),
            &project_tag_patterns(Some("dev"))
        );
        assert_eq!(
            settings.for_project_or_default(&ProjectName::try_new("other".to_owned()).unwrap()),
            &TagPatternSet::semver_with_v_prefix()
        );
    }
}
