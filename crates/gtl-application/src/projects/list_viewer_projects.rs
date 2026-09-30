use anyhow::Context as _;
use gtl_models::projects::catalogue::{
    ProjectDirectorySource, ProjectId, ProjectStatus, ProjectStatusFilter,
};
use gtl_wire::viewer::projects::{
    ListViewerProjects, ViewerProject, ViewerProjectPage, ViewerProjectsCursor,
};
use rusqlite::{Connection, Row, params};

const PROJECT_SELECT: &str = "SELECT p.id, p.title, s.source_value, p.comparison_branch,
    p.paused_at IS NOT NULL
    FROM projects p JOIN project_sources s USING (source_id)
    WHERE p.unmanaged_at IS NULL AND s.source_kind = 'directory'";

const fn status_predicate(filter: ProjectStatusFilter) -> &'static str {
    match filter {
        ProjectStatusFilter::Active => "p.paused_at IS NULL",
        ProjectStatusFilter::Paused => "p.paused_at IS NOT NULL",
        ProjectStatusFilter::All => "TRUE",
    }
}

fn count_projects(filter: ProjectStatusFilter, connection: &Connection) -> anyhow::Result<u32> {
    let sql = format!(
        "SELECT count(*) FROM projects p WHERE p.unmanaged_at IS NULL AND {}",
        status_predicate(filter)
    );
    Ok(connection.query_row(&sql, [], |row| row.get(0))?)
}

#[cqrsy::query]
pub fn execute(
    request: &ListViewerProjects,
    connection: &Connection,
) -> anyhow::Result<ViewerProjectPage> {
    if let Some(sort) = request.sort {
        return sorted_page(request, sort, connection);
    }
    let transaction = connection.unchecked_transaction()?;
    let connection = &transaction;
    let status = status_predicate(request.status);
    let total = count_projects(request.status, connection)?;
    let page_size = request.page_size.into_inner();
    let (predicate, cursor, reverse, limit) = match &request.cursor {
        ViewerProjectsCursor::First => ("", None, false, page_size),
        ViewerProjectsCursor::After(id) => ("AND p.id > ?2", Some(id.as_ref()), false, page_size),
        ViewerProjectsCursor::Before(id) => ("AND p.id < ?2", Some(id.as_ref()), true, page_size),
        ViewerProjectsCursor::Last => ("", None, true, (total.saturating_sub(1) % page_size) + 1),
    };
    let order = if reverse { "DESC" } else { "ASC" };
    let sql = format!("{PROJECT_SELECT} AND {status} {predicate} ORDER BY p.id {order} LIMIT ?1");
    let mut statement = connection.prepare_cached(&sql)?;
    let mut rows = match cursor {
        Some(cursor) => statement.query(params![limit, cursor])?,
        None => statement.query([limit])?,
    };
    let mut projects = Vec::new();
    while let Some(row) = rows.next()? {
        projects.push(read_project(row)?);
    }
    if reverse {
        projects.reverse();
    }
    let count_before = if let Some(project) = projects.first() {
        connection.query_row(
            &format!(
                "SELECT count(*) FROM projects p WHERE p.unmanaged_at IS NULL AND {status} AND p.id < ?1"
            ),
            [project.id.as_ref()],
            |row| row.get(0),
        )?
    } else if reverse {
        0
    } else {
        total
    };
    ViewerProjectPage::try_new(projects, total, count_before).context("invalid stored project page")
}

pub fn status_refresh_candidates(
    filter: ProjectStatusFilter,
    connection: &Connection,
) -> anyhow::Result<Vec<ViewerProject>> {
    let mut statement = connection.prepare_cached(&format!(
        "SELECT p.id, p.title, s.source_value, p.comparison_branch, p.paused_at IS NOT NULL
         FROM projects p JOIN project_sources s USING (source_id)
         LEFT JOIN project_status_index i ON i.project_id = p.id
         WHERE p.unmanaged_at IS NULL AND {}
           AND (i.project_id IS NULL OR i.source_id != p.source_id
                OR i.comparison_branch != p.comparison_branch OR i.checked_at < unixepoch() - 60)
         ORDER BY i.checked_at, p.id LIMIT ?1",
        status_predicate(filter)
    ))?;
    let mut rows = statement.query([gtl_models::projects::catalogue::PROJECTS_MAX])?;
    let mut projects = Vec::new();
    while let Some(row) = rows.next()? {
        projects.push(read_project(row)?);
    }
    Ok(projects)
}

