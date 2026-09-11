use anyhow::Context as _;
use directories::BaseDirs;
use gtl_application::history::associate_render_projects::{self, AssociateRenderProjects};

use crate::app_state::SqliteAppState;

impl SqliteAppState {
    pub fn associate_render_projects(&self) -> anyhow::Result<()> {
        let directories = BaseDirs::new().context("home directory is unavailable")?;
        let connection = self.connection_lock()?;
        associate_render_projects::execute(
            AssociateRenderProjects {
                home: directories.home_dir(),
            },
            &connection,
        )
    }
}
