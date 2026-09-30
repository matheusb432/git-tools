use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::HtmlElement;

#[derive(Clone, Copy)]
enum MenuKeyboardFocus {
    Trigger,
    Item(usize),
    Elsewhere,
}

struct MenuKeyboardItem {
    label: String,
    disabled: bool,
    selected: bool,
}

#[derive(Debug, PartialEq)]
enum MenuKeyboardAction {
    /// Focuses the item at this index, opening the menu first from its trigger.
    FocusItem(usize),
    /// Closes the menu on Escape and consumes the key.
    Dismiss,
    /// Closes the menu on Tab and lets the key move focus past the trigger.
    Exit,
}

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
    let Ok(nodes) =
        menu.query_selector_all("[role='menuitem'], [role='menuitemcheckbox'], [role='option']")
    else {
        return;
    };
    let elements = (0..nodes.length())
        .filter_map(|index| nodes.item(index)?.dyn_into::<HtmlElement>().ok())
        .collect::<Vec<_>>();
    let items = elements
        .iter()
        .map(|element| MenuKeyboardItem {
            label: element.text_content().unwrap_or_default(),
            disabled: element.matches(":disabled").unwrap_or(false),
            selected: element.get_attribute("aria-selected").as_deref() == Some("true"),
        })
        .collect::<Vec<_>>();
    let active = document.active_element();
    let focus = if active.as_ref() == Some(trigger.as_ref()) {
        MenuKeyboardFocus::Trigger
    } else if let Some(index) = elements
        .iter()
        .position(|element| active.as_ref() == Some(element.as_ref()))
    {
        MenuKeyboardFocus::Item(index)
    } else {
        MenuKeyboardFocus::Elsewhere
    };
    match menu_keyboard_action(&event.key(), event.modifiers(), focus, &items) {
        Some(MenuKeyboardAction::FocusItem(index)) => {
            let Some(item) = elements.get(index) else {
                return;
            };
            event.prevent_default();
            event.stop_propagation();
            if matches!(focus, MenuKeyboardFocus::Trigger)
                && !menu.matches(":popover-open").unwrap_or(false)
            {
                trigger.click();
            }
            let _ = item.focus();
        }
        Some(MenuKeyboardAction::Dismiss) => {
            let _ = menu.hide_popover();
            let _ = trigger.focus();
            event.prevent_default();
            event.stop_propagation();
        }
        Some(MenuKeyboardAction::Exit) => {
            let _ = menu.hide_popover();
            let _ = trigger.focus();
        }
        None => {}
    }
}

