use dioxus::prelude::*;
use gtl_models::{
    settings::ViewerLanguage,
    viewer::{
        InvalidViewerKeybindings, ParseViewerKeybindingError, ViewerKeybinding,
        ViewerKeybindingAction, ViewerKeybindingPlatform, ViewerKeybindings,
        ViewerKeyboardModifier,
    },
};
use lucide_dioxus::{Keyboard, Pencil, RotateCcw, X};
use wasm_bindgen::JsCast as _;

use crate::shared::{
    browser,
    i18n::{t, use_language},
    keyboard::native_keyboard_event_key,
    ui::{
        Button, ButtonSize, ButtonState, ButtonVariant, Checkbox, FieldError, FieldLabelVisibility,
        PanelDialog, TextInput, dialog::use_dialog_slot, panel_dialog::PanelDialogVariant,
    },
};

#[component]
pub(crate) fn KeybindingsEditor(
    bindings: ViewerKeybindings,
    error: Option<String>,
    onchange: EventHandler<ViewerKeybindings>,
) -> Element {
    let language = use_language();
    let mut query = use_signal(String::new);
    let mut modified = use_signal(|| false);
    let mut recording = use_signal(|| false);
    let mut recording_error = use_signal(|| None);
    let mut reset_error = use_signal(|| None);
    let editor = use_dialog_slot::<ViewerKeybindingAction>();
    let defaults = ViewerKeybindings::for_platform(bindings.platform());
    let capture = use_callback(move |event: web_sys::KeyboardEvent| {
        if !recording() || editor.is_open() || event.is_composing() || event.repeat() {
            return;
        }
        let key = native_keyboard_event_key(&event);
        if key == "Escape" {
            recording.set(false);
            recording_error.set(None);
            return;
        }
        if key == "Tab" && !event.ctrl_key() && !event.alt_key() && !event.meta_key() {
            recording.set(false);
            return;
        }
        if is_modifier(&key) {
            return;
        }
        event.prevent_default();
        event.stop_immediate_propagation();
        match record_keybinding(bindings.platform(), &event) {
            Ok(binding) => {
                query.set(format!("\"{binding}\""));
                recording.set(false);
                recording_error.set(None);
                browser::focus_element("keybindings-search".into());
            }
            Err(error) => recording_error.set(Some(error)),
        }
    });
    browser::use_window_keydown(move |event| capture.call(event));
    let visible = ViewerKeybindingAction::ALL
        .into_iter()
        .filter(|action| !modified() || bindings[*action] != defaults[*action])
        .filter(|action| matches_query(bindings, *action, &query(), language))
        .collect::<Vec<_>>();
    rsx! {
        p { class: "keybindings-description", {t!(language, "keybindings-description")} }
        div { class: "keybindings-toolbar",
            div { class: "keybindings-search",
                TextInput {
                    id: "keybindings-search",
                    "data-tour": super::viewer_settings_form::tours::KEYBINDINGS_SEARCH.value(),
                    label: t!(language, "keybindings-search"),
                    label_visibility: FieldLabelVisibility::Hidden,
                    placeholder: if recording() { t!(language, "keybindings-record-search") } else { t!(language, "keybindings-search-placeholder") },
                    value: query(),
                    autocomplete: "off",
                    oninput: move |event: FormEvent| query.set(event.value()),
                }
                Button {
                    id: "keybindings-record-search",
                    "data-tour": super::viewer_settings_form::tours::KEYBINDINGS_RECORD.value(),
                    size: ButtonSize::IconSmall,
                    variant: ButtonVariant::Toggle,
                    aria_pressed: recording().to_string(),
                    aria_label: t!(language, "keybindings-record"),
                    title: t!(language, "keybindings-record"),
                    onclick: move |_| {
                        recording.toggle();
                        recording_error.set(None);
                    },
                    Keyboard { size: 16 }
                }
                if !query().is_empty() {
                    Button {
                        size: ButtonSize::IconSmall,
                        variant: ButtonVariant::Ghost,
                        aria_label: t!(language, "keybindings-clear-search"),
                        onclick: move |_| {
                            query.set(String::new());
                            recording.set(false);
                            browser::focus_element("keybindings-search".into());
                        },
                        X { size: 14 }
                    }
                }
            }
            Button {
                id: "keybindings-reset-all",
                "data-tour": super::viewer_settings_form::tours::KEYBINDINGS_RESET.value(),
                variant: ButtonVariant::Outline,
                size: ButtonSize::Small,
                state: if bindings == defaults { ButtonState::Disabled } else { ButtonState::Enabled },
                onclick: move |_| {
                    reset_error.set(None);
                    onchange.call(defaults);
                },
                RotateCcw { size: 14 }
                {t!(language, "keybindings-reset-all")}
            }
        }
        div { class: "keybindings-filter-row",
            Checkbox {
                id: "keybindings-modified",
                "data-tour": super::viewer_settings_form::tours::KEYBINDINGS_MODIFIED.value(),
                label: t!(language, "keybindings-modified"),
                checked: modified(),
                onchange: move |value| modified.set(value),
            }
            span { role: "status", {t!(language, "keybindings-count", count = visible.len())} }
        }
        if recording() {
            p { class: "keybindings-record-status", role: "status",
                {t!(language, "keybindings-record-search")}
            }
        }
        FieldError {
            id: "keybindings-error",
            message: error
                .or_else(|| {
                    recording_error()
                        .as_ref()
                        .map(|error| recording_error_message(error, language))
                })
                .or_else(|| reset_error().map(|error| binding_error_message(error, language))),
        }
        div {
            class: "keybindings-table-scroll",
            "data-tour": super::viewer_settings_form::tours::KEYBINDINGS_TABLE.value(),
            table { class: "keybindings-table",
                thead {
                    tr {
                        th { scope: "col", {t!(language, "keybindings-command")} }
                        th { scope: "col", {t!(language, "keybindings-shortcut")} }
                        th { scope: "col", class: "keybindings-context",
                            {t!(language, "keybindings-when")}
                        }
                        th { scope: "col", class: "keybindings-source",
                            {t!(language, "keybindings-source")}
                        }
                        th { scope: "col",
                            span { class: "sr-only", {t!(language, "keybindings-actions")} }
                        }
                    }
                }
                tbody {
                    for action in visible.iter().copied() {
                        KeybindingRow {
                            key: "{action}",
                            action,
                            bindings,
                            onedit: move |action| {
                                recording.set(false);
                                editor.open(action);
                            },
                            onremove: move |action| {
                                if let Ok(next) = bindings.with_binding(action, ViewerKeybinding::unassigned()) {
                                    reset_error.set(None);
                                    onchange.call(next);
                                    browser::focus_element("keybindings-search".into());
                                }
                            },
                            onreset: move |action| {
                                match bindings.with_binding(action, defaults[action]) {
                                    Ok(next) => {
                                        reset_error.set(None);
                                        onchange.call(next);
                                        browser::focus_element("keybindings-search".into());
                                    }
                                    Err(error) => reset_error.set(Some(error)),
                                }
                            },
                        }
                    }
                }
            }
            if visible.is_empty() {
                p { class: "keybindings-empty", {t!(language, "keybindings-empty")} }
            }
        }
        if let Some(action) = editor.subject() {
            KeybindingRecorder {
                key: "{action}",
                action,
                bindings,
                open: editor.is_open(),
                onclose: move |()| editor.close(),
                onclosed: move |()| {
                    editor.release();
                    if web_sys::window()
                        .and_then(|window| window.document())
                        .and_then(|document| {
                            document.get_element_by_id(&format!("keybinding-edit-{action}"))
                        })
                        .is_none()
                    {
                        browser::focus_element("keybindings-search".into());
                    }
                },
                onchange,
            }
        }
    }
}

