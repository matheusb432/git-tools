//! Tag name templates whose numeric slots are parsed from and rendered into tag names.

use std::{fmt, str::FromStr};

use crate::git::{TagName, known_valid};

pub const TAG_TEMPLATE_SLOT_COUNT_MAX: usize = 16;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "lowercase")]
pub enum SemverComponent {
    Major,
    Minor,
    Patch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TagSlot(u8);

impl TagSlot {
    pub const RIGHTMOST: Self = Self(0);

    pub fn try_new(index_from_right: u32) -> Result<Self, TagSlotError> {
        u8::try_from(index_from_right)
            .ok()
            .filter(|index| usize::from(*index) < TAG_TEMPLATE_SLOT_COUNT_MAX)
            .map(Self)
            .ok_or(TagSlotError { index_from_right })
    }

    #[must_use]
    pub const fn index_from_right(self) -> u32 {
        self.0 as u32
    }
}

impl fmt::Display for TagSlot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "slot {index_from_right} is out of range; templates hold at most {TAG_TEMPLATE_SLOT_COUNT_MAX} slots"
)]
pub struct TagSlotError {
    index_from_right: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TemplateSegment {
    Literal(String),
    Slot(Option<SemverComponent>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagTemplate {
    source: String,
    segments: Vec<TemplateSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TagTemplateError {
    #[error("tag template must not be blank")]
    Blank,
    #[error("tag template must not contain whitespace")]
    Whitespace,
    #[error("tag template must contain at least one slot such as `{{patch}}` or `{{n}}`")]
    NoSlots,
    #[error("tag template has more than {TAG_TEMPLATE_SLOT_COUNT_MAX} slots")]
    TooManySlots,
    #[error(
        "tag template placeholder `{{{placeholder}}}` is not one of `{{major}}`, `{{minor}}`, `{{patch}}`, or `{{n}}`"
    )]
    UnknownPlaceholder { placeholder: String },
    #[error("tag template repeats `{{{component}}}`")]
    DuplicateComponent { component: SemverComponent },
    #[error("tag template places two slots without a literal between them")]
    AdjacentSlots,
    #[error("tag template literal after a slot must not start with a digit")]
    DigitAfterSlot,
    #[error("tag template has an unclosed `{{`")]
    UnclosedPlaceholder,
    #[error("tag template has a `}}` without a matching `{{`")]
    UnmatchedClosingBrace,
}

impl TagTemplate {
    #[must_use]
    pub fn semver_with_v_prefix() -> Self {
        Self {
            source: "v{major}.{minor}.{patch}".into(),
            segments: vec![
                TemplateSegment::Literal("v".into()),
                TemplateSegment::Slot(Some(SemverComponent::Major)),
                TemplateSegment::Literal(".".into()),
                TemplateSegment::Slot(Some(SemverComponent::Minor)),
                TemplateSegment::Literal(".".into()),
                TemplateSegment::Slot(Some(SemverComponent::Patch)),
            ],
        }
    }

    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|segment| matches!(segment, TemplateSegment::Slot(_)))
            .count()
    }

    #[must_use]
    pub fn slot_of(&self, component: SemverComponent) -> Option<TagSlot> {
        self.slots_from_right()
            .find(|(_, named)| *named == Some(component))
            .map(|(slot, _)| slot)
    }

    #[must_use]
    pub fn component_at(&self, slot: TagSlot) -> Option<SemverComponent> {
        self.slots_from_right()
            .find(|(candidate, _)| *candidate == slot)
            .and_then(|(_, named)| named)
    }

    fn slots_from_right(&self) -> impl Iterator<Item = (TagSlot, Option<SemverComponent>)> {
        self.segments
            .iter()
            .rev()
            .filter_map(|segment| match segment {
                TemplateSegment::Slot(named) => Some(*named),
                TemplateSegment::Literal(_) => None,
            })
            .enumerate()
            .filter_map(|(index, named)| {
                u8::try_from(index)
                    .ok()
                    .map(|index| (TagSlot(index), named))
            })
    }

    #[must_use]
    pub fn zero_version(&self) -> TagVersion {
        TagVersion(vec![0; self.slot_count()])
    }

    #[must_use]
    pub fn parse_version(&self, tag_name: &str) -> Option<TagVersion> {
        let mut rest = tag_name;
        let mut components = Vec::with_capacity(self.slot_count());
        for segment in &self.segments {
            match segment {
                TemplateSegment::Literal(literal) => {
                    rest = rest.strip_prefix(literal.as_str())?;
                }
                TemplateSegment::Slot(_) => {
                    let (component, remaining) = take_component(rest)?;
                    components.push(component);
                    rest = remaining;
                }
            }
        }
        rest.is_empty().then_some(TagVersion(components))
    }