fn menu_keyboard_action(
    key: &Key,
    modifiers: Modifiers,
    focus: MenuKeyboardFocus,
    items: &[MenuKeyboardItem],
) -> Option<MenuKeyboardAction> {
    let enabled = items
        .iter()
        .enumerate()
        .filter(|(_, item)| !item.disabled)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if enabled.is_empty() {
        return None;
    }
    let from_trigger = matches!(focus, MenuKeyboardFocus::Trigger);
    let current = match focus {
        MenuKeyboardFocus::Item(index) => enabled.iter().position(|enabled| *enabled == index),
        MenuKeyboardFocus::Trigger | MenuKeyboardFocus::Elsewhere => None,
    };
    let last = enabled.len().saturating_sub(1);
    let selected = enabled.iter().position(|index| items[*index].selected);
    let target = match key {
        Key::ArrowDown | Key::ArrowUp if from_trigger && selected.is_some() => selected,
        Key::ArrowDown => Some(current.map_or(0, |current| (current + 1) % enabled.len())),
        Key::ArrowUp => Some(
            current
                .filter(|current| *current > 0)
                .map_or(last, |current| current - 1),
        ),
        Key::Home if !from_trigger => Some(0),
        Key::End if !from_trigger => Some(last),
        Key::Escape if !from_trigger => return Some(MenuKeyboardAction::Dismiss),
        Key::Tab if !from_trigger => return Some(MenuKeyboardAction::Exit),
        Key::Character(text)
            if !from_trigger
                && text.chars().count() == 1
                && text != " "
                && !modifiers.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META) =>
        {
            let text = text.to_lowercase();
            (1..=enabled.len())
                .map(|offset| (current.unwrap_or(0) + offset) % enabled.len())
                .find(|position| {
                    items[enabled[*position]]
                        .label
                        .trim()
                        .to_lowercase()
                        .starts_with(&text)
                })
        }
        _ => None,
    };
    target.map(|position| MenuKeyboardAction::FocusItem(enabled[position]))
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;

    use super::{MenuKeyboardAction, MenuKeyboardFocus, MenuKeyboardItem, menu_keyboard_action};

    fn item(label: &str) -> MenuKeyboardItem {
        MenuKeyboardItem {
            label: label.to_owned(),
            disabled: false,
            selected: false,
        }
    }

    fn disabled_item(label: &str) -> MenuKeyboardItem {
        MenuKeyboardItem {
            disabled: true,
            ..item(label)
        }
    }

    fn selected_item(label: &str) -> MenuKeyboardItem {
        MenuKeyboardItem {
            selected: true,
            ..item(label)
        }
    }

    fn press(
        key: &Key,
        focus: MenuKeyboardFocus,
        items: &[MenuKeyboardItem],
    ) -> Option<MenuKeyboardAction> {
        menu_keyboard_action(key, Modifiers::empty(), focus, items)
    }

    fn letter(text: &str) -> Key {
        Key::Character(text.to_owned())
    }

    #[test]
    fn arrow_keys_move_through_items_and_wrap() {
        let items = [item("Settings"), item("Theme"), item("About")];

        assert_eq!(
            press(&Key::ArrowDown, MenuKeyboardFocus::Item(0), &items),
            Some(MenuKeyboardAction::FocusItem(1))
        );
        assert_eq!(
            press(&Key::ArrowDown, MenuKeyboardFocus::Item(2), &items),
            Some(MenuKeyboardAction::FocusItem(0))
        );
        assert_eq!(
            press(&Key::ArrowUp, MenuKeyboardFocus::Item(1), &items),
            Some(MenuKeyboardAction::FocusItem(0))
        );
        assert_eq!(
            press(&Key::ArrowUp, MenuKeyboardFocus::Item(0), &items),
            Some(MenuKeyboardAction::FocusItem(2))
        );
        assert_eq!(
            press(&Key::ArrowDown, MenuKeyboardFocus::Trigger, &items),
            Some(MenuKeyboardAction::FocusItem(0))
        );
        assert_eq!(
            press(&Key::ArrowUp, MenuKeyboardFocus::Trigger, &items),
            Some(MenuKeyboardAction::FocusItem(2))
        );
    }

    #[test]
    fn trigger_arrows_open_a_listbox_at_its_selected_option() {
        let items = [item("Active"), selected_item("Paused"), item("All")];

        assert_eq!(
            press(&Key::ArrowDown, MenuKeyboardFocus::Trigger, &items),
            Some(MenuKeyboardAction::FocusItem(1))
        );
        assert_eq!(
            press(&Key::ArrowUp, MenuKeyboardFocus::Trigger, &items),
            Some(MenuKeyboardAction::FocusItem(1))
        );
        assert_eq!(
            press(&Key::ArrowDown, MenuKeyboardFocus::Item(1), &items),
            Some(MenuKeyboardAction::FocusItem(2))
        );
    }

    #[test]
    fn home_and_end_jump_to_the_edges_inside_the_menu_only() {
        let items = [item("Settings"), item("Theme"), item("About")];

        assert_eq!(
            press(&Key::Home, MenuKeyboardFocus::Item(1), &items),
            Some(MenuKeyboardAction::FocusItem(0))
        );
        assert_eq!(
            press(&Key::End, MenuKeyboardFocus::Item(1), &items),
            Some(MenuKeyboardAction::FocusItem(2))
        );
        assert_eq!(press(&Key::Home, MenuKeyboardFocus::Trigger, &items), None);
        assert_eq!(press(&Key::End, MenuKeyboardFocus::Trigger, &items), None);
    }

    #[test]
    fn letters_focus_the_next_item_with_that_initial() {
        let items = [item("Pin tab"), item("Close tab"), item(" close others ")];

        assert_eq!(
            press(&letter("c"), MenuKeyboardFocus::Item(0), &items),
            Some(MenuKeyboardAction::FocusItem(1))
        );
        assert_eq!(
            press(&letter("C"), MenuKeyboardFocus::Item(1), &items),
            Some(MenuKeyboardAction::FocusItem(2))
        );
        assert_eq!(
            press(&letter("c"), MenuKeyboardFocus::Item(2), &items),
            Some(MenuKeyboardAction::FocusItem(1))
        );
        assert_eq!(
            press(&letter("z"), MenuKeyboardFocus::Item(0), &items),
            None
        );
        assert_eq!(
            press(&letter(" "), MenuKeyboardFocus::Item(0), &items),
            None
        );
        assert_eq!(
            press(&letter("c"), MenuKeyboardFocus::Trigger, &items),
            None
        );
        assert_eq!(
            menu_keyboard_action(
                &letter("c"),
                Modifiers::CONTROL,
                MenuKeyboardFocus::Item(0),
                &items,
            ),
            None
        );
    }

    #[test]
    fn navigation_skips_disabled_items() {
        let pinned_tab_menu = [
            item("Unpin tab"),
            disabled_item("Close tab"),
            item("Close others"),
        ];
        let edges = [
            disabled_item("Rename snapshot"),
            item("Pin tab"),
            item("Close tab"),
            disabled_item("Close others"),
        ];

        assert_eq!(
            press(
                &Key::ArrowDown,
                MenuKeyboardFocus::Item(0),
                &pinned_tab_menu
            ),
            Some(MenuKeyboardAction::FocusItem(2))
        );
        assert_eq!(
            press(&Key::ArrowUp, MenuKeyboardFocus::Item(2), &pinned_tab_menu),
            Some(MenuKeyboardAction::FocusItem(0))
        );
        assert_eq!(
            press(&letter("c"), MenuKeyboardFocus::Item(0), &pinned_tab_menu),
            Some(MenuKeyboardAction::FocusItem(2))
        );
        assert_eq!(
            press(&Key::Home, MenuKeyboardFocus::Item(2), &edges),
            Some(MenuKeyboardAction::FocusItem(1))
        );
        assert_eq!(
            press(&Key::End, MenuKeyboardFocus::Item(1), &edges),
            Some(MenuKeyboardAction::FocusItem(2))
        );
        assert_eq!(
            press(
                &Key::ArrowDown,
                MenuKeyboardFocus::Trigger,
                &[disabled_item("Close tab")]
            ),
            None
        );
    }

    #[test]
    fn escape_dismisses_and_tab_exits_from_inside_the_menu() {
        let items = [item("Settings")];

        assert_eq!(
            press(&Key::Escape, MenuKeyboardFocus::Item(0), &items),
            Some(MenuKeyboardAction::Dismiss)
        );
        assert_eq!(
            press(&Key::Tab, MenuKeyboardFocus::Elsewhere, &items),
            Some(MenuKeyboardAction::Exit)
        );
        assert_eq!(
            press(&Key::Escape, MenuKeyboardFocus::Trigger, &items),
            None
        );
        assert_eq!(press(&Key::Tab, MenuKeyboardFocus::Trigger, &items), None);
    }
}
