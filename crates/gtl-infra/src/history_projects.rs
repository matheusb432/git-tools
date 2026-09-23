use gtl_application::history::associate_render_projects;

use crate::app_state::SqliteAppState;

impl SqliteAppState {
    pub fn associate_render_projects(&self) -> anyhow::Result<()> {
        let connection = self.connection_lock()?;
        associate_render_projects::execute((), &connection)
    }
}
