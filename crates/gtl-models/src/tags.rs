//! Structured local tag values independent of Git transport and presentation.

use crate::diffs::CommitId;

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
    Annotated {
        object: String,
        message: Option<String>,
    },
    Lightweight,
}

/// Describes one local tag and the commit it resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    name: String,
    commit: CommitId,
    created_at: Option<i64>,
    kind: TagKind,
    state: Option<TagState>,
}

impl Tag {
    /// Creates an annotated tag that resolves its tag object to `commit`.
    pub fn annotated(
        name: String,
        object: String,
        commit: CommitId,
        created_at: Option<i64>,
        message: Option<String>,
    ) -> Self {
        Self {
            name,
            commit,
            created_at,
            kind: TagKind::Annotated { object, message },
            state: None,
        }
    }

    /// Creates a lightweight tag whose ref object is its resolved commit.
    pub fn lightweight(name: String, commit: CommitId, created_at: Option<i64>) -> Self {
        Self {
            name,
            commit,
            created_at,
            kind: TagKind::Lightweight,
            state: None,
        }
    }

    /// Returns the full object identifier stored in the tag ref.
    pub fn object(&self) -> &str {
        match &self.kind {
            TagKind::Annotated { object, .. } => object,
            TagKind::Lightweight => self.commit.as_ref(),
        }
    }

    /// Returns the full commit identifier the tag resolves to.
    pub const fn commit(&self) -> &CommitId {
        &self.commit
    }

    /// Returns the local tag name without the `refs/tags/` prefix.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the tag's Unix creation time when Git reports one.
    pub fn created_at(&self) -> Option<i64> {
        self.created_at
    }

    /// Returns the first line of an annotated tag message.
    pub fn message(&self) -> Option<&str> {
        match &self.kind {
            TagKind::Annotated { message, .. } => message.as_deref(),
            TagKind::Lightweight => None,
        }
    }

    /// Returns whether the tag has its own annotated tag object.
    pub fn is_annotated(&self) -> bool {
        matches!(self.kind, TagKind::Annotated { .. })
    }

    /// Returns the tag's origin state, or `None` when origin was not queried.
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

    const COMMIT_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn commit_id() -> crate::diffs::CommitId {
        COMMIT_ID.try_into().expect("fixture commit ID is valid")
    }

    #[test]
    fn lightweight_tag_uses_its_commit_as_the_ref_object() {
        let tag = Tag::lightweight("stable".into(), commit_id(), Some(100));

        assert_eq!(tag.object(), COMMIT_ID);
        assert_eq!(tag.message(), None);
        assert!(!tag.is_annotated());
        assert_eq!(tag.state(), None);
    }

    #[test]
    fn annotated_tag_keeps_its_distinct_ref_object() {
        let tag = Tag::annotated(
            "v1.0.0".into(),
            "tag-object".into(),
            commit_id(),
            Some(100),
            Some("release".into()),
        );

        assert_eq!(tag.object(), "tag-object");
        assert_eq!(tag.commit().as_ref(), COMMIT_ID);
        assert_eq!(tag.message(), Some("release"));
        assert!(tag.is_annotated());
    }

    #[test]
    fn state_transition_changes_only_the_tag_state() {
        let mut tag = Tag::lightweight("stable".into(), commit_id(), Some(100));
        let original_object = tag.object().to_string();

        tag.set_state(TagState::Remote);

        assert_eq!(tag.state(), Some(TagState::Remote));
        assert_eq!(tag.object(), original_object);
    }
}
