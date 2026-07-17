//! Structured local tag values independent of Git transport and presentation.

/// Identifies whether a local tag object is known by the origin tracking refs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagState {
    /// The tag object is absent from origin's local tracking refs.
    Local,
    /// Origin's local tracking ref points to the same tag object.
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
    commit: String,
    commit_short: String,
    created_at: Option<i64>,
    kind: TagKind,
    state: TagState,
}

impl Tag {
    /// Creates an annotated tag that resolves its tag object to `commit`.
    pub fn annotated(
        name: String,
        object: String,
        commit: String,
        commit_short: String,
        created_at: Option<i64>,
        message: Option<String>,
    ) -> Self {
        Self {
            name,
            commit,
            commit_short,
            created_at,
            kind: TagKind::Annotated { object, message },
            state: TagState::Local,
        }
    }

    /// Creates a lightweight tag whose ref object is its resolved commit.
    pub fn lightweight(
        name: String,
        commit: String,
        commit_short: String,
        created_at: Option<i64>,
    ) -> Self {
        Self {
            name,
            commit,
            commit_short,
            created_at,
            kind: TagKind::Lightweight,
            state: TagState::Local,
        }
    }

    /// Returns the full object identifier stored in the tag ref.
    pub fn object(&self) -> &str {
        match &self.kind {
            TagKind::Annotated { object, .. } => object,
            TagKind::Lightweight => &self.commit,
        }
    }

    /// Returns the full commit identifier the tag resolves to.
    pub fn commit(&self) -> &str {
        &self.commit
    }

    /// Returns the abbreviated commit identifier used for presentation.
    pub fn commit_short(&self) -> &str {
        &self.commit_short
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

    /// Returns whether the tag is local-only or known by origin.
    pub fn state(&self) -> TagState {
        self.state
    }

    /// Marks the local tag object as known by origin.
    pub fn mark_remote(&mut self) {
        self.state = TagState::Remote;
    }
}

#[cfg(test)]
mod tests {
    use super::{Tag, TagState};

    #[test]
    fn lightweight_tag_uses_its_commit_as_the_ref_object() {
        let tag = Tag::lightweight(
            "stable".into(),
            "commit-a".into(),
            "commit".into(),
            Some(100),
        );

        assert_eq!(tag.object(), "commit-a");
        assert_eq!(tag.message(), None);
        assert!(!tag.is_annotated());
        assert_eq!(tag.state(), TagState::Local);
    }

    #[test]
    fn annotated_tag_keeps_its_distinct_ref_object() {
        let tag = Tag::annotated(
            "v1.0.0".into(),
            "tag-object".into(),
            "commit-a".into(),
            "commit".into(),
            Some(100),
            Some("release".into()),
        );

        assert_eq!(tag.object(), "tag-object");
        assert_eq!(tag.commit(), "commit-a");
        assert_eq!(tag.message(), Some("release"));
        assert!(tag.is_annotated());
    }

    #[test]
    fn remote_tracking_transition_changes_only_the_tag_state() {
        let mut tag = Tag::lightweight(
            "stable".into(),
            "commit-a".into(),
            "commit".into(),
            Some(100),
        );
        let original_object = tag.object().to_string();

        tag.mark_remote();

        assert_eq!(tag.state(), TagState::Remote);
        assert_eq!(tag.object(), original_object);
    }
}
