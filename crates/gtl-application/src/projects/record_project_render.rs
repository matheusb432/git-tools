use gtl_models::{paths::RepositoryRoot, timestamps::MachineTimestamp};
use rusqlite::{Connection, params};

pub struct RecordProjectRender {
    pub path: RepositoryRoot,
    pub rendered_at: MachineTimestamp,
}

#[cqrsy::command]
pub fn execute(request: &RecordProjectRender, connection: &Connection) -> anyhow::Result<()> {
    connection.execute(
        "INSERT INTO project_render_recency (source_value, rendered_at) VALUES (?1, ?2)
         ON CONFLICT(source_value) DO UPDATE SET rendered_at = excluded.rendered_at",
        params![request.path.to_string(), request.rendered_at.as_ref()],
    )?;
    Ok(())
}
