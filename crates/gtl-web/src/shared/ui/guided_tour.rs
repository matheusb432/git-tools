mod browser;
mod layout;
mod seen_guided_tours;

use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::settings::ViewerLanguage;
use lucide_dioxus::{CircleQuestionMark, X};

use super::{
    Button, ButtonSize, ButtonVariant,
    dialog::{DialogPlacement, use_dialog, use_dialog_slot},
};
use crate::shared::i18n::{t, use_language};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GuidedTourAnchor(&'static str);

impl GuidedTourAnchor {
    pub(crate) const fn new(value: &'static str) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> &'static str {
        self.0
    }
}

#[derive(Clone, PartialEq)]
pub(crate) struct GuidedTourStep {
    anchor: GuidedTourAnchor,
    title: String,
    body: String,
}

impl GuidedTourStep {
    pub(crate) const fn new(anchor: GuidedTourAnchor, title: String, body: String) -> Self {
        Self {
            anchor,
            title,
            body,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GuidedTour {
    id: &'static str,
    localize: fn(ViewerLanguage) -> Vec<GuidedTourStep>,
}

impl GuidedTour {
    pub(crate) const fn new(
        id: &'static str,
        localize: fn(ViewerLanguage) -> Vec<GuidedTourStep>,
    ) -> Self {
        Self { id, localize }
    }
}

impl PartialEq for GuidedTour {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[component]
pub(crate) fn GuidedTourButton(tour: GuidedTour) -> Element {
    let language = use_language();
    let scope = use_hook(dioxus::dioxus_core::current_scope_id);
    let trigger_id = format!("guided-tour-{}-{}", tour.id, scope.0);
    let mut unseen = use_signal(move || !seen_guided_tours::read().contains(tour.id));
    let active = use_dialog_slot::<Vec<GuidedTourStep>>();
    let label = t!(language, "tour-start");
    rsx! {
        Button {
            id: trigger_id.clone(),
            class: "guided-tour-trigger",
            size: ButtonSize::IconSmall,
            variant: ButtonVariant::Ghost,
            aria_label: label.clone(),
            title: label,
            aria_haspopup: "dialog",
            "data-tour-launch": tour.id,
            onclick: move |_| {
                unseen.set(false);
                seen_guided_tours::record(tour.id);
                active.open((tour.localize)(language));
            },
            span { class: "guided-tour-icon", aria_hidden: "true",
                CircleQuestionMark { size: 16 }
                if unseen() {
                    span { class: "guided-tour-unseen" }
                }
            }
        }
        if let Some(steps) = active.subject() {
            GuidedTourDialog {
                id: format!("{trigger_id}-dialog"),
                trigger_id,
                steps,
                open: active.is_open(),
                onclose: move |()| active.close(),
                onclosed: move |()| active.release(),
            }
        }
    }
}

#[component]
fn GuidedTourDialog(
    id: String,
    trigger_id: String,
    steps: Vec<GuidedTourStep>,
    open: bool,
    onclose: EventHandler<()>,
    onclosed: EventHandler<()>,
) -> Element {
    let language = use_language();
    let steps = use_hook(move || steps);
    let mut index = use_signal(|| 0_usize);
    let mut geometry = use_signal(layout::GuidedTourLayout::default);
    let anchors = use_hook(|| steps.iter().map(|step| step.anchor).collect::<Vec<_>>());
    let measure = use_callback(move |scroll: bool| {
        geometry.set(browser::measure(
            anchors.get(*index.peek()).copied(),
            scroll,
        ));
    });
    crate::shared::browser::use_window_resize(move || measure.call(false));
    use_dialog(
        &id,
        &trigger_id,
        open,
        Duration::ZERO,
        DialogPlacement::Center,
        Some(onclosed),
    );
    let count = steps.len();
    let select = use_callback(move |next: usize| {
        index.set(next);
        measure.call(true);
    });
    let previous = use_callback(move |()| {
        let current = *index.peek();
        select.call(current.saturating_sub(1));
    });
    let next = use_callback(move |()| {
        let current = *index.peek();
        if current + 1 >= count {
            onclose.call(());
        } else {
            select.call(current + 1);
        }
    });
    let Some(step) = steps.get(index()) else {
        return rsx! {};
    };
    let title_id = format!("{id}-title");
    let body_id = format!("{id}-body");
    let keyboard_id = id.clone();
    let last = index() + 1 == count;
    rsx! {
        dialog {
            id,
            class: "guided-tour-dialog",
            "data-guided-tour": "",
            aria_modal: "true",
            aria_labelledby: title_id.clone(),
            aria_describedby: body_id.clone(),
            onmounted: move |_| measure.call(true),
            oncancel: move |event| {
                event.prevent_default();
                onclose.call(());
            },
            onkeydown: move |event| {
                match event.key() {
                    Key::Tab => {
                        event.prevent_default();
                        event.stop_propagation();
                        browser::move_focus(
                            &keyboard_id,
                            event.modifiers().contains(Modifiers::SHIFT),
                        );
                    }
                    Key::ArrowLeft => {
                        event.prevent_default();
                        event.stop_propagation();
                        previous.call(());
                    }
                    Key::ArrowRight => {
                        event.prevent_default();
                        event.stop_propagation();
                        next.call(());
                    }
                    Key::Escape => {
                        event.prevent_default();
                        event.stop_propagation();
                        onclose.call(());
                    }
                    _ => {}
                }
            },
            div {
                class: "guided-tour-spotlight",
                style: geometry().spotlight_style(),
                aria_hidden: "true",
            }
            section {
                class: "guided-tour-card",
                style: geometry().card_style(),
                aria_live: "polite",
                aria_atomic: "true",
                header { class: "guided-tour-header",
                    span { class: "guided-tour-progress",
                        {t!(language, "tour-progress", current = index() + 1, total = count)}
                    }
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: t!(language, "tour-close"),
                        title: t!(language, "tour-close"),
                        onclick: move |_| onclose.call(()),
                        X { size: 16 }
                    }
                }
                div { class: "guided-tour-copy", tabindex: "0",
                    h2 { id: title_id, class: "guided-tour-title", "{step.title}" }
                    p { id: body_id, class: "guided-tour-body", "{step.body}" }
                }
                footer { class: "guided-tour-actions",
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Ghost,
                        disabled: index() == 0,
                        onclick: move |_| previous.call(()),
                        {t!(language, "tour-previous")}
                    }
                    Button {
                        size: ButtonSize::Small,
                        variant: ButtonVariant::Primary,
                        "data-dialog-content-initial-focus": "true",
                        onclick: move |_| next.call(()),
                        if last {
                            {t!(language, "tour-finish")}
                        } else {
                            {t!(language, "tour-next")}
                        }
                    }
                }
            }
        }
    }
}
