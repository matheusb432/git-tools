use dioxus::{core::spawn_forever, prelude::*};
use gtl_wire::viewer::{ResetSettings, ViewerSettingsRecovery};

use crate::{
    app::application_layout::ViewerContext,
    entities::diffs::viewer_server,
    shared::{
        ui::{Button, ButtonVariant, PageNotice, ScrollArea, ToastHandle, use_toast},
        viewer_client::ViewerClientError,
    },
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
            toast.client_error(&error);
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

    rsx! {
        SettingsRecoveryNotice {
            recovery: recovery.cloned(),
            pending: pending(),
            onreset: reset,
            onretry: move |()| {
                recovery.restart();
                onretry.call(());
            },
        }
    }
}

/// `recovery` is `None` while the recovery details load.
#[component]
fn SettingsRecoveryNotice(
    #[props(!optional)] recovery: Option<Result<ViewerSettingsRecovery, ViewerClientError>>,
    pending: bool,
    onreset: EventHandler<String>,
    onretry: EventHandler<()>,
) -> Element {
    rsx! {
        ScrollArea { class: "h-full",
            PageNotice {
                class: "min-h-full px-4 py-6",
                role: "alert",
                aria_label: "Settings recovery",
                title: "User settings are invalid",
                message: "Repair the file and retry, or restore defaults. Reset saves the original file as config.yyyymmdd-hhmmss-backup.toml before replacing it. Backup timestamps use UTC.",
                match &recovery {
                    None => rsx! {
                        p { role: "status", "Loading settings details..." }
                    },
                    Some(Err(error)) => rsx! {
                        p { "{error}" }
                    },
                    Some(Ok(recovery)) => rsx! {
                        p { class: "settings-recovery-path mt-4 font-mono", "{recovery.configuration_path}" }
                        if let Some(diagnostic) = &recovery.diagnostic {
                            pre { class: "settings-recovery-diagnostic mt-3", "{diagnostic}" }
                            Button {
                                class: "mx-auto mt-4",
                                variant: ButtonVariant::Outline,
                                disabled: pending,
                                onclick: {
                                    let revision = recovery.revision.clone();
                                    move |_| onreset.call(revision.clone())
                                },
                                if pending {
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
                    disabled: pending,
                    onclick: move |_| onretry.call(()),
                    "Retry"
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_settings_show_the_diagnostic_path_and_recovery_actions() {
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || SettingsRecoveryNoticeProps {
            recovery: Some(Ok(ViewerSettingsRecovery {
                configuration_path: "/home/user/.config/git-tools/config.toml".to_owned(),
                diagnostic: Some("TOML parse error at line 3, column 9".to_owned()),
                revision: "revision-1".to_owned(),
            })),
            pending: false,
            onreset: EventHandler::new(|_| {}),
            onretry: EventHandler::new(|()| {}),
        });
        let mut notice = VirtualDom::new_with_props(SettingsRecoveryNotice, props);
        notice.rebuild_in_place();
        let html = dioxus_ssr::render(&notice);

        assert!(html.contains("User settings are invalid"));
        assert!(html.contains("/home/user/.config/git-tools/config.toml"));
        assert!(html.contains("TOML parse error at line 3, column 9"));
        assert!(html.contains(">Back up and reset settings</button>"));
        assert!(html.contains(">Retry</button>"));
    }
}