    #[must_use]
    pub fn render(&self, version: &TagVersion) -> TagName {
        let mut components = version.0.iter();
        let mut rendered = String::with_capacity(self.source.len());
        for segment in &self.segments {
            match segment {
                TemplateSegment::Literal(literal) => rendered.push_str(literal),
                TemplateSegment::Slot(_) => {
                    let component = components.next().copied().unwrap_or(0);
                    rendered.push_str(&component.to_string());
                }
            }
        }
        known_valid(TagName::try_new(rendered))
    }
}

fn take_component(rest: &str) -> Option<(u64, &str)> {
    let digits_len = rest.bytes().take_while(u8::is_ascii_digit).count();
    let (digits, remaining) = rest.split_at(digits_len);
    let canonical = digits == "0" || (!digits.is_empty() && !digits.starts_with('0'));
    canonical
        .then(|| digits.parse::<u64>().ok())
        .flatten()
        .map(|component| (component, remaining))
}

impl FromStr for TagTemplate {
    type Err = TagTemplateError;

    fn from_str(source: &str) -> Result<Self, Self::Err> {
        if source.trim().is_empty() {
            return Err(TagTemplateError::Blank);
        }
        if source.chars().any(char::is_whitespace) {
            return Err(TagTemplateError::Whitespace);
        }
        let segments = parse_segments(source)?;
        let template = Self {
            source: source.to_owned(),
            segments,
        };
        match template.slot_count() {
            0 => Err(TagTemplateError::NoSlots),
            count if count > TAG_TEMPLATE_SLOT_COUNT_MAX => Err(TagTemplateError::TooManySlots),
            _ => Ok(template),
        }
    }
}

fn parse_segments(source: &str) -> Result<Vec<TemplateSegment>, TagTemplateError> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut rest = source;
    while let Some(open) = rest.find(['{', '}']) {
        let (before, from_brace) = rest.split_at(open);
        literal.push_str(before);
        if from_brace.starts_with('}') {
            return Err(TagTemplateError::UnmatchedClosingBrace);
        }
        let close = from_brace
            .find('}')
            .ok_or(TagTemplateError::UnclosedPlaceholder)?;
        let placeholder = &from_brace[1..close];
        if placeholder.contains('{') {
            return Err(TagTemplateError::UnclosedPlaceholder);
        }
        push_literal(&mut segments, &mut literal)?;
        push_slot(&mut segments, placeholder)?;
        rest = &from_brace[close + 1..];
    }
    literal.push_str(rest);
    push_literal(&mut segments, &mut literal)?;
    Ok(segments)
}

fn push_literal(
    segments: &mut Vec<TemplateSegment>,
    literal: &mut String,
) -> Result<(), TagTemplateError> {
    if literal.is_empty() {
        return Ok(());
    }
    if matches!(segments.last(), Some(TemplateSegment::Slot(_)))
        && literal.starts_with(|character: char| character.is_ascii_digit())
    {
        return Err(TagTemplateError::DigitAfterSlot);
    }
    segments.push(TemplateSegment::Literal(std::mem::take(literal)));
    Ok(())
}

fn push_slot(
    segments: &mut Vec<TemplateSegment>,
    placeholder: &str,
) -> Result<(), TagTemplateError> {
    if matches!(segments.last(), Some(TemplateSegment::Slot(_))) {
        return Err(TagTemplateError::AdjacentSlots);
    }
    let named = match placeholder {
        "n" => None,
        component => Some(component.parse::<SemverComponent>().map_err(|_| {
            TagTemplateError::UnknownPlaceholder {
                placeholder: component.to_owned(),
            }
        })?),
    };
    if let Some(component) = named
        && segments.contains(&TemplateSegment::Slot(Some(component)))
    {
        return Err(TagTemplateError::DuplicateComponent { component });
    }
    segments.push(TemplateSegment::Slot(named));
    Ok(())
}

impl fmt::Display for TagTemplate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.source)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TagVersion(Vec<u64>);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TagVersionBumpError {
    #[error("slot {slot} does not exist; the template has {slot_count} slots")]
    SlotOutOfRange { slot: TagSlot, slot_count: usize },
    #[error("slot {slot} overflowed")]
    Overflow { slot: TagSlot },
}

impl TagVersion {
    pub fn bump(&self, slot: TagSlot) -> Result<Self, TagVersionBumpError> {
        let slot_count = self.0.len();
        let index = slot_count
            .checked_sub(1)
            .and_then(|last| last.checked_sub(usize::from(slot.0)))
            .ok_or(TagVersionBumpError::SlotOutOfRange { slot, slot_count })?;
        let mut components = self.0.clone();
        let bumped = components[index]
            .checked_add(1)
            .ok_or(TagVersionBumpError::Overflow { slot })?;
        components[index] = bumped;
        for component in &mut components[index + 1..] {
            *component = 0;
        }
        Ok(Self(components))
    }
}

#[cfg(test)]
mod tests {
    use super::{SemverComponent, TagSlot, TagTemplate, TagTemplateError, TagVersionBumpError};

    fn parse(source: &str) -> TagTemplate {
        source.parse().unwrap()
    }

    fn slot(index: u32) -> TagSlot {
        TagSlot::try_new(index).unwrap()
    }

