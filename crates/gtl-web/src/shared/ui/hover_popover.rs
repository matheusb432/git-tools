use dioxus::{dioxus_core::Task, prelude::*};

use super::{ScrollArea, scroll_area::ScrollAreaVariant};
use crate::shared::browser;

#[derive(Default)]
struct HoverState {
    pointer_inside: bool,
    focus_inside: bool,
    reveal_task: Option<Task>,
}

#[derive(Clone, Copy)]
pub(crate) struct HoverInteraction {
    pub active: Memo<bool>,
    pub pointer_enter: Callback<()>,
    pub pointer_leave: Callback<()>,
    pub focus_enter: Callback<()>,
    pub focus_leave: Callback<()>,
}

pub(crate) fn use_hover_popover(id: String) -> HoverInteraction {
    let state = use_signal(HoverState::default);
    let active = use_memo(move || {
        let state = state.read();
        state.pointer_inside || state.focus_inside
    });
    let pointer_id = id.clone();
    let pointer_leave_id = id.clone();
    let focus_id = id.clone();
    HoverInteraction {
        active,
        pointer_enter: use_callback(move |()| begin(state, true, pointer_id.clone())),
        pointer_leave: use_callback(move |()| end(state, true, &pointer_leave_id)),
        focus_enter: use_callback(move |()| begin(state, false, focus_id.clone())),
        focus_leave: use_callback(move |()| end(state, false, &id)),
    }
}

fn begin(mut state: Signal<HoverState>, pointer: bool, id: String) {
    {
        let mut state = state.write();
        let active = state.pointer_inside || state.focus_inside;
        if pointer {
            state.pointer_inside = true;
        } else {
            state.focus_inside = true;
        }
        if active {
            return;
        }
        if let Some(task) = state.reveal_task.take() {
            task.cancel();
        }
    }
    let task = spawn(async move {
        dioxus_sdk_time::sleep(std::time::Duration::from_millis(350)).await;
        browser::show_hover_popover(&id);
    });
    state.write().reveal_task = Some(task);
}

fn end(mut state: Signal<HoverState>, pointer: bool, id: &str) {
    let mut state = state.write();
    if pointer {
        state.pointer_inside = false;
    } else {
        state.focus_inside = false;
    }
    if state.pointer_inside || state.focus_inside {
        return;
    }
    if let Some(task) = state.reveal_task.take() {
        task.cancel();
    }
    browser::hide_popover(id);
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum HoverPopoverPlacement {
    #[default]
    Left,
    #[cfg(feature = "desktop")]
    Below,
}

#[component]
pub(crate) fn HoverPopover(
    id: String,
    anchor_name: String,
    aria_label: String,
    #[props(default)] placement: HoverPopoverPlacement,
    children: Element,
) -> Element {
    let placement_class = match placement {
        HoverPopoverPlacement::Left => {
            "-translate-x-2 [position-area:left_span-bottom] [position-try-fallbacks:flip-inline]"
        }
        #[cfg(feature = "desktop")]
        HoverPopoverPlacement::Below => {
            "translate-y-1 [position-area:bottom_span-left] [position-try-fallbacks:flip-block]"
        }
    };
    rsx! {
        span {
            id,
            class: "hover-popover-shell m-0 p-0 {placement_class}",
            style: "position-anchor: {anchor_name};",
            popover: "auto",
            role: "tooltip",
            aria_label,
            ScrollArea {
                variant: ScrollAreaVariant::Vertical,
                class: "hover-popover-content p-3",
                {children}
            }
        }
    }
}