#[component]
fn KeybindingRow(
    action: ViewerKeybindingAction,
    bindings: ViewerKeybindings,
    onedit: EventHandler<ViewerKeybindingAction>,
    onremove: EventHandler<ViewerKeybindingAction>,
    onreset: EventHandler<ViewerKeybindingAction>,
) -> Element {
    let language = use_language();
    let custom = bindings[action] != ViewerKeybindings::for_platform(bindings.platform())[action];
    let label = action_label(action, language);
    let edit_label = t!(language, "keybindings-edit", command = label.as_str());
    rsx! {
        tr {
            "data-keybinding-action": action.to_string(),
            ondoubleclick: move |_| onedit.call(action),
            td { class: "keybindings-command-cell",
                div { class: "keybindings-command-label", "{label}" }
                code { class: "keybindings-command-id", "{action}" }
            }
            td {
                button {
                    id: format!("keybinding-edit-{action}"),
                    r#type: "button",
                    class: "keybindings-binding-button",
                    aria_label: format!(
                        "{}: {}",
                        edit_label,
                        if bindings[action].is_unassigned() {
                            t!(language, "keybindings-unassigned")
                        } else {
                            bindings
                                .display_keys(action)
                                .map(|key| key.to_string())
                                .collect::<Vec<_>>()
                                .join("+")
                        },
                    ),
                    title: edit_label,
                    onclick: move |_| onedit.call(action),
                    KeybindingKeys { bindings, action }
                }
            }
            td { class: "keybindings-context",
                if matches!(
                    action,
                    ViewerKeybindingAction::NextTab
                    | ViewerKeybindingAction::PreviousTab
                    | ViewerKeybindingAction::CloseTab
                    | ViewerKeybindingAction::PinTab
                    | ViewerKeybindingAction::CloseOtherTabs
                )
                {
                    {t!(language, "keybindings-context-tabs")}
                } else {
                    {t!(language, "keybindings-context-diff")}
                }
            }
            td { class: "keybindings-source",
                span { class: if custom { "keybindings-source-custom" } else { "" },
                    if custom {
                        {t!(language, "keybindings-source-user")}
                    } else {
                        {t!(language, "keybindings-source-default")}
                    }
                }
            }
            td { class: "keybindings-actions",
                Button {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::IconCompact,
                    aria_label: t!(language, "keybindings-edit", command = label.as_str()),
                    title: t!(language, "keybindings-edit", command = label.as_str()),
                    onclick: move |_| onedit.call(action),
                    Pencil { size: 13 }
                }
                if !bindings[action].is_unassigned() {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconCompact,
                        aria_label: t!(language, "keybindings-remove", command = label.as_str()),
                        title: t!(language, "keybindings-remove", command = label.as_str()),
                        onclick: move |_| onremove.call(action),
                        X { size: 13 }
                    }
                }
                if custom {
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::IconCompact,
                        aria_label: t!(language, "keybindings-reset", command = label.as_str()),
                        title: t!(language, "keybindings-reset", command = label.as_str()),
                        onclick: move |_| onreset.call(action),
                        RotateCcw { size: 13 }
                    }
                }
            }
        }
    }
}

