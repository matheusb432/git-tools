use dioxus::prelude::*;
use futures_util::StreamExt as _;
use gtl_wire::viewer::ViewerUserSettings;

use crate::{
    app::{
        application_layout::{ViewerContext, ViewerShellLoad},
        application_router::Route,
    },
    entities::diffs::viewer_server,
    shared::{ui::use_toast, viewer_client::ViewerClientError},
    views::viewer_settings_form::{SettingsEdit, ViewerSettingsSelection, viewer_settings_patch},
};

#[derive(Clone, PartialEq)]
enum SettingsStatus {
    Loading,
    Ready,
    Refreshing,
    Saving,
    Failed(ViewerClientError),
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct UserSettings {
    pub(crate) selection: ReadSignal<Option<ViewerSettingsSelection>>,
    pub(crate) path: ReadSignal<Option<String>>,
    pub(crate) error: Memo<Option<ViewerClientError>>,
    pub(crate) select: Callback<SettingsEdit>,
    pub(crate) retry: Callback<()>,
    pub(crate) reload: Callback<()>,
}

enum SettingsCommand {
    Load,
    Save,
}

#[derive(Clone, Copy)]
struct SettingsSelection(Signal<Option<ViewerSettingsSelection>>);

pub(crate) fn use_settings_selection_provider() -> ReadSignal<Option<ViewerSettingsSelection>> {
    let selection = use_signal(|| None);
    use_context_provider(|| SettingsSelection(selection));
    selection.into()
}

/// Shares the displayed settings and keeps writes alive across route changes.
pub(crate) fn use_user_settings_provider() -> UserSettings {
    let viewer = use_context::<ViewerContext>();
    let toast = use_toast();
    let router = dioxus::router::router();
    let mut selection = use_context::<SettingsSelection>().0;
    let path = use_signal(|| None);
    let mut status = use_signal(|| SettingsStatus::Loading);
    let failed = use_callback(move |error: ViewerClientError| {
        if !matches!(router.current::<Route>(), Route::Settings { .. })
            && selection.peek().is_some()
        {
            toast.client_error(&error);
        }
        status.set(SettingsStatus::Failed(error));
    });
    let worker = use_coroutine(move |commands| {
        run_settings_commands(commands, viewer, selection, path, status, failed)
    });
    let select = use_callback(move |edit: SettingsEdit| {
        let Some(mut selected) = *selection.peek() else {
            return;
        };
        edit.apply(&mut selected);
        if *selection.peek() == Some(selected) {
            return;
        }
        selection.set(Some(selected));
        if matches!(
            *status.peek(),
            SettingsStatus::Saving | SettingsStatus::Failed(_)
        ) {
            return;
        }
        status.set(SettingsStatus::Saving);
        worker.send(SettingsCommand::Save);
    });
    let retry = use_callback(move |()| {
        if !matches!(*status.peek(), SettingsStatus::Failed(_)) {
            return;
        }
        status.set(SettingsStatus::Saving);
        worker.send(SettingsCommand::Save);
    });
    let reload = use_callback(move |()| {
        if matches!(
            *status.peek(),
            SettingsStatus::Saving | SettingsStatus::Refreshing
        ) {
            return;
        }
        status.set(SettingsStatus::Refreshing);
        worker.send(SettingsCommand::Load);
    });
    let preferences = use_memo(move || match &*viewer.shell().read() {
        ViewerShellLoad::Ready(shell) => Some(shell.preferences),
        _ => None,
    });
    let mut previous_instance = use_signal(|| None::<String>);
    use_effect(move || {
        let instance = viewer.server_instance_id();
        let replaced = *previous_instance.peek() != instance;
        if preferences().is_none() || !viewer.actions_enabled() {
            return;
        }
        previous_instance.set(instance);
        if !replaced
            && matches!(
                *status.peek(),
                SettingsStatus::Saving | SettingsStatus::Refreshing | SettingsStatus::Failed(_)
            )
        {
            return;
        }
        status.set(SettingsStatus::Refreshing);
        worker.send(SettingsCommand::Load);
    });
    let error = use_memo(move || match status() {
        SettingsStatus::Failed(error) => Some(error),
        _ => None,
    });
    use_context_provider(|| UserSettings {
        selection: selection.into(),
        path: path.into(),
        error,
        select,
        retry,
        reload,
    })
}

async fn run_settings_commands(
    mut commands: UnboundedReceiver<SettingsCommand>,
    viewer: ViewerContext,
    selection: Signal<Option<ViewerSettingsSelection>>,
    path: Signal<Option<String>>,
    status: Signal<SettingsStatus>,
    failed: Callback<ViewerClientError>,
) {
    // Supplies the next write's revision and field baseline.
    let mut acknowledged = None::<ViewerUserSettings>;
    while let Some(command) = commands.next().await {
        let result = match command {
            SettingsCommand::Load => {
                load_settings(viewer, selection, path, status, &mut acknowledged).await
            }
            SettingsCommand::Save => {
                save_settings(viewer, selection, path, status, &mut acknowledged).await
            }
        };
        if let Err(error) = result {
            failed.call(error);
        }
    }
}

async fn load_settings(
    viewer: ViewerContext,
    mut selection: Signal<Option<ViewerSettingsSelection>>,
    mut path: Signal<Option<String>>,
    mut status: Signal<SettingsStatus>,
    acknowledged: &mut Option<ViewerUserSettings>,
) -> Result<(), ViewerClientError> {
    let instance = viewer.server_instance_id();
    let before = *selection.peek();
    let result = viewer_server::get_settings().await;
    if viewer.server_instance_id() != instance {
        return Ok(());
    }
    let settings = result?;
    let loaded = ViewerSettingsSelection::from(&settings);
    let selected = *selection.peek();
    selection.set(match (before, selected) {
        (Some(before), Some(selected)) => Some(merge_settings(before, selected, loaded)),
        _ => Some(loaded),
    });
    path.set(settings.configuration_path.clone());
    *acknowledged = Some(settings);
    if !matches!(*status.peek(), SettingsStatus::Saving) {
        status.set(SettingsStatus::Ready);
    }
    Ok(())
}

async fn save_settings(
    viewer: ViewerContext,
    mut selection: Signal<Option<ViewerSettingsSelection>>,
    mut path: Signal<Option<String>>,
    mut status: Signal<SettingsStatus>,
    acknowledged: &mut Option<ViewerUserSettings>,
) -> Result<(), ViewerClientError> {
    while matches!(*status.peek(), SettingsStatus::Saving) {
        let (Some(current), Some(submitted)) = (acknowledged.as_ref(), *selection.peek()) else {
            break;
        };
        let request = viewer_settings_patch(
            ViewerSettingsSelection::from(current),
            submitted,
            current.revision,
        );
        let instance = viewer.server_instance_id();
        let result = persist_settings(request).await;
        if viewer.server_instance_id() != instance {
            break;
        }
        let (settings, shell) = result?;
        let loaded = ViewerSettingsSelection::from(&settings);
        let selected = selection.peek().unwrap_or(submitted);
        let selected = merge_settings(submitted, selected, loaded);
        selection.set(Some(selected));
        path.set(settings.configuration_path.clone());
        *acknowledged = Some(settings);
        viewer.replace_shell(shell);
        if selected == loaded {
            status.set(SettingsStatus::Ready);
        }
    }
    Ok(())
}

async fn persist_settings(
    request: gtl_wire::viewer::EditSettingsRequest,
) -> Result<(ViewerUserSettings, gtl_wire::viewer::ViewerShell), ViewerClientError> {
    viewer_server::edit_settings(request).await?;
    let settings = viewer_server::get_settings().await?;
    let shell = viewer_server::get_shell().await?;
    Ok((settings, shell))
}

fn merge_settings(
    submitted: ViewerSettingsSelection,
    mut selected: ViewerSettingsSelection,
    loaded: ViewerSettingsSelection,
) -> ViewerSettingsSelection {
    // Only untouched fields accept the response; later input stays visible.
    merge(&mut selected.language, submitted.language, loaded.language);
    merge(
        &mut selected.date_format,
        submitted.date_format,
        loaded.date_format,
    );
    merge(&mut selected.theme, submitted.theme, loaded.theme);
    merge(
        &mut selected.focus_window_on_diff,
        submitted.focus_window_on_diff,
        loaded.focus_window_on_diff,
    );
    merge(
        &mut selected.push_confirmation_required,
        submitted.push_confirmation_required,
        loaded.push_confirmation_required,
    );
    merge(
        &mut selected.accessibility.ui_scale_percent,
        submitted.accessibility.ui_scale_percent,
        loaded.accessibility.ui_scale_percent,
    );
    merge(
        &mut selected.accessibility.reduce_motion,
        submitted.accessibility.reduce_motion,
        loaded.accessibility.reduce_motion,
    );
    merge(
        &mut selected.render_options.layout,
        submitted.render_options.layout,
        loaded.render_options.layout,
    );
    merge(
        &mut selected.render_options.density,
        submitted.render_options.density,
        loaded.render_options.density,
    );
    merge(
        &mut selected.render_options.wrap_lines,
        submitted.render_options.wrap_lines,
        loaded.render_options.wrap_lines,
    );
    merge(
        &mut selected.copy_with_line_context,
        submitted.copy_with_line_context,
        loaded.copy_with_line_context,
    );
    selected
}

fn merge<T: Copy + PartialEq>(selected: &mut T, submitted: T, loaded: T) {
    if *selected == submitted {
        *selected = loaded;
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::settings::{ViewerAccessibility, ViewerDateFormat, ViewerLanguage};
    use gtl_wire::viewer::{ViewerDiffDensity, ViewerDiffLayout, ViewerRenderOptions, ViewerTheme};

    use super::*;

    fn selection() -> ViewerSettingsSelection {
        ViewerSettingsSelection {
            language: ViewerLanguage::EnUs,
            date_format: ViewerDateFormat::Iso,
            theme: None,
            render_options: ViewerRenderOptions {
                layout: ViewerDiffLayout::Unified,
                density: ViewerDiffDensity::Compact,
                wrap_lines: false,
            },
            focus_window_on_diff: true,
            copy_with_line_context: true,
            push_confirmation_required: true,
            accessibility: ViewerAccessibility::default(),
        }
    }

    #[test]
    fn an_acknowledgement_preserves_later_input_and_accepts_untouched_fields() {
        let submitted = selection();
        let selected = ViewerSettingsSelection {
            theme: Some(ViewerTheme::Carbon),
            language: ViewerLanguage::PtBr,
            ..submitted
        };
        let loaded = ViewerSettingsSelection {
            focus_window_on_diff: false,
            ..submitted
        };
        assert_eq!(
            merge_settings(submitted, selected, loaded),
            ViewerSettingsSelection {
                focus_window_on_diff: false,
                ..selected
            },
        );
    }

    #[test]
    fn reverting_an_in_flight_choice_remains_visible() {
        let selected = selection();
        let submitted = ViewerSettingsSelection {
            theme: Some(ViewerTheme::Carbon),
            ..selected
        };
        assert_eq!(merge_settings(submitted, selected, submitted), selected);
    }
}
