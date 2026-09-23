use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

pub(crate) fn keydown(id: &str, trigger_id: &str, event: &KeyboardEvent) {
    let Some(document) = web_sys::window().and_then(|window| window.document()) else {
        return;
    };
    let Some(menu) = document
        .get_element_by_id(id)
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let Some(trigger) = document
        .get_element_by_id(trigger_id)
        .and_then(|element| element.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    let Ok(nodes) = menu.query_selector_all("[role='menuitem']:not(:disabled)") else {
        return;
    };
    let items = (0..nodes.length())
        .filter_map(|index| nodes.item(index)?.dyn_into::<HtmlElement>().ok())
        .collect::<Vec<_>>();
    if items.is_empty() {
        return;
    }
    let active = document.active_element();
    let from_trigger = active.as_ref() == Some(trigger.as_ref());
    let index = items
        .iter()
        .position(|item| active.as_ref() == Some(item.as_ref()));
    let key = event.key();
    let target = match key {
        Key::ArrowDown => Some(index.map_or(0, |index| (index + 1) % items.len())),
        Key::ArrowUp => Some(
            index
                .filter(|index| *index > 0)
                .map_or(items.len().saturating_sub(1), |index| index - 1),
        ),
        Key::Home if !from_trigger => Some(0),
        Key::End if !from_trigger => Some(items.len().saturating_sub(1)),
        Key::Escape | Key::Tab if !from_trigger => {
            let _ = menu.hide_popover();
            let _ = trigger.focus();
            if key == Key::Escape {
                event.prevent_default();
                event.stop_propagation();
            }
            return;
        }
        Key::Character(ref text)
            if !from_trigger
                && text.chars().count() == 1
                && text != " "
                && !event
                    .modifiers()
                    .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) =>
        {
            (1..=items.len())
                .map(|offset| (index.unwrap_or(0) + offset) % items.len())
                .find(|index| {
                    items[*index]
                        .text_content()
                        .unwrap_or_default()
                        .trim()
                        .to_lowercase()
                        .starts_with(&text.to_lowercase())
                })
        }
        _ => None,
    };
    if let Some(item) = target.and_then(|index| items.get(index)) {
        event.prevent_default();
        event.stop_propagation();
        if from_trigger && !menu.matches(":popover-open").unwrap_or(false) {
            trigger.click();
        }
        let _ = item.focus();
    }
}
