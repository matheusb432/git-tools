#[cfg(feature = "component-preview")]
use dioxus::prelude::*;
use gtl_models::viewer::{
    ViewerKeybindingAction, ViewerKeybindings, ViewerKeyboardModifier, ViewerKeyboardModifiers,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewerKeyboardInput<'a> {
    key: &'a str,
    modifiers: ViewerKeyboardModifiers,
    composing: bool,
}

#[cfg(feature = "component-preview")]
pub(crate) fn keyboard_event_matches(
    event: &KeyboardEvent,
    keybindings: ViewerKeybindings,
    action: ViewerKeybindingAction,
) -> bool {
    let key = event.key().to_string();
    let modifiers = event.modifiers();
    keybinding_matches(
        keybindings,
        action,
        ViewerKeyboardInput {
            key: &key,
            modifiers: [
                modifiers.ctrl().then_some(ViewerKeyboardModifier::Control),
                modifiers.alt().then_some(ViewerKeyboardModifier::Alt),
                modifiers.shift().then_some(ViewerKeyboardModifier::Shift),
                modifiers.meta().then_some(ViewerKeyboardModifier::Meta),
            ]
            .into_iter()
            .flatten()
            .collect(),
            composing: event.is_composing(),
        },
    )
}

pub(crate) fn native_keyboard_event_matches(
    event: &web_sys::KeyboardEvent,
    keybindings: ViewerKeybindings,
    action: ViewerKeybindingAction,
) -> bool {
    let key = native_keyboard_event_key(event);
    keybinding_matches(
        keybindings,
        action,
        ViewerKeyboardInput {
            key: &key,
            modifiers: [
                event.ctrl_key().then_some(ViewerKeyboardModifier::Control),
                event.alt_key().then_some(ViewerKeyboardModifier::Alt),
                event.shift_key().then_some(ViewerKeyboardModifier::Shift),
                event.meta_key().then_some(ViewerKeyboardModifier::Meta),
            ]
            .into_iter()
            .flatten()
            .collect(),
            composing: event.is_composing(),
        },
    )
}

pub(crate) fn native_keyboard_event_key(event: &web_sys::KeyboardEvent) -> String {
    keyboard_key(&event.key(), &event.code()).to_owned()
}

fn keyboard_key<'a>(key: &'a str, code: &'a str) -> &'a str {
    // Shifted digits and macOS Option letters can report punctuation instead of the bound key.
    if let Some(digit) = code
        .strip_prefix("Digit")
        .filter(|digit| digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit())
    {
        return digit;
    }
    if key.chars().count() == 1
        && !key.is_ascii()
        && let Some(letter) = code
            .strip_prefix("Key")
            .filter(|letter| letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic())
    {
        return letter;
    }
    // WebKit reports Shift+Tab as Unidentified even when its physical code is Tab.
    if key == "Unidentified" && code == "Tab" {
        "Tab"
    } else {
        key
    }
}

fn keybinding_matches(
    keybindings: ViewerKeybindings,
    action: ViewerKeybindingAction,
    input: ViewerKeyboardInput<'_>,
) -> bool {
    !input.composing && keybindings.matches_keypress(action, input.key, input.modifiers)
}

#[cfg(test)]
mod tests {
    use gtl_models::viewer::{
        ViewerKeybindingAction, ViewerKeybindingPlatform, ViewerKeybindings,
        ViewerKeyboardModifier, ViewerKeyboardModifiers,
    };

    use super::{ViewerKeyboardInput, keybinding_matches};

    #[test]
    fn shifted_digits_and_option_letters_use_the_bindable_key() {
        assert_eq!(super::keyboard_key("!", "Digit1"), "1");
        assert_eq!(super::keyboard_key("π", "KeyP"), "P");
        assert_eq!(super::keyboard_key("Unidentified", "Tab"), "Tab");
        assert_eq!(super::keyboard_key("F3", "F3"), "F3");
    }

    fn input(key: &str) -> ViewerKeyboardInput<'_> {
        ViewerKeyboardInput {
            key,
            modifiers: ViewerKeyboardModifiers::default(),
            composing: false,
        }
    }

    #[test]
    fn requires_an_exact_platform_resolved_chord_and_ignores_composition() {
        let mut linux = input("f");
        linux.modifiers = [ViewerKeyboardModifier::Control].into_iter().collect();
        assert!(keybinding_matches(
            ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux),
            ViewerKeybindingAction::SearchTextInAllFiles,
            linux
        ));

        linux.modifiers = [
            ViewerKeyboardModifier::Control,
            ViewerKeyboardModifier::Shift,
        ]
        .into_iter()
        .collect();
        assert!(!keybinding_matches(
            ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux),
            ViewerKeybindingAction::SearchTextInAllFiles,
            linux
        ));
        linux.modifiers = [ViewerKeyboardModifier::Control].into_iter().collect();
        linux.composing = true;
        assert!(!keybinding_matches(
            ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux),
            ViewerKeybindingAction::SearchTextInAllFiles,
            linux
        ));

        let mut macos = input("F");
        macos.modifiers = [ViewerKeyboardModifier::Meta].into_iter().collect();
        assert!(keybinding_matches(
            ViewerKeybindings::for_platform(ViewerKeybindingPlatform::MacOs),
            ViewerKeybindingAction::SearchTextInAllFiles,
            macos
        ));
        macos.modifiers = [
            ViewerKeyboardModifier::Control,
            ViewerKeyboardModifier::Meta,
        ]
        .into_iter()
        .collect();
        assert!(!keybinding_matches(
            ViewerKeybindings::for_platform(ViewerKeybindingPlatform::MacOs),
            ViewerKeybindingAction::SearchTextInAllFiles,
            macos
        ));
    }
}