#[component]
fn KeybindingKeys(bindings: ViewerKeybindings, action: ViewerKeybindingAction) -> Element {
    let language = use_language();
    rsx! {
        span { class: "keybinding-keys",
            if bindings[action].is_unassigned() {
                span { class: "keybinding-unassigned", {t!(language, "keybindings-unassigned")} }
            } else {
                for key in bindings.display_keys(action) {
                    kbd { "{key}" }
                }
            }
        }
    }
}

#[component]
fn KeybindingRecorder(
    action: ViewerKeybindingAction,
    bindings: ViewerKeybindings,
    open: bool,
    onclose: EventHandler<()>,
    onclosed: EventHandler<()>,
    onchange: EventHandler<ViewerKeybindings>,
) -> Element {
    let language = use_language();
    let mut draft = use_signal(|| None::<Result<ViewerKeybinding, ParseViewerKeybindingError>>);
    let proposed = draft().map(|draft| {
        draft.map_err(RecorderError::Key).and_then(|binding| {
            bindings
                .with_binding(action, binding)
                .map_err(RecorderError::Bindings)
        })
    });
    let save = use_callback(move |()| {
        if let Some(Ok(next)) = draft()
            .and_then(Result::ok)
            .map(|binding| bindings.with_binding(action, binding))
        {
            onchange.call(next);
            onclose.call(());
        }
    });
    let capture = use_callback(move |event: web_sys::KeyboardEvent| {
        if !open || event.is_composing() || event.repeat() {
            return;
        }
        let key = native_keyboard_event_key(&event);
        if is_modifier(&key) {
            return;
        }
        let accelerator = event.ctrl_key() || event.alt_key() || event.meta_key();
        if !accelerator
            && (key == "Tab"
                || ((key == "Enter" || key == " ")
                    && event
                        .target()
                        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                        .is_some_and(|target| target.closest("button").ok().flatten().is_some())))
        {
            return;
        }
        event.prevent_default();
        event.stop_immediate_propagation();
        if !accelerator && key == "Escape" {
            onclose.call(());
        } else if !accelerator && key == "Enter" {
            save.call(());
        } else {
            draft.set(Some(record_keybinding(bindings.platform(), &event)));
        }
    });
    browser::use_window_keydown(move |event| capture.call(event));
    let candidate = draft().and_then(Result::ok);
    let conflict = match &proposed {
        Some(Err(RecorderError::Bindings(InvalidViewerKeybindings::Conflict {
            first,
            second,
            ..
        }))) => Some(if *first == action { *second } else { *first }),
        _ => None,
    };
    rsx! {
        PanelDialog {
            id: "keybinding-recorder",
            trigger_id: format!("keybinding-edit-{action}"),
            open,
            title: t!(language, "keybindings-recorder-title"),
            variant: PanelDialogVariant::Form,
            onclose,
            onclosed,
            div { class: "keybinding-recorder",
                p { class: "keybinding-recorder-command", {action_label(action, language)} }
                p { class: "keybinding-recorder-instructions",
                    {t!(language, "keybindings-recorder-instructions")}
                }
                div {
                    class: "keybinding-recorder-capture",
                    tabindex: "0",
                    "data-dialog-content-initial-focus": "true",
                    role: "status",
                    aria_live: "polite",
                    if let Some(binding) = candidate {
                        if let Ok(display) = ViewerKeybindings::try_from_fn(
                            bindings.platform(),
                            |candidate| {
                                if candidate == action { binding } else { ViewerKeybinding::unassigned() }
                            },
                        )
                        {
                            KeybindingKeys { bindings: display, action }
                        }
                    } else {
                        {t!(language, "keybindings-recorder-waiting")}
                    }
                }
                p { class: "keybinding-recorder-current",
                    {t!(language, "keybindings-current")}
                    KeybindingKeys { bindings, action }
                }
                FieldError {
                    id: "keybinding-recorder-error",
                    message: proposed
                        .as_ref()
                        .and_then(|value| value.as_ref().err())
                        .map(|error| match error {
                            RecorderError::Key(error) => recording_error_message(error, language),
                            RecorderError::Bindings(error) => binding_error_message(*error, language),
                        }),
                }
                if conflict.is_some() {
                    p { class: "keybinding-recorder-instructions",
                        {t!(language, "keybindings-replace-hint")}
                    }
                }
                div { class: "keybinding-recorder-footer",
                    Button {
                        variant: ButtonVariant::Outline,
                        onclick: move |_| onclose.call(()),
                        {t!(language, "keybindings-cancel")}
                    }
                    if let (Some(conflict), Some(binding)) = (conflict, candidate) {
                        Button {
                            id: "keybinding-replace",
                            onclick: move |_| {
                                if let Ok(next) = bindings
                                    .with_binding(conflict, ViewerKeybinding::unassigned())
                                    .and_then(|bindings| bindings.with_binding(action, binding))
                                {
                                    onchange.call(next);
                                    onclose.call(());
                                }
                            },
                            {t!(language, "keybindings-replace")}
                        }
                    } else {
                        Button {
                            id: "keybinding-save",
                            state: if matches!(proposed, Some(Ok(_))) { ButtonState::Enabled } else { ButtonState::Disabled },
                            onclick: move |_| save.call(()),
                            {t!(language, "keybindings-save")}
                        }
                    }
                }
            }
        }
    }
}

