use anyhow::{Context as _, bail};
use gtl_models::diffs::{DiffText, DiffTextId};
use rusqlite::{Connection, OptionalExtension as _, params};

#[cqrsy::query]
pub fn execute(id: &DiffTextId, connection: &Connection) -> anyhow::Result<Option<DiffText>> {
    let content = connection
        .query_row(
            "SELECT content FROM diff_texts WHERE text_id = ?1",
            params![id.as_ref()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(content) = content else {
        return Ok(None);
    };
    let text = DiffText::try_new(content).context("stored diff text exceeds its size limit")?;
    if text.id() != id {
        bail!("stored diff text {id} no longer matches its identity");
    }
    Ok(Some(text))
}
