use gtl_application::{diffs::get_diff_text, ports::DiffTextReader};
use gtl_models::diffs::{DiffText, DiffTextId};

use crate::app_state::SqliteAppState;

impl DiffTextReader for SqliteAppState {
    fn diff_text(&self, id: &DiffTextId) -> anyhow::Result<Option<DiffText>> {
        get_diff_text::execute(id, &*self.connection_lock()?)
    }
}
