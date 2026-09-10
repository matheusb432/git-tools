//! Selects the base and next tag for one template and bump level.

use gtl_models::{
    git::TagName,
    tags::{SemverComponent, TagSlot, TagTemplate, TagVersionBumpError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BumpLevel {
    Slot(TagSlot),
    Component(SemverComponent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagVersionDecision {
    pub(super) slot: TagSlot,
    pub(super) base_tag: Option<TagName>,
    pub(super) next_tag: TagName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TagVersionRejection {
    ComponentAbsent {
        component: SemverComponent,
        template: TagTemplate,
    },
    Bump(TagVersionBumpError),
}

impl std::fmt::Display for TagVersionRejection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ComponentAbsent {
                component,
                template,
            } => write!(
                formatter,
                "cannot bump {component}: template `{template}` has no `{{{component}}}` slot"
            ),
            Self::Bump(error) => write!(formatter, "cannot bump tags: {error}"),
        }
    }
}

pub(super) fn decide_tag_version<'tag>(
    tag_names: impl IntoIterator<Item = &'tag str>,
    template: &TagTemplate,
    level: BumpLevel,
) -> Result<TagVersionDecision, TagVersionRejection> {
    let slot = match level {
        BumpLevel::Slot(slot) => slot,
        BumpLevel::Component(component) => {
            template
                .slot_of(component)
                .ok_or_else(|| TagVersionRejection::ComponentAbsent {
                    component,
                    template: template.clone(),
                })?
        }
    };
    let base = tag_names
        .into_iter()
        .filter_map(|name| template.parse_version(name))
        .max();
    let next = base
        .as_ref()
        .map_or_else(|| template.zero_version(), Clone::clone)
        .bump(slot)
        .map_err(TagVersionRejection::Bump)?;
    Ok(TagVersionDecision {
        slot,
        base_tag: base.as_ref().map(|base| template.render(base)),
        next_tag: template.render(&next),
    })
}

#[cfg(test)]
mod tests {
    use gtl_models::{
        git::TagName,
        tags::{SemverComponent, TagSlot, TagTemplate, TagVersionBumpError},
    };

    use super::{BumpLevel, TagVersionDecision, TagVersionRejection, decide_tag_version};

    fn template(source: &str) -> TagTemplate {
        source.parse().unwrap()
    }

    fn slot(index: u32) -> TagSlot {
        TagSlot::try_new(index).unwrap()
    }

    fn tag(name: &str) -> TagName {
        TagName::try_new(name).unwrap()
    }

    #[test]
    fn bumps_named_components_and_resets_the_slots_to_their_right() {
        let semver = template("v{major}.{minor}.{patch}");
        let expected = [
            (SemverComponent::Patch, 0, "v0.30.1"),
            (SemverComponent::Minor, 1, "v0.31.0"),
            (SemverComponent::Major, 2, "v1.0.0"),
        ];

        for (component, index, next_tag) in expected {
            assert_eq!(
                decide_tag_version(["v0.30.0"], &semver, BumpLevel::Component(component)),
                Ok(TagVersionDecision {
                    slot: slot(index),
                    base_tag: Some(tag("v0.30.0")),
                    next_tag: tag(next_tag),
                })
            );
        }
    }

    #[test]
    fn only_tags_matching_the_template_compete_for_the_base() {
        let release = template("release-{major}.{minor}.{patch}");

        assert_eq!(
            decide_tag_version(
                [
                    "0.20.0",
                    "release-0.3.0",
                    "v9.9.9",
                    "release-0.10.0",
                    "handoff"
                ],
                &release,
                BumpLevel::Slot(TagSlot::RIGHTMOST),
            ),
            Ok(TagVersionDecision {
                slot: TagSlot::RIGHTMOST,
                base_tag: Some(tag("release-0.10.0")),
                next_tag: tag("release-0.10.1"),
            })
        );
    }

    #[test]
    fn an_empty_lineage_seeds_from_zero() {
        assert_eq!(
            decide_tag_version(
                ["0.19.1"],
                &template("release-{major}.{minor}.{patch}"),
                BumpLevel::Component(SemverComponent::Minor),
            ),
            Ok(TagVersionDecision {
                slot: slot(1),
                base_tag: None,
                next_tag: tag("release-0.1.0"),
            })
        );
    }

    #[test]
    fn rejects_a_component_the_template_lacks_and_slots_out_of_range() {
        let build = template("build-{n}");

        assert_eq!(
            decide_tag_version([], &build, BumpLevel::Component(SemverComponent::Patch)),
            Err(TagVersionRejection::ComponentAbsent {
                component: SemverComponent::Patch,
                template: build.clone(),
            })
        );
        assert_eq!(
            decide_tag_version([], &build, BumpLevel::Slot(slot(1))),
            Err(TagVersionRejection::Bump(
                TagVersionBumpError::SlotOutOfRange {
                    slot: slot(1),
                    slot_count: 1,
                }
            ))
        );
    }

    #[test]
    fn rejects_component_overflow() {
        assert_eq!(
            decide_tag_version(
                ["v1.2.18446744073709551615"],
                &template("v{major}.{minor}.{patch}"),
                BumpLevel::Slot(TagSlot::RIGHTMOST),
            ),
            Err(TagVersionRejection::Bump(TagVersionBumpError::Overflow {
                slot: TagSlot::RIGHTMOST
            }))
        );
    }
}
