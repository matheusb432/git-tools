use rusqlite::Connection;

#[cqrsy::command]
pub fn execute(_: (), connection: &Connection) -> anyhow::Result<()> {
    connection.execute(
        "UPDATE recent_renders SET project_id = (
            SELECT p.id FROM projects p
            JOIN project_sources s ON s.source_id = p.source_id
            JOIN render_sources r ON r.kind = s.source_kind
                AND r.value = s.source_value
            WHERE r.id = recent_renders.source_id
        ) WHERE project_id IS NULL",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use gtl_wire::viewer::ViewerHistoryFilter;

    use crate::history::{
        associate_render_projects,
        list_recent_render_page::{self, ListRecentRenderPage},
        persistence::{seed_recent_render, store_test},
    };

    #[test]
    fn associations_use_exact_absolute_paths_and_survive_repeated_backfills() {
        let connection = store_test();
        seed_recent_render(&connection, 1, "snapshot");
        connection
            .execute_batch(
                "INSERT INTO project_sources VALUES (1, 'directory', '/other-home/gt');
            INSERT INTO projects VALUES ('GT', 1, 'git-tools');",
            )
            .unwrap();
        associate_render_projects::execute((), &connection).unwrap();
        let all = list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection)
            .unwrap();
        assert!(all.entries[0].project_id.is_none());
        connection
            .execute("UPDATE project_sources SET source_value = '/repos/gt'", [])
            .unwrap();
        associate_render_projects::execute((), &connection).unwrap();
        let all = list_recent_render_page::execute(&ListRecentRenderPage::default(), &connection)
            .unwrap();
        assert_eq!(all.entries[0].project_id.as_ref().unwrap().as_ref(), "GT");
        associate_render_projects::execute((), &connection).unwrap();
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
