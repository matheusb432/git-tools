use gtl_models::diffs::{DiffText, DiffTextId};

/// Reads diff text stored outside a repository.
pub trait DiffTextReader {
    /// Returns `None` once the text has been pruned.
    fn diff_text(&self, id: &DiffTextId) -> anyhow::Result<Option<DiffText>>;
}