    #[test]
    fn parses_and_renders_each_supported_shape() {
        let cases = [
            ("v{major}.{minor}.{patch}", "v1.2.3", "v1.2.4"),
            ("{major}.{minor}.{patch}", "0.19.1", "0.19.2"),
            (
                "release-{major}.{minor}.{patch}",
                "release-0.3.0",
                "release-0.3.1",
            ),
            (
                "{major}.{minor}.{patch}-alpha.{n}.{n}",
                "0.154.0-alpha.6.1",
                "0.154.0-alpha.6.2",
            ),
        ];

        for (source, tag, bumped) in cases {
            let template = parse(source);
            let version = template.parse_version(tag).unwrap();
            assert_eq!(template.render(&version).as_ref(), tag);
            assert_eq!(
                template
                    .render(&version.bump(TagSlot::RIGHTMOST).unwrap())
                    .as_ref(),
                bumped
            );
        }
    }

    #[test]
    fn bumping_a_slot_resets_every_slot_to_its_right() {
        let template = parse("{major}.{minor}.{patch}-alpha.{n}.{n}");
        let version = template.parse_version("0.154.0-alpha.6.1").unwrap();

        assert_eq!(
            template.render(&version.bump(slot(1)).unwrap()).as_ref(),
            "0.154.0-alpha.7.0"
        );
        assert_eq!(
            template.render(&version.bump(slot(2)).unwrap()).as_ref(),
            "0.154.1-alpha.0.0"
        );
        assert_eq!(
            template.render(&version.bump(slot(4)).unwrap()).as_ref(),
            "1.0.0-alpha.0.0"
        );
    }

    #[test]
    fn matching_is_anchored_and_rejects_leading_zeros() {
        let template = parse("{major}.{minor}.{patch}");

        assert!(template.parse_version("v0.19.1").is_none());
        assert!(template.parse_version("0.19.1-rc").is_none());
        assert!(template.parse_version("0.019.1").is_none());
        assert!(template.parse_version("0.19").is_none());
        assert!(template.parse_version("release-0.19.1").is_none());
        assert!(template.parse_version("0.0.0").is_some());
    }

    #[test]
    fn semver_names_resolve_to_slots_by_position() {
        let template = parse("{major}.{minor}.{patch}-alpha.{n}.{n}");

        assert_eq!(template.slot_of(SemverComponent::Patch), Some(slot(2)));
        assert_eq!(template.slot_of(SemverComponent::Major), Some(slot(4)));
        assert_eq!(template.component_at(slot(3)), Some(SemverComponent::Minor));
        assert_eq!(template.component_at(slot(0)), None);
        assert_eq!(parse("build-{n}").slot_of(SemverComponent::Patch), None);
    }

    #[test]
    fn rejects_malformed_templates() {
        let cases = [
            ("", TagTemplateError::Blank),
            ("release {n}", TagTemplateError::Whitespace),
            ("release", TagTemplateError::NoSlots),
            (
                "{build}",
                TagTemplateError::UnknownPlaceholder {
                    placeholder: "build".into(),
                },
            ),
            (
                "{patch}.{patch}",
                TagTemplateError::DuplicateComponent {
                    component: SemverComponent::Patch,
                },
            ),
            ("{n}{n}", TagTemplateError::AdjacentSlots),
            ("{n}1", TagTemplateError::DigitAfterSlot),
            ("{n", TagTemplateError::UnclosedPlaceholder),
            ("n}", TagTemplateError::UnmatchedClosingBrace),
        ];

        for (source, expected) in cases {
            assert_eq!(source.parse::<TagTemplate>(), Err(expected), "{source}");
        }
        let too_many = "{n}.".repeat(17);
        assert_eq!(
            too_many.parse::<TagTemplate>(),
            Err(TagTemplateError::TooManySlots)
        );
    }

    #[test]
    fn bump_reports_missing_slots_and_overflow() {
        let template = parse("{major}.{minor}.{patch}");
        let version = template.zero_version();

        assert_eq!(
            version.bump(slot(3)),
            Err(TagVersionBumpError::SlotOutOfRange {
                slot: slot(3),
                slot_count: 3,
            })
        );
        let saturated = template.parse_version("1.2.18446744073709551615").unwrap();
        assert_eq!(
            saturated.bump(TagSlot::RIGHTMOST),
            Err(TagVersionBumpError::Overflow {
                slot: TagSlot::RIGHTMOST
            })
        );
    }

    #[test]
    fn versions_order_by_slot_tuple() {
        let template = parse("{major}.{minor}.{patch}");
        let mut versions = ["0.9.0", "0.10.0", "0.2.1"]
            .into_iter()
            .map(|tag| template.parse_version(tag).unwrap())
            .collect::<Vec<_>>();
        versions.sort();

        assert_eq!(
            versions
                .iter()
                .map(|version| template.render(version).to_string())
                .collect::<Vec<_>>(),
            vec!["0.2.1", "0.9.0", "0.10.0"]
        );
    }
}