fn sorted_page(
    request: &ListViewerProjects,
    sort: gtl_models::settings::ProjectsSort,
    connection: &Connection,
) -> anyhow::Result<ViewerProjectPage> {
    use gtl_models::settings::ProjectsSort;
    let transaction = connection.unchecked_transaction()?;
    let connection = &transaction;
    let total = count_projects(request.status, connection)?;
    let order = match sort {
        ProjectsSort::Changes => {
            "changes_priority DESC, commits_ahead DESC, title COLLATE NOCASE, id"
        }
        ProjectsSort::ChangesAscending => {
            "changes_priority = -1, changes_priority, commits_ahead, title COLLATE NOCASE, id"
        }
        ProjectsSort::Name => "title COLLATE NOCASE, id",
        ProjectsSort::NameDescending => "title COLLATE NOCASE DESC, id",
        ProjectsSort::Branch => "branch IS NULL, branch COLLATE NOCASE, title COLLATE NOCASE, id",
        ProjectsSort::BranchDescending => {
            "branch IS NULL, branch COLLATE NOCASE DESC, title COLLATE NOCASE, id"
        }
    };
    let page_size = request.page_size.into_inner();
    let (predicate, cursor, reverse, limit) = match &request.cursor {
        ViewerProjectsCursor::First => ("", None, false, page_size),
        ViewerProjectsCursor::After(id) => (
            "WHERE position > (SELECT position FROM ordered WHERE id = ?2)",
            Some(id.as_ref()),
            false,
            page_size,
        ),
        ViewerProjectsCursor::Before(id) => (
            "WHERE position < (SELECT position FROM ordered WHERE id = ?2)",
            Some(id.as_ref()),
            true,
            page_size,
        ),
        ViewerProjectsCursor::Last => ("", None, true, (total.saturating_sub(1) % page_size) + 1),
    };
    let direction = if reverse { "DESC" } else { "ASC" };
    let sql = format!(
        "WITH indexed AS (
            SELECT p.id, p.title, s.source_value, p.comparison_branch,
                p.paused_at IS NOT NULL AS paused, i.branch, i.commits_ahead,
                CASE WHEN i.commits_ahead > 0 OR i.tracked_changes = 1 OR i.untracked_changes = 1
                     THEN 4 * coalesce(i.commits_ahead > 0, 0) + 2 * coalesce(i.tracked_changes, 0) + coalesce(i.untracked_changes, 0)
                     WHEN i.commits_ahead IS NOT NULL AND i.tracked_changes IS NOT NULL THEN 0
                     ELSE -1 END AS changes_priority
            FROM projects p JOIN project_sources s USING (source_id)
            LEFT JOIN project_status_index i ON i.project_id = p.id AND i.source_id = p.source_id
                AND i.comparison_branch = p.comparison_branch
            WHERE p.unmanaged_at IS NULL AND {status}
         ), ordered AS (SELECT *, row_number() OVER (ORDER BY {order}) AS position FROM indexed)
         SELECT id, title, source_value, comparison_branch, paused, position FROM ordered
         {predicate} ORDER BY position {direction} LIMIT ?1",
        status = status_predicate(request.status),
    );
    let mut statement = connection.prepare_cached(&sql)?;
    let mut rows = match cursor {
        Some(cursor) => statement.query(params![limit, cursor])?,
        None => statement.query([limit])?,
    };
    let mut projects = Vec::new();
    let mut count_before = total;
    while let Some(row) = rows.next()? {
        let position: u32 = row.get(5)?;
        count_before = count_before.min(position - 1);
        projects.push(read_project(row)?);
    }
    if reverse {
        projects.reverse();
    }
    ViewerProjectPage::try_new(projects, total, count_before).context("invalid sorted project page")
}

pub fn get_project(
    id: &ProjectId,
    connection: &Connection,
) -> anyhow::Result<Option<ViewerProject>> {
    let mut statement = connection.prepare_cached(&format!("{PROJECT_SELECT} AND p.id = ?1"))?;
    let mut rows = statement.query([id.as_ref()])?;
    rows.next()?.map(read_project).transpose()
}

fn read_project(row: &Row<'_>) -> anyhow::Result<ViewerProject> {
    let source: ProjectDirectorySource = row.get::<_, String>(2)?.try_into()?;
    let path = source.resolve()?;
    Ok(ViewerProject {
        id: row.get::<_, String>(0)?.try_into()?,
        name: row.get::<_, String>(1)?.try_into()?,
        path,
        comparison_branch: row.get::<_, String>(3)?.try_into()?,
        status: if row.get(4)? {
            ProjectStatus::Paused
        } else {
            ProjectStatus::Active
        },
    })
}