enum RecorderError {
    Key(ParseViewerKeybindingError),
    Bindings(InvalidViewerKeybindings),
}

fn record_keybinding(
    platform: ViewerKeybindingPlatform,
    event: &web_sys::KeyboardEvent,
) -> Result<ViewerKeybinding, ParseViewerKeybindingError> {
    ViewerKeybinding::from_keypress(
        platform,
        &native_keyboard_event_key(event),
        [
            event.ctrl_key().then_some(ViewerKeyboardModifier::Control),
            event.alt_key().then_some(ViewerKeyboardModifier::Alt),
            event.shift_key().then_some(ViewerKeyboardModifier::Shift),
            event.meta_key().then_some(ViewerKeyboardModifier::Meta),
        ]
        .into_iter()
        .flatten()
        .collect(),
    )
}

fn is_modifier(key: &str) -> bool {
    matches!(key, "Control" | "Alt" | "Shift" | "Meta" | "AltGraph")
}

fn action_label(action: ViewerKeybindingAction, language: ViewerLanguage) -> String {
    match action {
        ViewerKeybindingAction::SearchFiles => t!(language, "keybindings-search-files"),
        ViewerKeybindingAction::SearchTextInAllFiles => t!(language, "keybindings-search-text"),
        ViewerKeybindingAction::ToggleFilesSidebar => t!(language, "keybindings-toggle-files"),
        ViewerKeybindingAction::ToggleCommitsSidebar => t!(language, "keybindings-toggle-commits"),
        ViewerKeybindingAction::PushDiff => t!(language, "keybindings-push"),
        ViewerKeybindingAction::NextTab => t!(language, "keybindings-next-tab"),
        ViewerKeybindingAction::PreviousTab => t!(language, "keybindings-previous-tab"),
        ViewerKeybindingAction::CloseTab => t!(language, "keybindings-close-tab"),
        ViewerKeybindingAction::PinTab => t!(language, "keybindings-pin-tab"),
        ViewerKeybindingAction::CloseOtherTabs => t!(language, "keybindings-close-others"),
        ViewerKeybindingAction::ToggleFileReviewed => {
            t!(language, "keybindings-toggle-file-reviewed")
        }
    }
}

