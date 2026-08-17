use gtl_application::viewer::{project_diff_lines, project_render_options};
use gtl_wire::viewer::{LoadViewerDiffLines, ViewerApiError, ViewerDiffLines, ViewerViewIdentity};

use super::{internal, settings, shell};
use crate::presentation::ViewerApp;

pub(super) fn load_lines(
    app: &ViewerApp,
    request: &LoadViewerDiffLines,
) -> Result<ViewerDiffLines, ViewerApiError> {
    let options = validated_current_options(app, request.identity)?;
    let snapshot = {
        let mut session = app
            .session
            .lock()
            .map_err(|error| internal("failed to lock viewer session", error))?;
        let snapshot = session
            .active_content_snapshot()
            .ok_or(ViewerApiError::Conflict)?;
        if !shell::identity_matches(request.identity, snapshot.identity(), options) {
            return Err(ViewerApiError::Conflict);
        }
        snapshot
    };
    let page = project_diff_lines(snapshot.view(), request)?;
    validate_current_request(app, request.identity, options)?;
    Ok(page)
}

pub(super) fn validated_current_options(
    app: &ViewerApp,
    identity: ViewerViewIdentity,
) -> Result<gtl_models::viewer::RenderOptions, ViewerApiError> {
    let options = settings::load(app)?.viewer_render_options();
    if project_render_options(options) != identity.render_options {
        return Err(ViewerApiError::Conflict);
    }
    Ok(options)
}

fn validate_content_identity(
    app: &ViewerApp,
    expected: ViewerViewIdentity,
    options: gtl_models::viewer::RenderOptions,
) -> Result<(), ViewerApiError> {
    let session = app
        .session
        .lock()
        .map_err(|error| internal("failed to lock viewer session", error))?;
    let current = session
        .active_content_identity()
        .ok_or(ViewerApiError::Conflict)?;
    if !shell::identity_matches(expected, current, options) {
        return Err(ViewerApiError::Conflict);
    }
    Ok(())
}

