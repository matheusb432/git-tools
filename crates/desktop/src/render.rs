mod document;
mod fragments;
mod routes;

#[cfg(feature = "benchmark-support")]
pub use document::MaudViewerRenderer;
#[cfg(not(feature = "benchmark-support"))]
pub(crate) use document::MaudViewerRenderer;
pub(crate) use fragments::SwapFeedback;
use routes::{ViewerRoute, ViewerSettingChange};

// These values form the Rust side of the `data-viewer-state` DOM contract.
pub(crate) const VIEW_STATE_EMPTY: &str = "empty";
pub(crate) const VIEW_STATE_LOADING: &str = "loading";
pub(crate) const VIEW_STATE_READY: &str = "ready";
pub(crate) const VIEW_STATE_BROKEN: &str = "broken";
pub(crate) const VIEW_STATE_ERROR: &str = "error";

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application::{
        diffs::{Cmd, FileDiff, Foot, LineOwners, View},
        viewer::{
            DiffDensity, DiffLayout, RenderHistoryId, RenderOptions, Theme, ViewerDocument,
            ViewerHistoryEntry, ViewerSettings, ViewerTab, ViewerTabId, ViewerTabKind,
            ViewerTabState, ViewerView,
        },
    };
    use strum::VariantArray as _;

    use super::{
        MaudViewerRenderer, SwapFeedback, VIEW_STATE_BROKEN, VIEW_STATE_EMPTY, VIEW_STATE_ERROR,
        VIEW_STATE_LOADING, VIEW_STATE_READY, ViewerRoute, ViewerSettingChange,
    };
    use crate::materialization::ViewLoadId;

    const HTMX_SHA256: &str = "71ea67185bfa8c98c39d31717c6fce5d852370fcdfd129db4543774d3145c0de";

    fn tab_id(raw: u64) -> ViewerTabId {
        ViewerTabId::try_new(raw).expect("positive tab id")
    }

    fn history_id(raw: i64) -> RenderHistoryId {
        RenderHistoryId::try_new(raw).expect("positive history id")
    }

    fn sample_recipe() -> contracts::recipes::Recipe {
        contracts::recipes::Recipe {
            source: contracts::recipes::RecipeSource::LocalRepo("/repos/gt".into()),
            op: contracts::recipes::RecipeOp::Diff {
                target: contracts::recipes::RecipeTarget::Unpushed { pinned: None },
            },
            name: None,
        }
    }

    fn view() -> Arc<View> {
        Arc::new(View {
            exclusions: None,
            repo_name: "git-tools".into(),
            repo_root: "/repo".into(),
            branch: "feature/htmx".into(),
            upstream: "origin/main".into(),
            commits: vec![],
            files: vec![FileDiff {
                path: "src/a b.rs".into(),
                added: 1,
                removed: 0,
                lines: vec!["@@ -0,0 +1 @@".into(), "+new".into()],
                full_lines: None,
                commits: vec![],
                owners: LineOwners::default(),
            }],
            title: "Working tree".into(),
            cmd: Cmd {
                lead: String::new(),
                range: String::new(),
                trail: String::new(),
            },
            commits_label: "Commits".into(),
            foot: Foot {
                cmd: "git diff".into(),
                note: String::new(),
            },
        })
    }

    fn settings() -> ViewerSettings {
        ViewerSettings::new(
            RenderOptions::new(DiffLayout::Split, DiffDensity::Full),
            Theme::Hearth,
        )
    }

    fn sample_document() -> ViewerDocument {
        let id = tab_id(1);
        ViewerDocument::new(
            vec![ViewerTab::new(
                id,
                "git-tools changes".into(),
                ViewerTabKind::Live,
                ViewerTabState::Ready,
            )],
            Some(id),
            Some(ViewerView::new(
                id,
                view(),
                settings().options(),
                ViewerTabKind::Live,
            )),
            vec![ViewerHistoryEntry::new(
                history_id(7),
                "Recent changes".into(),
                "git-tools".into(),
                "main..HEAD".into(),
                "2026-07-11T00:00:00Z".into(),
                sample_recipe(),
            )],
            settings(),
        )
        .expect("sample document is internally consistent")
    }

    #[test]
    fn route_formatter_is_closed_over_validated_values() {
        let tab = tab_id(7);
        let options = RenderOptions::new(DiffLayout::Split, DiffDensity::Full);
        let render = history_id(42);

        assert_eq!(
            ViewerRoute::View { tab, options }.to_string(),
            "/tabs/7/view?layout=split&density=full"
        );
        assert_eq!(
            ViewerRoute::Activate { tab }.to_string(),
            "/tabs/7/activate"
        );
        assert_eq!(ViewerRoute::Refresh { tab }.to_string(), "/tabs/7/refresh");
        assert_eq!(ViewerRoute::Close { tab }.to_string(), "/tabs/7/close");
        assert_eq!(
            ViewerRoute::DeleteLiveView { tab }.to_string(),
            "/tabs/7/live-view"
        );
        assert_eq!(ViewerRoute::History.to_string(), "/history");
        assert_eq!(
            ViewerRoute::OpenHistory { render }.to_string(),
            "/history/42/open"
        );
        assert_eq!(
            ViewerRoute::Settings(ViewerSettingChange::Layout(DiffLayout::Unified)).to_string(),
            "/settings?layout=unified"
        );
        assert_eq!(
            ViewerRoute::Settings(ViewerSettingChange::Density(DiffDensity::Compact)).to_string(),
            "/settings?density=compact"
        );
        assert_eq!(
            ViewerRoute::Settings(ViewerSettingChange::Theme(Theme::Dark)).to_string(),
            "/settings?theme=dark"
        );
    }

    #[test]
    fn document_is_offline_and_has_stable_swap_roots_and_one_active_layout() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let expected_route = ViewerRoute::View {
            tab: tab_id(1),
            options: settings().options(),
        }
        .to_string();

        assert!(html.contains("id=\"viewer-tabs\""));
        assert!(html.contains("id=\"viewer-view\""));
        assert!(html.contains("id=\"viewer-history\""));
        assert!(html.contains(&expected_route.replace('&', "&amp;")));
        assert_eq!(html.matches(crate::protocol_config::APP_URL).count(), 1);
        assert!(!html.contains("<iframe"));
        assert!(!html.contains("<script src="));
        assert!(!html.contains("<link "));
        assert!(!html.contains("hx-swap-oob="));
        assert_eq!(html.matches("class=\"layout").count(), 1);
        assert!(html.contains("hx-post=\"/tabs/1/files/open?path="));
    }

    #[test]
    fn document_embeds_one_shared_preview_stylesheet() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let stylesheet = html
            .split_once("<style>")
            .and_then(|(_, tail)| tail.split_once("</style>"))
            .map(|(stylesheet, _)| stylesheet)
            .expect("document embeds a stylesheet");

        assert_eq!(html.matches("<style>").count(), 1);
        assert_eq!(stylesheet, preview::preview_css());
        assert!(!stylesheet.contains("viewer-toast-dismiss"));
    }

    #[test]
    fn document_keeps_server_owned_controls_outside_the_shared_layout() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let controls = html
            .find("aria-label=\"Diff display controls\"")
            .expect("ready view renders controls");
        let layout = html
            .find("class=\"layout")
            .expect("ready view renders a layout");

        assert!(controls < layout);
        assert!(html.contains("name=\"viewer-layout\""));
        assert!(html.contains("name=\"viewer-density\""));
        assert!(html.contains("hx-get=\"/settings?layout="));
        assert!(html.contains("hx-get=\"/settings?density="));
    }

    #[test]
    fn ready_document_connects_mobile_navigation_to_server_owned_popovers() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        for (label, target) in [
            ("Changed files", "viewer-files-popover"),
            ("Commits in range", "viewer-commits-popover"),
            ("View settings", "viewer-controls-popover"),
        ] {
            let button = html
                .split_once(&format!("aria-label=\"{label}\""))
                .and_then(|(head, _)| head.rsplit_once("<button"))
                .map(|(_, tag)| tag)
                .expect("mobile navigation button");

            assert!(button.contains(&format!("popovertarget=\"{target}\"")));
            assert!(html.contains(&format!("id=\"{target}\"")));
        }

        assert!(html.contains(">1</span>"));
    }

    #[test]
    fn ready_document_injects_viewer_actions_into_shared_mobile_controls() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        assert_eq!(
            html.matches(r#"class="preview-mobile-controls "#).count(),
            1
        );
        assert!(html.contains(r#"data-preview-action="fold-all""#));
        assert!(html.contains(r#"data-preview-action="toggle-context""#));
        assert!(html.contains(r#"name="viewer-layout-mobile""#));
        assert!(html.contains(r#"name="viewer-density-mobile""#));
        assert!(html.contains(r#"hx-get="/history""#));
        assert!(html.contains(r#"hx-get="/tabs/1/refresh""#));
        assert!(html.contains(r#"hx-delete="/tabs/1/live-view""#));
    }

    #[test]
    fn empty_document_disables_mobile_view_navigation() {
        let document = ViewerDocument::new(vec![], None, None, vec![], settings())
            .expect("empty viewer is valid");
        let html = MaudViewerRenderer.build_document(&document);

        for label in ["Changed files", "Commits in range", "View settings"] {
            let button = html
                .split_once(&format!("aria-label=\"{label}\""))
                .and_then(|(head, _)| head.rsplit_once("<button"))
                .map(|(_, tag)| tag)
                .expect("mobile navigation button");

            assert!(button.contains(" disabled"));
        }
    }

    #[test]
    fn viewer_fragments_keep_stable_htmx_and_inline_script_hooks() {
        let document = sample_document();
        let html = MaudViewerRenderer.build_document(&document);
        let feedback = MaudViewerRenderer.build_tabs_with_view(
            &document,
            SwapFeedback::SnapshotRecipesSkipped(&["api".into()]),
        );

        assert!(html.contains("id=\"viewer-tabs\""));
        assert!(html.contains("id=\"viewer-view\""));
        assert!(html.contains("id=\"viewer-history\""));
        assert!(html.contains("hx-target=\"#viewer-view\""));
        assert!(html.contains("hx-target=\"#viewer-tabs\""));
        assert!(html.contains("hx-target=\"#viewer-history\""));
        assert!(html.contains("data-viewer-theme="));
        assert!(feedback.contains("data-viewer-toast"));
    }

    #[test]
    fn viewer_view_marks_each_server_rendered_state_explicitly() {
        let ready = MaudViewerRenderer.build_view(&sample_document());
        let empty = ViewerDocument::new(vec![], None, None, vec![], settings())
            .expect("empty viewer is valid");
        let empty = MaudViewerRenderer.build_view(&empty);

        assert!(ready.contains(&format!("data-viewer-state=\"{VIEW_STATE_READY}\"")));
        assert!(empty.contains(&format!("data-viewer-state=\"{VIEW_STATE_EMPTY}\"")));

        for (state, expected) in [
            (
                ViewerTabState::Broken {
                    code: "missing".into(),
                    reason: "artifact gone".into(),
                },
                VIEW_STATE_BROKEN,
            ),
            (
                ViewerTabState::Error {
                    reason: "render failed".into(),
                },
                VIEW_STATE_ERROR,
            ),
        ] {
            let id = tab_id(3);
            let document = ViewerDocument::new(
                vec![ViewerTab::new(
                    id,
                    "unavailable".into(),
                    ViewerTabKind::Live,
                    state,
                )],
                Some(id),
                None,
                vec![],
                settings(),
            )
            .expect("non-ready active tabs intentionally have no view");
            let html = MaudViewerRenderer.build_view(&document);

            assert!(
                html.contains(&format!("data-viewer-state=\"{expected}\"")),
                "{html}"
            );
        }
    }

    #[test]
    fn deferred_document_paints_loading_chrome_before_rendering_ready_rows() {
        let html = MaudViewerRenderer.build_deferred_document(&sample_document());

        assert!(html.contains("id=\"viewer-loading-template\""));
        assert!(html.contains(&format!("data-viewer-state=\"{VIEW_STATE_LOADING}\"")));
        assert!(!html.contains("class=\"dl "));
    }

    #[test]
    fn materialized_view_contains_a_shell_and_one_bounded_loader_chain() {
        let load_id = ViewLoadId::try_new(17).expect("positive load id");
        let html =
            MaudViewerRenderer.build_materialized_view_with_tabs(&sample_document(), load_id);

        assert!(html.contains("id=\"viewer-diff-0\""));
        assert!(html.contains("id=\"viewer-chunk-loader\""));
        assert!(html.contains("hx-get=\"/loads/17/next\""));
        assert!(!html.contains("class=\"dl "));
    }

    #[test]
    fn viewer_fragment_owners_render_immediate_request_and_swap_feedback() {
        let document = sample_document();
        let tabs = MaudViewerRenderer.build_tabs(
            document.tabs(),
            document.active_tab_id(),
            settings().theme(),
        );
        let view = MaudViewerRenderer.build_view(&document);
        let history = MaudViewerRenderer.build_history(document.history());
        let motion_class = ["trans", "ition"].concat();

        assert!(view.contains("htmx-request"));
        for fragment in [tabs, view, history] {
            assert!(fragment.contains("htmx-swapping"), "{fragment}");
            assert!(fragment.contains("htmx-settling"), "{fragment}");
            assert!(!fragment.contains(&motion_class), "{fragment}");
            assert!(!fragment.contains("animate-"), "{fragment}");
        }
    }

    #[test]
    fn document_disables_htmx_runtime_transition_styles_and_smooth_scrolling() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let config = r#"<meta name="htmx-config" content="{&quot;includeIndicatorStyles&quot;:false,&quot;scrollBehavior&quot;:&quot;instant&quot;,&quot;globalViewTransitions&quot;:false}">"#;

        let config_position = html
            .find(config)
            .expect("document carries the exact htmx config");
        let htmx_position = html
            .find("window.htmx=htmx")
            .expect("document loads the vendored htmx runtime");

        assert!(config_position < htmx_position);
    }

    #[test]
    fn user_controlled_shell_values_are_escaped() {
        let tab = ViewerTab::new(
            tab_id(1),
            "<script>alert(1)</script>".into(),
            ViewerTabKind::Snapshot,
            ViewerTabState::Error {
                reason: "failed".into(),
            },
        );

        let html = MaudViewerRenderer.build_tabs(&[tab], None, settings().theme());

        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn broken_and_error_tabs_render_distinct_escaped_recovery_states() {
        for (state, marker) in [
            (
                ViewerTabState::Broken {
                    code: "missing<&>".into(),
                    reason: "artifact <gone>".into(),
                },
                "viewer-status-broken",
            ),
            (
                ViewerTabState::Error {
                    reason: "render <failed>".into(),
                },
                "viewer-status-error",
            ),
        ] {
            let id = tab_id(3);
            let document = ViewerDocument::new(
                vec![ViewerTab::new(
                    id,
                    "unavailable".into(),
                    ViewerTabKind::Live,
                    state,
                )],
                Some(id),
                None,
                vec![],
                settings(),
            )
            .expect("non-ready active tabs intentionally have no view");

            let html = MaudViewerRenderer.build_document(&document);

            assert!(html.contains(marker));
            assert!(html.contains("&lt;"));
            assert!(!html.contains("class=\"layout"));
            assert!(html.contains("/tabs/3/refresh"));
            assert!(html.contains("hx-delete=\"/tabs/3/live-view\""));
            assert!(html.contains("hx-confirm=\"Delete this saved live view?"));
        }
    }

    #[test]
    fn destructive_live_action_is_accessible_and_never_appears_for_snapshots_or_raw_artifacts() {
        let live = MaudViewerRenderer.build_document(&sample_document());
        assert!(live.contains("class=\"viewer-control-button viewer-danger-button "));
        assert!(live.contains("aria-label=\"Delete saved live view\""));
        assert!(live.contains("hx-delete=\"/tabs/1/live-view\""));
        assert!(live.contains("hx-target=\"#viewer-tabs\""));

        let id = tab_id(1);
        let snapshot = ViewerDocument::new(
            vec![ViewerTab::new(
                id,
                "snapshot".into(),
                ViewerTabKind::Snapshot,
                ViewerTabState::Ready,
            )],
            Some(id),
            Some(ViewerView::new(
                id,
                view(),
                settings().options(),
                ViewerTabKind::Snapshot,
            )),
            vec![],
            settings(),
        )
        .expect("snapshot document is valid");
        let snapshot = MaudViewerRenderer.build_document(&snapshot);
        assert!(!snapshot.contains("Delete live view"));
        assert!(!snapshot.contains("/live-view"));

        let raw = preview::build_html(&view(), RenderOptions::DEFAULT, None);
        assert!(!raw.contains("Delete live view"));
        assert!(!raw.contains("/live-view"));
    }

    #[test]
    fn empty_document_gives_a_direct_next_step() {
        let document = ViewerDocument::new(vec![], None, None, vec![], settings())
            .expect("empty viewer is valid");

        let html = MaudViewerRenderer.build_document(&document);

        assert!(html.contains("viewer-status-empty"));
        assert!(html.contains("gtl diff"));
        assert!(!html.contains("class=\"layout"));
    }

    fn sample_history_entry() -> ViewerHistoryEntry {
        ViewerHistoryEntry::new(
            history_id(9),
            "<recent>".into(),
            "repo & tools".into(),
            "main..HEAD".into(),
            "2026-07-11T00:00:00Z".into(),
            sample_recipe(),
        )
    }

    #[test]
    fn history_row_shows_stable_id_and_escapes_metadata() {
        let html = MaudViewerRenderer.build_history(&[sample_history_entry()]);
        let row = html
            .split_once("viewer-history-row")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("history entry renders a row");

        assert!(html.contains("#9"));
        assert!(html.contains("/history/9/open"));
        assert!(html.contains("&lt;recent&gt;"));
        assert!(html.contains("repo &amp; tools"));
        assert!(row.contains("role=\"button\""));
        assert!(row.contains("hx-get"));
    }

    #[test]
    fn history_row_offers_a_json_copy_action() {
        let html = MaudViewerRenderer.build_history(&[sample_history_entry()]);

        assert!(html.contains("data-history-copy="));
        assert!(html.contains("aria-label=\"Copy render JSON\""));
    }

    #[test]
    fn theme_setting_updates_the_document_palette_without_a_reload() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        assert!(html.contains("data-viewer-theme=\"dark\""));
        assert!(html.contains("/settings?theme=dark"));
    }

    /// Splits the theme panel out of a rendered document into its opening tag
    /// and the option rows it holds.
    fn theme_panel(html: &str) -> (&str, &str) {
        let (_, tail) = html
            .split_once("<div id=\"viewer-theme-popover\"")
            .expect("document renders the theme panel");
        let (tag, tail) = tail.split_once('>').expect("the panel tag closes");
        let (options, _) = tail
            .split_once("</div>")
            .expect("the theme panel renders its options");
        (tag, options)
    }

    #[test]
    fn theme_picker_opens_from_the_tab_strip_into_the_top_layer() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let (panel, options) = theme_panel(&html);
        let active = options
            .split_once("data-viewer-theme=\"hearth\"")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("the active palette renders an option");

        assert!(html.contains("popovertarget=\"viewer-theme-popover\""));
        assert!(panel.contains(" popover"), "{panel}");
        assert!(html.contains("id=\"viewer-theme-name\">Hearth</span>"));
        assert_eq!(
            html.matches("data-viewer-theme=").count(),
            Theme::VARIANTS.len()
        );
        assert!(active.contains("checked"), "{active}");
        assert!(active.contains("autofocus"), "{active}");
        assert_eq!(options.matches("autofocus").count(), 1);
    }

    #[test]
    fn every_theme_option_carries_its_own_palette_on_its_swatch() {
        // ! `data-theme` on the swatch is what makes each row paint its own
        // ! accent while another palette is active on the document root; the
        // ! unqualified token blocks it relies on are pinned in
        // ! `preview::assets::tests::theme_token_blocks_are_scoped_to_any_subtree`.
        let html = MaudViewerRenderer.build_document(&sample_document());
        let (_, options) = theme_panel(&html);

        for theme in Theme::VARIANTS {
            assert!(
                options.contains(&format!("data-theme=\"{theme}\"")),
                "{options}"
            );
        }
        assert_eq!(
            options.matches("data-theme=\"").count(),
            Theme::VARIANTS.len()
        );
    }

    #[test]
    fn theme_picker_stays_out_of_the_swapped_view_fragment() {
        let view = MaudViewerRenderer.build_view(&sample_document());

        assert!(!view.contains("data-viewer-theme="));
    }

    #[test]
    fn layout_and_density_swaps_follow_native_radio_changes() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        assert_eq!(
            html.matches("hx-trigger=\"change from:find input\"")
                .count(),
            8
        );
    }

    #[test]
    fn settings_controls_exclude_native_radio_parameters() {
        let html = MaudViewerRenderer.build_document(&sample_document());
        let settings_inputs: Vec<&str> = html
            .split("<input ")
            .skip(1)
            .map(|tail| tail.split_once('>').expect("input has a closing angle").0)
            .filter(|input| input.contains("hx-get=\"/settings?"))
            .collect();

        assert_eq!(settings_inputs.len(), 8 + Theme::VARIANTS.len());
        for input in settings_inputs {
            assert!(input.contains("hx-params=\"none\""), "{input}");
            if input.contains("layout=") || input.contains("density=") {
                assert!(input.contains("hx-sync=\"this:drop\""), "{input}");
            }
        }
    }

    #[test]
    fn view_primary_response_updates_tabs_out_of_band() {
        let html = MaudViewerRenderer.build_view_with_tabs(&sample_document());

        assert_eq!(html.matches("id=\"viewer-view\"").count(), 1);
        assert_eq!(html.matches("id=\"viewer-tabs\"").count(), 1);
        assert!(html.starts_with("<section id=\"viewer-view\""));
        assert!(html.contains("<nav id=\"viewer-tabs\" hx-swap-oob=\"outerHTML\""));
        assert!(!html.starts_with("<section id=\"viewer-view\" hx-swap-oob"));
    }

    #[test]
    fn tabs_primary_response_updates_view_out_of_band() {
        let html = MaudViewerRenderer.build_tabs_with_view(&sample_document(), SwapFeedback::None);

        assert_eq!(html.matches("id=\"viewer-tabs\"").count(), 1);
        assert_eq!(html.matches("id=\"viewer-view\"").count(), 1);
        assert!(html.starts_with("<nav id=\"viewer-tabs\""));
        assert!(html.contains("<section id=\"viewer-view\" hx-swap-oob=\"outerHTML\""));
        assert!(!html.starts_with("<nav id=\"viewer-tabs\" hx-swap-oob"));
    }

    #[test]
    fn snapshot_skips_render_one_accessible_escaped_toast() {
        let labels = vec!["api".into(), "<script>web</script>".into()];

        let html = MaudViewerRenderer.build_tabs_with_view(
            &sample_document(),
            SwapFeedback::SnapshotRecipesSkipped(&labels),
        );

        assert!(html.contains("class=\"gtl-toast viewer-toast-skip pointer-events-none "));
        assert!(html.contains("data-viewer-toast"));
        assert!(html.contains("role=\"status\" aria-live=\"polite\" aria-atomic=\"true\""));
        assert!(html.contains(
            "Skipped 2 diffs with no commits or changed files: api, &lt;script&gt;web&lt;/script&gt;."
        ));
        assert!(!html.contains("<script>web</script>"));
    }

    #[test]
    fn shared_stylesheet_emits_viewer_theme_scale_utilities() {
        let css = preview::preview_css();

        assert!(css.contains(".min-w-0{min-width:0}"));
        assert!(css.contains(".bg-surface{background-color:var(--surface)}"));
        assert!(css.contains(".text-ink-2{color:var(--ink-2)}"));
    }

    #[test]
    fn viewer_narrow_layout_includes_the_760px_boundary() {
        let css = preview::preview_css();

        assert!(css.contains("@media (max-width:760px)"));
        assert!(!css.contains("@media not all and (min-width:760px)"));
    }

    #[test]
    fn live_delete_feedback_focuses_the_active_tab_and_announces_success() {
        let html = MaudViewerRenderer
            .build_tabs_with_view(&sample_document(), SwapFeedback::LiveViewDeleted);
        let active = html
            .split_once("class=\"viewer-tab-activate ")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("active tab button renders");

        assert!(active.contains("autofocus"), "{active}");
        assert!(html.contains("role=\"status\" aria-live=\"polite\" aria-atomic=\"true\""));
        assert!(html.contains("Live view deleted. Focus moved to git-tools changes."));
        assert_eq!(html.matches("autofocus").count(), 1);
    }

    #[test]
    fn live_delete_feedback_focuses_history_when_no_tabs_remain() {
        let empty = ViewerDocument::new(vec![], None, None, vec![], settings())
            .expect("empty viewer is valid");
        let html = MaudViewerRenderer.build_tabs_with_view(&empty, SwapFeedback::LiveViewDeleted);
        let history = html
            .split_once("class=\"viewer-recovery-button ")
            .and_then(|(_, tail)| tail.split_once('>'))
            .map(|(tag, _)| tag)
            .expect("empty-state History action renders");

        assert!(history.contains("autofocus"), "{history}");
        assert!(html.contains("Live view deleted. No diffs remain open."));
        assert!(html.contains("Open History or run gtl diff live to add one."));
        assert_eq!(html.matches("autofocus").count(), 1);
    }

    #[test]
    fn narrow_split_rows_stack_in_the_shared_preview_styles() {
        let shared = preview::preview_css();

        assert!(shared.contains("@media (max-width:1024px)"));
        assert!(!shared.contains("@media not all and (min-width:1024px)"));
        assert!(shared.contains("grid-template-columns:44px minmax(0,1fr)"));
        assert!(shared.contains("grid-template-columns:30px minmax(0,1fr)"));
        assert!(!shared.contains("data-diff-full"));
        assert!(!shared.contains("body.viewer-shell"));
    }

    #[test]
    fn open_diff_navigation_uses_buttons_without_partial_tab_aria() {
        let document = sample_document();
        let html =
            MaudViewerRenderer.build_tabs(document.tabs(), Some(tab_id(1)), settings().theme());

        assert!(html.contains("<nav id=\"viewer-tabs\""));
        assert!(html.contains("<ul class=\"viewer-tab-list "));
        assert!(html.contains("aria-current=\"page\""));
        assert!(!html.contains("role=\"tablist\""));
        assert!(!html.contains("role=\"tab\""));
        assert!(!html.contains("aria-selected="));
    }

    #[test]
    fn dependent_actions_target_their_primary_compound_root() {
        fn opening_tag_with<'html>(html: &'html str, needle: &str) -> &'html str {
            let position = html.find(needle).expect("route appears in rendered markup");
            let start = html[..position].rfind('<').expect("route belongs to a tag");
            let end = html[position..].find('>').expect("tag closes") + position + 1;
            &html[start..end]
        }

        let document = sample_document();
        let html = MaudViewerRenderer.build_document(&document);
        let activate = opening_tag_with(&html, "/tabs/1/activate");
        let close = opening_tag_with(&html, "/tabs/1/close");
        let open_history = opening_tag_with(&html, "/history/7/open");

        assert!(activate.contains("hx-target=\"#viewer-view\""));
        assert!(close.contains("hx-target=\"#viewer-view\""), "{close}");
        assert!(
            open_history.contains("hx-target=\"#viewer-tabs\""),
            "{open_history}"
        );

        let inactive = ViewerTab::new(
            tab_id(2),
            "inactive".into(),
            ViewerTabKind::Snapshot,
            ViewerTabState::Ready,
        );
        let tabs = MaudViewerRenderer.build_tabs(
            &[document.tabs()[0].clone(), inactive],
            Some(tab_id(1)),
            settings().theme(),
        );
        let inactive_close = opening_tag_with(&tabs, "/tabs/2/close");
        assert!(
            inactive_close.contains("hx-target=\"#viewer-tabs\""),
            "{inactive_close}"
        );
    }

    #[test]
    fn recipe_wake_drain_avoids_htmx_polling() {
        let html = MaudViewerRenderer.build_document(&sample_document());

        assert!(!html.contains("hx-trigger=\"load\""));
        assert!(!html.contains("<iframe"));
    }

    #[test]
    fn vendored_htmx_digest_is_pinned() {
        use std::fmt::Write as _;

        use sha2::{Digest as _, Sha256};

        let digest = Sha256::digest(crate::protocol_config::HTMX.as_bytes());
        let mut encoded = String::with_capacity(digest.len() * 2);
        for byte in digest {
            write!(encoded, "{byte:02x}").expect("writing to a string cannot fail");
        }

        assert_eq!(encoded, HTMX_SHA256);
    }
}
