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
    let key = event.key();
    // WebKit reports Shift+Tab as Unidentified even when its physical code is Tab.
    if key == "Unidentified" && event.code() == "Tab" {
        "Tab".to_owned()
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