pub(super) fn validate_current_request(
    app: &ViewerApp,
    expected: ViewerViewIdentity,
    options: gtl_models::viewer::RenderOptions,
) -> Result<(), ViewerApiError> {
    if validated_current_options(app, expected)? != options {
        return Err(ViewerApiError::Conflict);
    }
    validate_content_identity(app, expected, options)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gtl_application::{
        diffs::{Cmd, FileDiff, Foot, View},
        viewer::{RenderOptions, ViewerTabKind},
    };
    use gtl_infra::user_config::TomlSettingsStore;
    use gtl_models::{diffs::DiffLineCount, recipes::RecipeBatchId};
    use gtl_wire::{
        recipes::{Recipe, RecipeOp, RecipeSource},
        viewer::{SetViewerPreference, ViewerDiffCursor, ViewerDiffFileId, ViewerResource},
    };

    use super::*;
    use crate::{
        presentation::ViewerApp,
        session::{CachedView, ViewCacheWeight},
        testing::{
            git_head, git_revision, project_name, repository_relative_path, repository_root,
        },
    };

    fn ready_app_with_file(file: FileDiff) -> (tempfile::TempDir, ViewerApp, ViewerViewIdentity) {
        let directory = tempfile::tempdir().expect("temporary viewer data");
        let settings_path = directory.path().join("config.toml");
        let app = ViewerApp::open(
            directory.path(),
            TomlSettingsStore::new(Some(settings_path)),
            ViewCacheWeight::new(1024 * 1024),
        )
        .expect("open viewer app");
        let mut session = app.session.lock().expect("viewer session");
        let tab_id = session
            .open(
                Recipe {
                    source: RecipeSource::LocalRepo(repository_root("/repo")),
                    op: RecipeOp::MergeDiff {
                        base: None,
                        pinned: None,
                    },
                    name: None,
                },
                RecipeBatchId::generate(),
                ViewerTabKind::Snapshot,
            )
            .expect("tab id");
        let ticket = session.begin_compute(tab_id).expect("compute ticket");
        let view = Arc::new(View {
            repo_name: project_name("git-tools"),
            repo_root: repository_root("/repo"),
            branch: git_head("feature"),
            upstream: git_revision("main"),
            commits: Vec::new(),
            files: vec![file],
            title: "Feature diff".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot { cmd: String::new() },
            exclusions: None,
        });
        assert_eq!(
            session.publish_labeled_if_current(ticket, CachedView::new(view), "ready".into()),
            crate::session::PublishOutcome::Published
        );
        let identity = shell::to_identity(
            session.active_content_identity().expect("active identity"),
            RenderOptions::DEFAULT,
        );
        drop(session);
        (directory, app, identity)
    }

    fn ready_app() -> (tempfile::TempDir, ViewerApp, ViewerViewIdentity) {
        ready_app_with_file(FileDiff {
            path: repository_relative_path("src/lib.rs"),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::default(),
            lines: vec!["@@ -0,0 +1 @@".into(), "+server rendered".into()],
            full_lines: None,
        })
    }

    #[test]
    fn raw_line_pages_are_addressable_and_echo_the_validated_identity() {
        let (_directory, app, identity) = ready_app();
        let file = ViewerDiffFileId::for_index(0);

        let page = load_lines(
            &app,
            &LoadViewerDiffLines {
                identity,
                file: file.clone(),
                cursor: ViewerDiffCursor::default(),
            },
        )
        .expect("raw diff lines");

        assert_eq!(page.identity, identity);
        assert_eq!(page.file, file);
        assert_eq!(page.cursor, ViewerDiffCursor::default());
        assert_eq!(page.lines, ["@@ -0,0 +1 @@", "+server rendered"]);
        assert_eq!(page.next, None);
    }

    #[test]
    fn full_density_pages_select_the_available_full_diff() {
        let (_directory, app, mut identity) = ready_app_with_file(FileDiff {
            path: repository_relative_path("src/lib.rs"),
            added: DiffLineCount::new(1),
            removed: DiffLineCount::new(1),
            lines: vec!["+compact".into()],
            full_lines: Some(vec!["-full old".into(), "+full new".into()]),
        });
        super::super::actions::set_preference(
            &app,
            SetViewerPreference::Density(gtl_wire::viewer::ViewerDiffDensity::Full),
        )
        .expect("set full density");
        identity.render_options.density = gtl_wire::viewer::ViewerDiffDensity::Full;

        let page = load_lines(
            &app,
            &LoadViewerDiffLines {
                identity,
                file: ViewerDiffFileId::for_index(0),
                cursor: ViewerDiffCursor::default(),
            },
        )
        .expect("full diff lines");

        assert_eq!(page.lines, ["-full old", "+full new"]);
    }

    #[test]
    fn raw_line_pages_reject_unknown_files_and_out_of_range_cursors() {
        let (_directory, app, identity) = ready_app();
        let unknown = load_lines(
            &app,
            &LoadViewerDiffLines {
                identity,
                file: ViewerDiffFileId::for_index(99),
                cursor: ViewerDiffCursor::default(),
            },
        );

        assert_eq!(
            unknown,
            Err(ViewerApiError::NotFound {
                resource: ViewerResource::DiffFile,
            })
        );
        assert_eq!(
            load_lines(
                &app,
                &LoadViewerDiffLines {
                    identity,
                    file: ViewerDiffFileId::for_index(0),
                    cursor: ViewerDiffCursor::new(3),
                }
            ),
            Err(ViewerApiError::InvalidRequest)
        );

        let mut stale_identity = identity;
        stale_identity.range_generation = stale_identity.range_generation.next();
        assert_eq!(
            load_lines(
                &app,
                &LoadViewerDiffLines {
                    identity: stale_identity,
                    file: ViewerDiffFileId::for_index(0),
                    cursor: ViewerDiffCursor::default(),
                }
            ),
            Err(ViewerApiError::Conflict)
        );
    }
}
