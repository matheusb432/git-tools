use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(in crate::views::diffs) enum Panel {
    Files,
    Commits,
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, Default, PartialEq)]
pub(in crate::views::diffs) struct PanelScrollPosition {
    left: f64,
    top: f64,
}

pub(super) struct PanelScroll {
    pub(super) mount: Callback<MountedEvent>,
    pub(super) restore: Callback<()>,
    pub(super) save: Callback<ScrollEvent>,
}

/// provides saving and restoring panel scroll positions for diff tabs.
#[cfg(feature = "desktop")]
pub(super) fn use_panel_scroll(panel: Panel) -> PanelScroll {
    use crate::views::diffs::presentation::DiffPresentation;

    let presentation = try_use_context::<DiffPresentation>();
    let workspace = super::use_workspace_context();
    let tab = use_memo(move || workspace.view.read().identity.tab_id);
    let mut element = use_signal(|| None::<web_sys::Element>);
    let mut restored_tab = use_signal(|| None);
    let restore = use_callback(move |()| {
        let Some(presentation) = presentation else {
            return;
        };
        let tab = *tab.peek();
        if *restored_tab.peek() == Some(tab) {
            return;
        }
        if restored_tab.peek().is_some() {
            restored_tab.set(None);
        }
        let root = element.peek();
        let Some(root) = root
            .as_ref()
            .filter(|root| root.client_height() > 0 && root.client_width() > 0)
        else {
            return;
        };
        if matches!(panel, Panel::Commits)
            && workspace.view.peek().commit_count > 0
            && workspace.commits.peek().is_empty()
        {
            return;
        }
        let position = presentation.panel_scroll(tab, panel);
        root.scroll_to_with_x_and_y(position.left, position.top);
        restored_tab.set(Some(tab));
    });
    use_effect(move || {
        let _ = tab();
        let _ = element.read();
        let _ = workspace.commits.read().len();
        restore.call(());
    });

    PanelScroll {
        mount: use_callback(move |event: MountedEvent| {
            element.set(event.data().downcast::<web_sys::Element>().cloned());
        }),
        restore,
        save: use_callback(move |event: ScrollEvent| {
            let Some(presentation) = presentation else {
                return;
            };
            let tab = *tab.peek();
            let data = event.data();
            if *restored_tab.peek() == Some(tab)
                && data.client_height() > 0
                && data.client_width() > 0
            {
                presentation.set_panel_scroll(
                    tab,
                    panel,
                    PanelScrollPosition {
                        left: data.scroll_left(),
                        top: data.scroll_top(),
                    },
                );
            }
        }),
    }
}

#[cfg(not(feature = "desktop"))]
pub(super) fn use_panel_scroll(_panel: Panel) -> PanelScroll {
    PanelScroll {
        mount: use_callback(|_| {}),
        restore: use_callback(|()| {}),
        save: use_callback(|_| {}),
    }
}
