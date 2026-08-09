use gtl_contracts::viewer::{
    LoadViewerDiffChunk, PrepareDiffDocument, ViewerApiError, ViewerDiffChunk,
    ViewerDiffChunkContinuation, ViewerDiffDocument, ViewerDiffMaterialization, ViewerResource,
    ViewerViewIdentity,
};

use super::{internal, settings, shell, unavailable};
use crate::{
    materialization::{
        MaterializationError, PreparedMaterialization, RenderedMaterialization, ViewLoadId,
        ViewMaterializations,
    },
    presentation::ViewerApp,
};

pub(super) fn prepare(
    app: &ViewerApp,
    request: PrepareDiffDocument,
) -> Result<ViewerDiffDocument, ViewerApiError> {
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
    let html = gtl_preview::diff_document_shell(snapshot.view(), options)
        .map_err(|error| internal("failed to render diff document", error))?
        .into_string();
    let rendered = ViewMaterializations::render_content(&snapshot, options)
        .map_err(map_materialization_error)?;
    let materialization = publish_rendered(app, rendered)?;
    validate_current_request(app, request.identity, options)?;
    Ok(ViewerDiffDocument {
        identity: request.identity,
        html,
        materialization: match materialization {
            PreparedMaterialization::Complete => ViewerDiffMaterialization::Complete,
            PreparedMaterialization::Loading(load_id) => ViewerDiffMaterialization::Loading {
                load_id: load_id.get(),
            },
        },
    })
}

pub(super) fn load(
    app: &ViewerApp,
    request: LoadViewerDiffChunk,
) -> Result<ViewerDiffChunk, ViewerApiError> {
    let load_id = ViewLoadId::try_new(request.load_id).ok_or(ViewerApiError::InvalidRequest)?;
    let options = validated_current_options(app, request.identity)?;
    validate_content_identity(app, request.identity, options)?;
    let page = app
        .materializations
        .next(&app.session, load_id, options)
        .map_err(map_materialization_error)?;
    validate_current_request(app, request.identity, options)?;
    Ok(ViewerDiffChunk {
        identity: request.identity,
        target_id: page.chunk.target_id,
        html: page.chunk.html,
        row_count: page.chunk.rows,
        continuation: if page.has_more {
            ViewerDiffChunkContinuation::More
        } else {
            ViewerDiffChunkContinuation::Complete
        },
    })
}

fn publish_rendered(
    app: &ViewerApp,
    rendered: RenderedMaterialization,
) -> Result<PreparedMaterialization, ViewerApiError> {
    settings::with_current(app, |_store, settings| {
        app.materializations
            .publish_content(&app.session, rendered, settings.viewer_render_options())
            .map_err(map_materialization_error)
    })
}

