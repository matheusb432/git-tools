use std::{fmt, iter, ops::Index, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewerKeybindingAction {
    SearchFiles,
    SearchTextInAllFiles,
    ToggleFilesSidebar,
    ToggleCommitsSidebar,
    PushDiff,
}

impl ViewerKeybindingAction {
    const ALL: [Self; 5] = [
        Self::SearchFiles,
        Self::SearchTextInAllFiles,
        Self::ToggleFilesSidebar,
        Self::ToggleCommitsSidebar,
        Self::PushDiff,
    ];
    const COUNT: usize = Self::ALL.len();

    const fn index(self) -> usize {
        match self {
            Self::SearchFiles => 0,
            Self::SearchTextInAllFiles => 1,
            Self::ToggleFilesSidebar => 2,
            Self::ToggleCommitsSidebar => 3,
            Self::PushDiff => 4,
        }
    }

    const fn default_keybinding(self) -> ViewerKeybinding {
        match self {
            Self::SearchFiles => ViewerKeybinding::primary_character('p'),
            Self::SearchTextInAllFiles => ViewerKeybinding::primary_character('f'),
            Self::ToggleFilesSidebar => ViewerKeybinding::primary_character('b'),
            Self::ToggleCommitsSidebar => ViewerKeybinding {
                key: ViewerKey::Character('b'),
                modifiers: ViewerModifiers(
                    ViewerModifier::Primary as u8 | ViewerModifier::Alt as u8,
                ),
            },
            Self::PushDiff => ViewerKeybinding {
                key: ViewerKey::Named(0),
                modifiers: ViewerModifiers::only(ViewerModifier::Primary),
            },
        }
    }
}

impl fmt::Display for ViewerKeybindingAction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SearchFiles => "search_files",
            Self::SearchTextInAllFiles => "search_text_in_all_files",
            Self::ToggleFilesSidebar => "toggle_files_sidebar",
            Self::ToggleCommitsSidebar => "toggle_commits_sidebar",
            Self::PushDiff => "push_diff",
        })
    }
}

