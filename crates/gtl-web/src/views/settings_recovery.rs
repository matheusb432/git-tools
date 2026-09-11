use dioxus::{core::spawn_forever, prelude::*};
use gtl_wire::viewer::ResetSettings;

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::ui::{Button, ButtonVariant, PageNotice, ScrollArea, ToastHandle, use_toast},
};

fn use_settings_reset(onretry: EventHandler<()>) -> (ReadSignal<bool>, Callback<String>) {
    let mut pending = use_signal(|| false);
    let toast = use_toast();
    let viewer = use_context::<ViewerContext>();
    let reset = use_callback(move |revision: String| {
        if pending() {
            return;
        }
        pending.set(true);
        spawn_forever(reset_settings(
            ResetSettings { revision },
            pending,
            onretry,
            viewer,
            toast,
        ));
    });
    (pending.into(), reset)
}

async fn reset_settings(
    request: ResetSettings,
    mut pending: Signal<bool>,
    onretry: EventHandler<()>,
    viewer: ViewerContext,
    toast: ToastHandle,
) {
    let result = viewer_server::reset_settings(request).await;
    let mounted = if let Ok(mut pending) = pending.try_write() {
        *pending = false;
        true
    } else {
        false
    };
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            toast.error(error.message());
            return;
        }
    };
    toast.ok(format!("Settings reset. Backup: {}", result.backup_path));
    if mounted {
        onretry.call(());
    } else {
        viewer.refresh(false);
    }
}

#[component]
pub(crate) fn SettingsRecovery(onretry: EventHandler<()>) -> Element {
    let mut recovery = use_resource(viewer_server::get_settings_recovery);
    let (pending, reset) = use_settings_reset(onretry);
    let load = recovery.read();

    rsx! {
        ScrollArea { class: "h-full",
            PageNotice {
                class: "min-h-full px-4 py-6",
                role: "alert",
                aria_label: "Settings recovery",
                title: "User settings are invalid",
                message: "Repair the file and retry, or restore defaults. Reset saves the original file as config.yyyymmdd-hhmmss-backup.toml before replacing it. Backup timestamps use UTC.",
                match &*load {
                    None => rsx! {
                        p { role: "status", "Loading settings details..." }
                    },
                    Some(Err(error)) => rsx! {
                        p { "{error.message()}" }
                    },
                    Some(Ok(recovery)) => rsx! {
                        p { class: "settings-recovery-path mt-4 font-mono", "{recovery.configuration_path}" }
                        if let Some(diagnostic) = &recovery.diagnostic {
                            pre { class: "settings-recovery-diagnostic mt-3", "{diagnostic}" }
                            Button {
                                class: "mx-auto mt-4",
                                variant: ButtonVariant::Outline,
                                disabled: pending(),
                                onclick: {
                                    let revision = recovery.revision.clone();
                                    move |_| reset.call(revision.clone())
                                },
                                if pending() {
                                    "Backing up and resetting..."
                                } else {
                                    "Back up and reset settings"
                                }
                            }
                        } else {
                            p { class: "mt-3 text-ink-2", "The settings file is valid now. Retry to continue." }
                        }
                    },
                }
                Button {
                    class: "mx-auto mt-4",
                    variant: ButtonVariant::Outline,
                    disabled: pending(),
                    onclick: move |_| {
                        recovery.restart();
                        onretry.call(());
                    },
                    "Retry"
                }
            }
        }
    }
}
