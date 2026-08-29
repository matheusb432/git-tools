//! Structured local tag values independent of Git transport and presentation.

use crate::{
    diffs::CommitId,
    git::{GitObjectId, TagName},
    timestamps::MachineTimestamp,
};

/// Identifies whether a local tag object is known by origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagState {
    /// Origin does not hold this tag object.
    Local,
    /// Origin's tag ref points to the same tag object.
    Remote,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TagKind {
    Annotated { message: Option<String> },
    Lightweight,
}

/// Describes one local tag and the commit it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    name: TagName,
    object: GitObjectId,
    commit: CommitId,
    created_at: Option<MachineTimestamp>,
    kind: TagKind,
    state: Option<TagState>,
}

impl Tag {
    /// Creates an annotated tag that resolves its tag object to `commit`.
    #[must_use]
    pub fn annotated(
        name: TagName,
        object: GitObjectId,
        commit: CommitId,
        created_at: Option<MachineTimestamp>,
        message: Option<String>,
    ) -> Self {
        Self {
            name,
            commit,
            created_at,
            object,
            kind: TagKind::Annotated { message },
            state: None,
        }
    }

    /// Creates a lightweight tag whose ref object is its resolved commit.
    #[must_use]
    pub fn lightweight(
        name: TagName,
        commit: CommitId,
        created_at: Option<MachineTimestamp>,
    ) -> Self {
        Self {
            name,
            object: GitObjectId::from(&commit),
            commit,
            created_at,
            kind: TagKind::Lightweight,
            state: None,
        }
    }

    /// Returns the full object identifier stored in the tag ref.
    #[must_use]
    pub const fn object(&self) -> &GitObjectId {
        &self.object
    }

    /// Returns the full commit identifier the tag resolves to.
    #[must_use]
    pub const fn commit(&self) -> &CommitId {
        &self.commit
    }

    /// Returns the local tag name without the `refs/tags/` prefix.
    #[must_use]
    pub const fn name(&self) -> &TagName {
        &self.name
    }

    /// Returns the tag's creation timestamp when Git reports one.
    #[must_use]
    pub const fn created_at(&self) -> Option<&MachineTimestamp> {
        self.created_at.as_ref()
    }

    /// Returns the first line of an annotated tag message.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        match &self.kind {
            TagKind::Annotated { message } => message.as_deref(),
            TagKind::Lightweight => None,
        }
    }

    /// Returns whether the tag has its own annotated tag object.
    #[must_use]
    pub fn is_annotated(&self) -> bool {
        matches!(self.kind, TagKind::Annotated { .. })
    }

    /// Returns the tag's origin state, or `None` when origin was not queried.
    #[must_use]
    pub fn state(&self) -> Option<TagState> {
        self.state
    }

    /// Records whether origin holds this tag object.
    pub fn set_state(&mut self, state: TagState) {
        self.state = Some(state);
    }
}

#[cfg(test)]
mod tests {
    use super::{Tag, TagState};
    use crate::git::{GitObjectId, TagName};

    const COMMIT_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn commit_id() -> crate::diffs::CommitId {
        COMMIT_ID.try_into().unwrap()
    }

    fn tag_name(raw: &str) -> TagName {
        TagName::try_new(raw.to_owned()).unwrap()
    }

    fn created_at(seconds: i64) -> crate::timestamps::MachineTimestamp {
        crate::timestamps::MachineTimestamp::from_unix_seconds(seconds).unwrap()
    }

    #[test]
    fn lightweight_tag_uses_its_commit_as_the_ref_object() {
        let tag = Tag::lightweight(tag_name("stable"), commit_id(), Some(created_at(100)));

        assert_eq!(tag.object().as_ref(), COMMIT_ID);
        assert_eq!(tag.message(), None);
        assert!(!tag.is_annotated());
        assert_eq!(tag.state(), None);
    }

    #[test]
    fn annotated_tag_keeps_its_distinct_ref_object() {
        let tag = Tag::annotated(
            tag_name("v1.0.0"),
            GitObjectId::try_new("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
            commit_id(),
            Some(created_at(100)),
            Some("release".into()),
        );

        assert_eq!(
            tag.object().as_ref(),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(tag.commit().as_ref(), COMMIT_ID);
        assert_eq!(tag.message(), Some("release"));
        assert!(tag.is_annotated());
    }

    #[test]
    fn state_transition_changes_only_the_tag_state() {
        let mut tag = Tag::lightweight(tag_name("stable"), commit_id(), Some(created_at(100)));
        let original_object = tag.object().clone();

        tag.set_state(TagState::Remote);

        assert_eq!(tag.state(), Some(TagState::Remote));
        assert_eq!(tag.object(), &original_object);
    }
}