fn action_pairs() -> impl Iterator<Item = (ViewerKeybindingAction, ViewerKeybindingAction)> {
    ViewerKeybindingAction::ALL
        .into_iter()
        .enumerate()
        .flat_map(|(index, first)| {
            ViewerKeybindingAction::ALL
                .into_iter()
                .skip(index + 1)
                .map(move |second| (first, second))
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerKeybindingPlatform {
    Linux,
    Windows,
    MacOs,
    Other,
}

impl ViewerKeybindingPlatform {
    #[must_use]
    pub fn current() -> Self {
        match std::env::consts::OS {
            "linux" => Self::Linux,
            "windows" => Self::Windows,
            "macos" => Self::MacOs,
            _ => Self::Other,
        }
    }
}

const NAMED_VIEWER_KEYS: [(&str, &str); 15] = [
    ("enter", "Enter"),
    ("escape", "Escape"),
    ("space", "Space"),
    ("tab", "Tab"),
    ("backspace", "Backspace"),
    ("delete", "Delete"),
    ("insert", "Insert"),
    ("arrowup", "ArrowUp"),
    ("arrowdown", "ArrowDown"),
    ("arrowleft", "ArrowLeft"),
    ("arrowright", "ArrowRight"),
    ("home", "Home"),
    ("end", "End"),
    ("pageup", "PageUp"),
    ("pagedown", "PageDown"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ViewerKey {
    Character(char),
    Function(u8),
    Named(u8),
}

impl ViewerKey {
    fn parse(token: &str) -> Option<Self> {
        match token.as_bytes() {
            [value] if value.is_ascii_alphabetic() => {
                Some(Self::Character(char::from(*value).to_ascii_lowercase()))
            }
            [value] if value.is_ascii_digit() => Some(Self::Character(char::from(*value))),
            _ => token
                .strip_prefix(['f', 'F'])
                .and_then(|number| number.parse::<u8>().ok())
                .filter(|number| (1..=12).contains(number))
                .map(Self::Function)
                .or_else(|| {
                    NAMED_VIEWER_KEYS
                        .iter()
                        .position(|(config, _)| token.eq_ignore_ascii_case(config))
                        .and_then(|index| u8::try_from(index).ok())
                        .map(Self::Named)
                }),
        }
    }

    fn matches_browser_key(self, value: &str) -> bool {
        Self::parse(match value {
            " " => "space",
            _ => value,
        }) == Some(self)
    }
}

impl fmt::Display for ViewerKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Character(value) => write!(formatter, "{value}"),
            Self::Function(number) => write!(formatter, "f{number}"),
            Self::Named(index) => formatter.write_str(NAMED_VIEWER_KEYS[usize::from(*index)].0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
enum ViewerModifier {
    Primary = 1 << 0,
    Alt = 1 << 1,
    Shift = 1 << 2,
    Meta = 1 << 3,
}

impl ViewerModifier {
    const ALL: [Self; 4] = [Self::Primary, Self::Alt, Self::Shift, Self::Meta];

    fn parse(token: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|modifier| token.eq_ignore_ascii_case(modifier.config_value()))
    }

    const fn config_value(self) -> &'static str {
        match self {
            Self::Primary => "ctrl",
            Self::Alt => "alt",
            Self::Shift => "shift",
            Self::Meta => "meta",
        }
    }

    const fn display_value(self, platform: ViewerKeybindingPlatform) -> &'static str {
        match (self, platform) {
            (Self::Primary | Self::Meta, ViewerKeybindingPlatform::MacOs) => "⌘",
            (Self::Primary, _) => "Ctrl",
            (Self::Alt, ViewerKeybindingPlatform::MacOs) => "⌥",
            (Self::Alt, _) => "Alt",
            (Self::Shift, ViewerKeybindingPlatform::MacOs) => "⇧",
            (Self::Shift, _) => "Shift",
            (Self::Meta, ViewerKeybindingPlatform::Windows) => "Win",
            (Self::Meta, ViewerKeybindingPlatform::Linux) => "Super",
            (Self::Meta, ViewerKeybindingPlatform::Other) => "Meta",
        }
    }

    const fn keyboard_modifier(self, platform: ViewerKeybindingPlatform) -> ViewerKeyboardModifier {
        match (self, platform) {
            (Self::Primary, ViewerKeybindingPlatform::MacOs) | (Self::Meta, _) => {
                ViewerKeyboardModifier::Meta
            }
            (Self::Primary, _) => ViewerKeyboardModifier::Control,
            (Self::Alt, _) => ViewerKeyboardModifier::Alt,
            (Self::Shift, _) => ViewerKeyboardModifier::Shift,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
struct ViewerModifiers(u8);

impl ViewerModifiers {
    const fn only(modifier: ViewerModifier) -> Self {
        Self(modifier as u8)
    }

    const fn contains(self, modifier: ViewerModifier) -> bool {
        self.0 & modifier as u8 != 0
    }

    fn insert(&mut self, modifier: ViewerModifier) -> bool {
        let was_absent = !self.contains(modifier);
        self.0 |= modifier as u8;
        was_absent
    }

    fn iter(self) -> impl Iterator<Item = ViewerModifier> {
        ViewerModifier::ALL
            .into_iter()
            .filter(move |modifier| self.contains(*modifier))
    }

    const fn has_accelerator(self) -> bool {
        const ACCELERATORS: u8 =
            ViewerModifier::Primary as u8 | ViewerModifier::Alt as u8 | ViewerModifier::Meta as u8;
        self.0 & ACCELERATORS != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseViewerKeybindingError {
    #[error("keyboard chord must not be empty")]
    Empty,
    #[error("keyboard chord contains an empty token")]
    EmptyToken,
    #[error("modifier `{modifier}` appears more than once")]
    DuplicateModifier { modifier: &'static str },
    #[error("keyboard chord contains multiple keys: `{first}` and `{second}`")]
    MultipleKeys { first: String, second: String },
    #[error("keyboard chord must contain one key")]
    MissingKey,
    #[error(
        "unsupported token `{token}`; use ctrl, alt, shift, or meta plus a-z, 0-9, f1-f12, enter, escape, space, tab, backspace, delete, insert, an arrow key, home, end, pageup, or pagedown"
    )]
    UnsupportedToken { token: String },
    #[error("keyboard chord must include ctrl, alt, or meta")]
    MissingAccelerator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewerKeybinding {
    modifiers: ViewerModifiers,
    key: ViewerKey,
}

impl ViewerKeybinding {
    const fn primary_character(key: char) -> Self {
        Self {
            modifiers: ViewerModifiers::only(ViewerModifier::Primary),
            key: ViewerKey::Character(key),
        }
    }

    fn keyboard_modifiers(self, platform: ViewerKeybindingPlatform) -> ViewerKeyboardModifiers {
        self.modifiers
            .iter()
            .map(|modifier| modifier.keyboard_modifier(platform))
            .collect()
    }

    const fn has_ambiguous_macos_modifiers(self, platform: ViewerKeybindingPlatform) -> bool {
        matches!(platform, ViewerKeybindingPlatform::MacOs)
            && self.modifiers.contains(ViewerModifier::Primary)
            && self.modifiers.contains(ViewerModifier::Meta)
    }

    fn conflicts_with(self, other: Self, platform: ViewerKeybindingPlatform) -> bool {
        self.key == other.key
            && self.keyboard_modifiers(platform) == other.keyboard_modifiers(platform)
    }
}

impl FromStr for ViewerKeybinding {
    type Err = ParseViewerKeybindingError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        if raw.trim().is_empty() {
            return Err(ParseViewerKeybindingError::Empty);
        }
        let mut modifiers = ViewerModifiers::default();
        let mut key = None;
        for token in raw.split('+').map(str::trim) {
            if token.is_empty() {
                return Err(ParseViewerKeybindingError::EmptyToken);
            }
            match (ViewerModifier::parse(token), ViewerKey::parse(token)) {
                (Some(modifier), _) => set_modifier(&mut modifiers, modifier)?,
                (None, Some(value)) => set_key(&mut key, value)?,
                (None, None) => {
                    return Err(ParseViewerKeybindingError::UnsupportedToken {
                        token: token.to_owned(),
                    });
                }
            }
        }
        let key = key.ok_or(ParseViewerKeybindingError::MissingKey)?;
        if !modifiers.has_accelerator() {
            return Err(ParseViewerKeybindingError::MissingAccelerator);
        }
        Ok(Self { modifiers, key })
    }
}

fn set_modifier(
    modifiers: &mut ViewerModifiers,
    modifier: ViewerModifier,
) -> Result<(), ParseViewerKeybindingError> {
    modifiers
        .insert(modifier)
        .then_some(())
        .ok_or(ParseViewerKeybindingError::DuplicateModifier {
            modifier: modifier.config_value(),
        })
}

fn set_key(slot: &mut Option<ViewerKey>, key: ViewerKey) -> Result<(), ParseViewerKeybindingError> {
    match slot.replace(key) {
        Some(first) => Err(ParseViewerKeybindingError::MultipleKeys {
            first: first.to_string(),
            second: key.to_string(),
        }),
        None => Ok(()),
    }
}

impl fmt::Display for ViewerKeybinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for modifier in self.modifiers.iter() {
            write!(formatter, "{}+", modifier.config_value())?;
        }
        write!(formatter, "{}", self.key)
    }
}

impl Serialize for ViewerKeybinding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ViewerKeybinding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(D::Error::custom)
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerKeyboardModifier {
    Control = 1 << 0,
    Alt = 1 << 1,
    Shift = 1 << 2,
    Meta = 1 << 3,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewerKeyboardModifiers(u8);

impl FromIterator<ViewerKeyboardModifier> for ViewerKeyboardModifiers {
    fn from_iter<T: IntoIterator<Item = ViewerKeyboardModifier>>(modifiers: T) -> Self {
        modifiers
            .into_iter()
            .fold(Self::default(), |set, modifier| {
                Self(set.0 | modifier as u8)
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewerKeybindingDisplayKey {
    Named(&'static str),
    Character(char),
    Function(u8),
}

impl ViewerKeybindingDisplayKey {
    fn from_modifier(modifier: ViewerModifier, platform: ViewerKeybindingPlatform) -> Self {
        Self::Named(modifier.display_value(platform))
    }

    fn from_key(key: ViewerKey) -> Self {
        match key {
            ViewerKey::Character(value) => Self::Character(value.to_ascii_uppercase()),
            ViewerKey::Function(number) => Self::Function(number),
            ViewerKey::Named(index) => Self::Named(NAMED_VIEWER_KEYS[usize::from(index)].1),
        }
    }
}

impl fmt::Display for ViewerKeybindingDisplayKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Named(value) => formatter.write_str(value),
            Self::Character(value) => write!(formatter, "{value}"),
            Self::Function(number) => write!(formatter, "F{number}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidViewerKeybindings {
    #[error("`push_diff` uses reserved tab shortcut (`{binding}`)")]
    ReservedTabShortcut { binding: ViewerKeybinding },
    #[error("`{action}` uses both ctrl and meta, which are both Command on macOS")]
    AmbiguousMacOsModifiers { action: ViewerKeybindingAction },
    #[error("`{first}` conflicts with `{second}` on the selected platform (`{binding}`)")]
    Conflict {
        first: ViewerKeybindingAction,
        second: ViewerKeybindingAction,
        binding: ViewerKeybinding,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewerKeybindings {
    platform: ViewerKeybindingPlatform,
    bindings: [ViewerKeybinding; ViewerKeybindingAction::COUNT],
}

impl ViewerKeybindings {
    pub fn try_from_fn(
        platform: ViewerKeybindingPlatform,
        binding_for_action: impl FnMut(ViewerKeybindingAction) -> ViewerKeybinding,
    ) -> Result<Self, InvalidViewerKeybindings> {
        let keybindings = Self {
            platform,
            bindings: ViewerKeybindingAction::ALL.map(binding_for_action),
        };
        keybindings.validate()?;
        Ok(keybindings)
    }

    #[must_use]
    pub fn for_platform(platform: ViewerKeybindingPlatform) -> Self {
        Self {
            platform,
            bindings: ViewerKeybindingAction::ALL.map(ViewerKeybindingAction::default_keybinding),
        }
    }

    #[must_use]
    pub const fn platform(self) -> ViewerKeybindingPlatform {
        self.platform
    }

    #[must_use]
    pub fn matches_keypress(
        self,
        action: ViewerKeybindingAction,
        key: &str,
        modifiers: ViewerKeyboardModifiers,
    ) -> bool {
        let binding = self[action];
        binding.key.matches_browser_key(key)
            && binding.keyboard_modifiers(self.platform) == modifiers
    }

    pub fn display_keys(
        self,
        action: ViewerKeybindingAction,
    ) -> impl Iterator<Item = ViewerKeybindingDisplayKey> {
        let binding = self[action];
        binding
            .modifiers
            .iter()
            .map(move |modifier| ViewerKeybindingDisplayKey::from_modifier(modifier, self.platform))
            .chain(iter::once(ViewerKeybindingDisplayKey::from_key(
                binding.key,
            )))
    }

    #[must_use]
    pub fn aria_keyshortcuts(self, action: ViewerKeybindingAction) -> String {
        let binding = self[action];
        let modifiers = binding.keyboard_modifiers(self.platform);
        [
            (ViewerKeyboardModifier::Control, "Control"),
            (ViewerKeyboardModifier::Alt, "Alt"),
            (ViewerKeyboardModifier::Shift, "Shift"),
            (ViewerKeyboardModifier::Meta, "Meta"),
        ]
        .into_iter()
        .filter(|(modifier, _)| modifiers.0 & *modifier as u8 != 0)
        .map(|(_, name)| name.to_owned())
        .chain(iter::once(
            ViewerKeybindingDisplayKey::from_key(binding.key).to_string(),
        ))
        .collect::<Vec<_>>()
        .join("+")
    }

    fn validate(self) -> Result<(), InvalidViewerKeybindings> {
        if [
            ("Tab", ViewerKeyboardModifier::Control, false),
            ("Tab", ViewerKeyboardModifier::Control, true),
            ("w", ViewerKeyboardModifier::Control, false),
            ("p", ViewerKeyboardModifier::Alt, false),
            ("o", ViewerKeyboardModifier::Alt, false),
        ]
        .into_iter()
        .any(|(key, modifier, shift)| {
            let modifiers = [
                Some(modifier),
                shift.then_some(ViewerKeyboardModifier::Shift),
            ]
            .into_iter()
            .flatten()
            .collect();
            self.matches_keypress(ViewerKeybindingAction::PushDiff, key, modifiers)
        }) {
            return Err(InvalidViewerKeybindings::ReservedTabShortcut {
                binding: self[ViewerKeybindingAction::PushDiff],
            });
        }
        if let Some(action) = ViewerKeybindingAction::ALL
            .into_iter()
            .find(|action| self[*action].has_ambiguous_macos_modifiers(self.platform))
        {
            return Err(InvalidViewerKeybindings::AmbiguousMacOsModifiers { action });
        }
        if let Some((first, second)) = action_pairs()
            .find(|(first, second)| self[*first].conflicts_with(self[*second], self.platform))
        {
            return Err(InvalidViewerKeybindings::Conflict {
                first,
                second,
                binding: self[first],
            });
        }
        Ok(())
    }
}

impl Index<ViewerKeybindingAction> for ViewerKeybindings {
    type Output = ViewerKeybinding;

    fn index(&self, action: ViewerKeybindingAction) -> &Self::Output {
        &self.bindings[action.index()]
    }
}

impl Default for ViewerKeybindings {
    fn default() -> Self {
        Self::for_platform(ViewerKeybindingPlatform::current())
    }
}

#[derive(Serialize, Deserialize)]
struct SerializedViewerKeybindings {
    platform: ViewerKeybindingPlatform,
    search_files: ViewerKeybinding,
    search_text_in_all_files: ViewerKeybinding,
    toggle_files_sidebar: ViewerKeybinding,
    toggle_commits_sidebar: ViewerKeybinding,
    #[serde(default)]
    push_diff: Option<ViewerKeybinding>,
}

impl Serialize for ViewerKeybindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SerializedViewerKeybindings {
            platform: self.platform,
            search_files: self[ViewerKeybindingAction::SearchFiles],
            search_text_in_all_files: self[ViewerKeybindingAction::SearchTextInAllFiles],
            toggle_files_sidebar: self[ViewerKeybindingAction::ToggleFilesSidebar],
            toggle_commits_sidebar: self[ViewerKeybindingAction::ToggleCommitsSidebar],
            push_diff: Some(self[ViewerKeybindingAction::PushDiff]),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ViewerKeybindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = SerializedViewerKeybindings::deserialize(deserializer)?;
        Self::try_from_fn(value.platform, |action| match action {
            ViewerKeybindingAction::SearchFiles => value.search_files,
            ViewerKeybindingAction::SearchTextInAllFiles => value.search_text_in_all_files,
            ViewerKeybindingAction::ToggleFilesSidebar => value.toggle_files_sidebar,
            ViewerKeybindingAction::ToggleCommitsSidebar => value.toggle_commits_sidebar,
            ViewerKeybindingAction::PushDiff => value
                .push_diff
                .unwrap_or(ViewerKeybindingAction::PushDiff.default_keybinding()),
        })
        .map_err(D::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        InvalidViewerKeybindings, ParseViewerKeybindingError, ViewerKeybinding,
        ViewerKeybindingAction, ViewerKeybindingPlatform, ViewerKeybindings,
        ViewerKeyboardModifier,
    };

    fn try_keybindings(
        platform: ViewerKeybindingPlatform,
        search_files: &str,
        search_text_in_all_files: &str,
    ) -> Result<ViewerKeybindings, InvalidViewerKeybindings> {
        let search_files = search_files.parse::<ViewerKeybinding>().unwrap();
        let search_text_in_all_files = search_text_in_all_files
            .parse::<ViewerKeybinding>()
            .unwrap();
        ViewerKeybindings::try_from_fn(platform, |action| match action {
            ViewerKeybindingAction::SearchFiles => search_files,
            ViewerKeybindingAction::SearchTextInAllFiles => search_text_in_all_files,
            ViewerKeybindingAction::ToggleFilesSidebar
            | ViewerKeybindingAction::ToggleCommitsSidebar
            | ViewerKeybindingAction::PushDiff => ViewerKeybindings::for_platform(platform)[action],
        })
    }

    #[test]
    fn sidebar_defaults_match_vs_code_and_reject_conflicting_overrides() {
        for platform in [
            ViewerKeybindingPlatform::Linux,
            ViewerKeybindingPlatform::Windows,
        ] {
            let bindings = ViewerKeybindings::for_platform(platform);
            assert_display_keys(
                bindings,
                ViewerKeybindingAction::ToggleFilesSidebar,
                ["Ctrl", "B"],
            );
            assert_display_keys(
                bindings,
                ViewerKeybindingAction::ToggleCommitsSidebar,
                ["Ctrl", "Alt", "B"],
            );
            let conflict = ViewerKeybindings::try_from_fn(platform, |action| match action {
                ViewerKeybindingAction::ToggleCommitsSidebar => {
                    bindings[ViewerKeybindingAction::ToggleFilesSidebar]
                }
                _ => bindings[action],
            });
            assert!(conflict.is_err());
        }
    }

    #[test]
    fn push_shortcut_resolves_platform_modifiers_and_accessible_hints() {
        for (platform, modifier, aria, display) in [
            (
                ViewerKeybindingPlatform::Linux,
                ViewerKeyboardModifier::Control,
                "Control+Enter",
                "Ctrl",
            ),
            (
                ViewerKeybindingPlatform::Windows,
                ViewerKeyboardModifier::Control,
                "Control+Enter",
                "Ctrl",
            ),
            (
                ViewerKeybindingPlatform::MacOs,
                ViewerKeyboardModifier::Meta,
                "Meta+Enter",
                "⌘",
            ),
        ] {
            let bindings = ViewerKeybindings::for_platform(platform);
            assert!(bindings.matches_keypress(
                ViewerKeybindingAction::PushDiff,
                "Enter",
                [modifier].into_iter().collect()
            ));
            assert!(
                !bindings.matches_keypress(
                    ViewerKeybindingAction::PushDiff,
                    "Enter",
                    [modifier, ViewerKeyboardModifier::Shift]
                        .into_iter()
                        .collect()
                )
            );
            assert_eq!(
                bindings.aria_keyshortcuts(ViewerKeybindingAction::PushDiff),
                aria
            );
            assert_display_keys(
                bindings,
                ViewerKeybindingAction::PushDiff,
                [display, "Enter"],
            );
        }
    }

    #[test]
    fn push_overrides_cannot_shadow_another_configured_action() {
        let bindings = ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux);
        let result = ViewerKeybindings::try_from_fn(bindings.platform(), |action| {
            if action == ViewerKeybindingAction::PushDiff {
                bindings[ViewerKeybindingAction::SearchFiles]
            } else {
                bindings[action]
            }
        });
        assert!(matches!(
            result,
            Err(InvalidViewerKeybindings::Conflict {
                first: ViewerKeybindingAction::SearchFiles,
                second: ViewerKeybindingAction::PushDiff,
                ..
            })
        ));
    }

    #[test]
    fn push_overrides_cannot_replace_fixed_tab_shortcuts() {
        let cases = [
            ViewerKeybindingPlatform::Linux,
            ViewerKeybindingPlatform::Windows,
        ]
        .into_iter()
        .flat_map(|platform| {
            ["ctrl+tab", "ctrl+shift+tab", "ctrl+w", "alt+p", "alt+o"]
                .into_iter()
                .map(move |chord| (platform, chord))
        });
        for (platform, chord) in cases {
            let defaults = ViewerKeybindings::for_platform(platform);
            let binding = chord.parse().unwrap();
            let result = ViewerKeybindings::try_from_fn(platform, |action| match action {
                ViewerKeybindingAction::PushDiff => binding,
                _ => defaults[action],
            });
            assert!(matches!(
                result,
                Err(InvalidViewerKeybindings::ReservedTabShortcut { .. })
            ));
        }
    }

    fn assert_display_keys<const N: usize>(
        keybindings: ViewerKeybindings,
        action: ViewerKeybindingAction,
        expected: [&str; N],
    ) {
        let displayed = keybindings.display_keys(action).map(|key| key.to_string());
        assert!(displayed.eq(expected.into_iter().map(str::to_owned)));
    }

    #[test]
    fn parses_case_insensitively_and_formats_as_lowercase() {
        let binding = " shift + ctrl + alt + f12 "
            .parse::<ViewerKeybinding>()
            .unwrap();

        assert_eq!(binding.to_string(), "ctrl+alt+shift+f12");
        let linux = try_keybindings(
            ViewerKeybindingPlatform::Linux,
            &binding.to_string(),
            "alt+x",
        )
        .unwrap();
        let macos = try_keybindings(
            ViewerKeybindingPlatform::MacOs,
            &binding.to_string(),
            "alt+x",
        )
        .unwrap();
        assert_display_keys(
            linux,
            ViewerKeybindingAction::SearchFiles,
            ["Ctrl", "Alt", "Shift", "F12"],
        );
        assert_display_keys(
            macos,
            ViewerKeybindingAction::SearchFiles,
            ["⌘", "⌥", "⇧", "F12"],
        );
    }

    #[test]
    fn presents_meta_with_the_platform_name() {
        for (platform, label) in [
            (ViewerKeybindingPlatform::Linux, "Super"),
            (ViewerKeybindingPlatform::Windows, "Win"),
            (ViewerKeybindingPlatform::MacOs, "⌘"),
            (ViewerKeybindingPlatform::Other, "Meta"),
        ] {
            let keybindings = try_keybindings(platform, "meta+k", "ctrl+f").unwrap();
            assert_display_keys(
                keybindings,
                ViewerKeybindingAction::SearchFiles,
                [label, "K"],
            );
        }
    }

    #[test]
    fn rejects_unsupported_duplicate_and_unmodified_chords() {
        for alias in ["Cmd", "Win", "Option", "Control"] {
            let chord = format!("{alias}+p");
            assert!(matches!(
                chord.parse::<ViewerKeybinding>(),
                Err(ParseViewerKeybindingError::UnsupportedToken { token }) if token == alias
            ));
        }
        assert!(matches!(
            "Ctrl+ctrl+P".parse::<ViewerKeybinding>(),
            Err(ParseViewerKeybindingError::DuplicateModifier { modifier }) if modifier == "ctrl"
        ));
        for chord in ["Shift+P", "P"] {
            assert_eq!(
                chord.parse::<ViewerKeybinding>(),
                Err(ParseViewerKeybindingError::MissingAccelerator)
            );
        }
    }

    #[test]
    fn accepts_the_closed_common_key_vocabulary() {
        for chord in [
            "Alt+0",
            "Meta+Enter",
            "Ctrl+Escape",
            "Alt+Space",
            "Ctrl+Tab",
            "Ctrl+Backspace",
            "Ctrl+Delete",
            "Ctrl+Insert",
            "Ctrl+ArrowUp",
            "Ctrl+ArrowDown",
            "Ctrl+ArrowLeft",
            "Ctrl+ArrowRight",
            "Ctrl+Home",
            "Ctrl+End",
            "Ctrl+PageUp",
            "Ctrl+PageDown",
        ] {
            assert_eq!(
                chord.parse::<ViewerKeybinding>().unwrap().to_string(),
                chord.to_ascii_lowercase()
            );
        }
    }

    #[test]
    fn validates_collisions_after_platform_modifier_resolution() {
        assert!(try_keybindings(ViewerKeybindingPlatform::Linux, "ctrl+p", "meta+p").is_ok());
        let search_files = "ctrl+p".parse().unwrap();
        assert_eq!(
            try_keybindings(ViewerKeybindingPlatform::MacOs, "ctrl+p", "meta+p"),
            Err(InvalidViewerKeybindings::Conflict {
                first: ViewerKeybindingAction::SearchFiles,
                second: ViewerKeybindingAction::SearchTextInAllFiles,
                binding: search_files,
            })
        );
    }

    #[test]
    fn matches_only_the_exact_native_keypress() {
        let keybindings = ViewerKeybindings::for_platform(ViewerKeybindingPlatform::MacOs);
        assert!(keybindings.matches_keypress(
            ViewerKeybindingAction::SearchFiles,
            "P",
            [ViewerKeyboardModifier::Meta].into_iter().collect(),
        ));
        assert!(
            !keybindings.matches_keypress(
                ViewerKeybindingAction::SearchFiles,
                "p",
                [
                    ViewerKeyboardModifier::Control,
                    ViewerKeyboardModifier::Meta,
                ]
                .into_iter()
                .collect(),
            )
        );
    }

    #[test]
    fn defaults_preserve_the_existing_global_search_shortcuts() {
        let keybindings = ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux);

        assert_eq!(
            keybindings[ViewerKeybindingAction::SearchFiles].to_string(),
            "ctrl+p"
        );
        assert_eq!(
            keybindings[ViewerKeybindingAction::SearchTextInAllFiles].to_string(),
            "ctrl+f"
        );
    }

    #[test]
    fn deserialization_rechecks_platform_collisions() {
        let value = json!({
            "platform": "mac_os",
            "search_files": "ctrl+p",
            "search_text_in_all_files": "meta+p",
            "toggle_files_sidebar": "ctrl+b",
            "toggle_commits_sidebar": "ctrl+alt+b",
        });

        assert!(serde_json::from_value::<ViewerKeybindings>(value).is_err());
    }

    #[test]
    fn older_serialized_keybindings_keep_the_default_push_chord() {
        let bindings = ViewerKeybindings::for_platform(ViewerKeybindingPlatform::Linux);
        let mut value = serde_json::to_value(bindings).unwrap();
        value.as_object_mut().unwrap().remove("push_diff");
        assert_eq!(
            serde_json::from_value::<ViewerKeybindings>(value).unwrap(),
            bindings
        );
    }
}