pub(super) fn validated_current_options(
    app: &ViewerApp,
    identity: ViewerViewIdentity,
) -> Result<gtl_models::viewer::RenderOptions, ViewerApiError> {
    let options = settings::load(app)?.viewer_render_options();
    if shell::to_render_options(options) != identity.render_options {
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

fn map_materialization_error(error: MaterializationError) -> ViewerApiError {
    match error {
        MaterializationError::Conflict => ViewerApiError::Conflict,
        MaterializationError::ExhaustedIds => unavailable(
            ViewerResource::DiffChunk,
            "diff materialization ids are exhausted",
            error,
        ),
        MaterializationError::Render(_) | MaterializationError::StatePoisoned => {
            internal("diff materialization failed", error)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gtl_application::{
        diffs::{Cmd, FileDiff, Foot, View},
        viewer::{RenderOptions, ViewerTabKind},
    };
    use gtl_contracts::{
        recipes::{Recipe, RecipeOp, RecipeSource},
        viewer::{SetViewerPreference, ViewerDiffLayout},
    };
    use gtl_infra::user_config::TomlSettingsStore;

    use super::*;
    use crate::{presentation::ViewerApp, session::CachedView};

    fn ready_app() -> (tempfile::TempDir, ViewerApp, ViewerViewIdentity) {
        let directory = tempfile::tempdir().expect("temporary viewer data");
        let settings_path = directory.path().join("config.toml");
        let app = ViewerApp::open(
            directory.path(),
            TomlSettingsStore::new(Some(settings_path)),
            1024 * 1024,
        )
        .expect("open viewer app");
        let mut session = app.session.lock().expect("viewer session");
        let tab_id = session
            .open(
                Recipe {
                    source: RecipeSource::LocalRepo("/repo".into()),
                    op: RecipeOp::MergeDiff {
                        base: None,
                        pinned: None,
                    },
                    name: None,
                },
                "bridge-test".into(),
                ViewerTabKind::Snapshot,
            )
            .expect("tab id");
        let ticket = session.begin_compute(tab_id).expect("compute ticket");
        let view = Arc::new(View {
            repo_name: "git-tools".into(),
            repo_root: "/repo".into(),
            branch: "feature".into(),
            upstream: "main".into(),
            commits: Vec::new(),
            files: vec![FileDiff {
                path: "src/lib.rs".into(),
                added: 1,
                removed: 0,
                lines: vec!["@@ -0,0 +1 @@".into(), "+server rendered".into()],
                full_lines: None,
            }],
            title: "Feature diff".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: String::new(),
            foot: Foot {
                cmd: String::new(),
                note: String::new(),
            },
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

    #[test]
    fn document_and_chunks_keep_all_diff_rows_on_the_rust_boundary() {
        let (_directory, app, identity) = ready_app();
        let document = prepare(&app, PrepareDiffDocument { identity }).expect("diff document");

        assert!(document.html.contains("src/lib.rs"));
        assert!(!document.html.contains("+server rendered"));
        let ViewerDiffMaterialization::Loading { load_id } = document.materialization else {
            panic!("a nonempty diff must create a chunk chain");
        };

        let chunk = load(&app, LoadViewerDiffChunk { identity, load_id }).expect("diff chunk");

        assert!(chunk.html.contains("server rendered"));
        assert!(chunk.row_count > 0);
        assert_eq!(chunk.continuation, ViewerDiffChunkContinuation::Complete);
    }

    #[test]
    fn zero_load_id_is_rejected_before_materialization_lookup() {
        assert!(ViewLoadId::try_new(0).is_none());
    }

    #[test]
    fn materialization_conflicts_remain_retryable_api_conflicts() {
        assert_eq!(
            map_materialization_error(MaterializationError::Conflict),
            ViewerApiError::Conflict
        );
    }

    #[test]
    fn old_options_prepare_cannot_strand_the_current_options_chain() {
        let (_directory, app, identity_old) = ready_app();
        let options_old = validated_current_options(&app, identity_old).expect("old options");
        let snapshot = app
            .session
            .lock()
            .expect("viewer session")
            .active_content_snapshot()
            .expect("active content");
        let rendered_old = ViewMaterializations::render_content(&snapshot, options_old)
            .expect("render old options");

        super::super::actions::set_preference(
            &app,
            SetViewerPreference::Layout(ViewerDiffLayout::Split),
        )
        .expect("set current options");
        let options_current = settings::load(&app)
            .expect("current settings")
            .viewer_render_options();
        let identity_current = shell::to_identity(snapshot.identity(), options_current);
        let rendered_current = ViewMaterializations::render_content(&snapshot, options_current)
            .expect("render current options");
        let PreparedMaterialization::Loading(current_load) =
            publish_rendered(&app, rendered_current).expect("publish current options")
        else {
            panic!("a nonempty diff must create a chunk chain");
        };

        assert_eq!(
            publish_rendered(&app, rendered_old),
            Err(ViewerApiError::Conflict)
        );
        let chunk = load(
            &app,
            LoadViewerDiffChunk {
                identity: identity_current,
                load_id: current_load.get(),
            },
        )
        .expect("current chain remains available");
        assert!(chunk.html.contains("server rendered"));
    }
}
