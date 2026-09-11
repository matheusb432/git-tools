use std::path::Path;

use rusqlite::Connection;

#[derive(Clone, Copy)]
pub struct AssociateRenderProjects<'a> {
    pub home: &'a Path,
}

#[cqrsy::command]
pub fn execute(
    request: AssociateRenderProjects<'_>,
    connection: &Connection,
) -> anyhow::Result<()> {
    let home = request.home.to_string_lossy().replace('\\', "/");
    connection.execute(
        "UPDATE recent_renders SET project_id = (
            SELECT p.id FROM projects p
            JOIN project_sources s ON s.source_id = p.source_id
            JOIN render_sources r ON r.kind = s.source_kind
                AND replace(r.value, char(92), '/') = ?1 || substr(s.source_value, 2)
            WHERE r.id = recent_renders.source_id
        ) WHERE project_id IS NULL",
        [home.trim_end_matches('/')],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerHistoryFilter;

    use super::*;
    use crate::history::{
        associate_render_projects,
        list_recent_render_page::{self, ListRecentRenderPage},
        persistence::{seed_recent_render, store_test},
    };

    #[test]
    fn associations_use_exact_home_paths_and_survive_repeated_backfills() {
        let connection = store_test();
        seed_recent_render(&connection, 1, "snapshot");
        connection
            .execute_batch(
                "INSERT INTO project_sources VALUES (1, 'directory', '~/gt');
            INSERT INTO projects VALUES ('GT', 1, 'git-tools');",
            )
            .unwrap();
        associate_render_projects::execute(
            AssociateRenderProjects {
                home: Path::new("/other-home"),
            },
            &connection,
        )
        .unwrap();
        let all = list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection)
            .unwrap();
        assert!(all.entries[0].project_id.is_none());
        associate_render_projects::execute(
            AssociateRenderProjects {
                home: Path::new("/repos"),
            },
            &connection,
        )
        .unwrap();
        associate_render_projects::execute(
            AssociateRenderProjects {
                home: Path::new("/other-home"),
            },
            &connection,
        )
        .unwrap();
        let filtered = list_recent_render_page::execute(
            &ListRecentRenderPage {
                filter: ViewerHistoryFilter::Project {
                    name: crate::utils::project_name("git-tools"),
                },
                ..Default::default()
            },
            &connection,
        )
        .unwrap();
        assert_eq!(
            filtered.entries[0].project_id.as_ref().unwrap().as_ref(),
            "GT"
        );
        let unassociated = list_recent_render_page::execute(
            &ListRecentRenderPage {
                filter: ViewerHistoryFilter::Unassociated,
                ..Default::default()
            },
            &connection,
        )
        .unwrap();
        assert!(unassociated.entries.is_empty());
    }
}