fn binding_error_message(error: InvalidViewerKeybindings, language: ViewerLanguage) -> String {
    match error {
        InvalidViewerKeybindings::Conflict { first, second, .. } => t!(
            language,
            "keybindings-conflict",
            first = action_label(first, language),
            second = action_label(second, language)
        ),
        InvalidViewerKeybindings::AmbiguousModifiers { .. } => {
            t!(language, "keybindings-ambiguous")
        }
    }
}

fn recording_error_message(error: &ParseViewerKeybindingError, language: ViewerLanguage) -> String {
    match error {
        ParseViewerKeybindingError::MissingAccelerator => {
            t!(language, "keybindings-modifier-required")
        }
        _ => t!(language, "keybindings-unsupported"),
    }
}

fn matches_query(
    bindings: ViewerKeybindings,
    action: ViewerKeybindingAction,
    query: &str,
    language: ViewerLanguage,
) -> bool {
    let query = query.trim();
    if let Some(exact) = query
        .strip_prefix('"')
        .and_then(|query| query.strip_suffix('"'))
    {
        return exact
            .parse::<ViewerKeybinding>()
            .is_ok_and(|binding| bindings[action].conflicts_with(binding, bindings.platform()));
    }
    let searchable = format!(
        "{} {action} {} {}",
        action_label(action, language),
        bindings[action],
        bindings
            .display_keys(action)
            .map(|key| key.to_string())
            .collect::<Vec<_>>()
            .join("+")
    )
    .to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|term| searchable.contains(term))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_finds_labels_identifiers_and_exact_recorded_shortcuts() {
        let bindings = ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux);
        for query in ["find file", "SEARCH_FILES", "ctrl+p", "\"control+p\""] {
            assert!(matches_query(
                bindings,
                ViewerKeybindingAction::SearchFiles,
                query,
                ViewerLanguage::EnUs
            ));
        }
        assert!(matches_query(
            bindings,
            ViewerKeybindingAction::SearchFiles,
            "encontrar arquivo",
            ViewerLanguage::PtBr
        ));
        assert!(!matches_query(
            bindings,
            ViewerKeybindingAction::SearchFiles,
            "\"ctrl+shift+p\"",
            ViewerLanguage::EnUs
        ));
        assert!(!matches_query(
            bindings,
            ViewerKeybindingAction::SearchFiles,
            "\"unsupported\"",
            ViewerLanguage::EnUs
        ));
    }
}
