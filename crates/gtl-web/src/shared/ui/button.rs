use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

use super::LoadingSpinner;

const BUTTON_CLASSES: &str = "cursor-pointer items-center whitespace-nowrap rounded-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc disabled:cursor-not-allowed disabled:opacity-50";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonLayout {
    #[default]
    Inline,
    #[cfg(feature = "artifact")]
    Content,
    FullWidthStart,
    Block,
}

impl ButtonLayout {
    const fn classes(self) -> &'static str {
        match self {
            Self::Inline => "inline-flex shrink-0 justify-center",
            #[cfg(feature = "artifact")]
            Self::Content => "",
            Self::FullWidthStart => "flex w-full justify-start",
            Self::Block => "block",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Pressed,
    #[cfg(feature = "interactive-ui")]
    Destructive,
    Failure,
    Outline,
    Ghost,
    Bare,
}

impl ButtonVariant {
    pub(crate) const fn classes(self) -> &'static str {
        match self {
            Self::Primary => {
                "border border-acc bg-acc text-bg hover:bg-acc-2 active:border-acc-2 active:bg-acc-2"
            }
            Self::Secondary => {
                "border border-acc-line bg-acc-soft text-acc hover:border-acc hover:text-acc-2 active:border-acc active:bg-acc active:text-bg"
            }
            Self::Pressed => {
                "border border-acc-line bg-acc-soft text-ink hover:border-acc active:border-acc active:bg-acc active:text-bg"
            }
            #[cfg(feature = "interactive-ui")]
            Self::Destructive => {
                "border border-del-line bg-del-bg text-del hover:border-del hover:bg-del hover:text-bg active:border-del active:bg-del active:text-bg"
            }
            Self::Failure => {
                "border border-del bg-del text-bg hover:border-del-line hover:bg-del-bg hover:text-del active:border-del active:bg-del active:text-bg"
            }
            Self::Outline => {
                "border border-line-2 bg-surface-2 text-ink-2 hover:border-acc-line hover:text-ink active:border-line-2 active:bg-line active:text-ink"
            }
            Self::Ghost => {
                "border border-transparent bg-transparent text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink active:border-line-2 active:bg-line active:text-ink"
            }
            Self::Bare => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonSize {
    Content,
    Inline,
    Small,
    #[default]
    Medium,
    #[cfg(feature = "interactive-ui")]
    IconCompact,
    IconSmall,
    #[cfg(feature = "interactive-ui")]
    IconMedium,
}

impl ButtonSize {
    const fn classes(self) -> &'static str {
        match self {
            Self::Content => "",
            Self::Inline => "min-h-5 gap-1 px-1.5 py-px text-xs",
            Self::Small => "min-h-7 gap-1.5 px-2",
            Self::Medium => "h-9 gap-2 px-4",
            #[cfg(feature = "interactive-ui")]
            Self::IconCompact => "size-6 p-0",
            Self::IconSmall => "size-8 p-0",
            #[cfg(feature = "interactive-ui")]
            Self::IconMedium => "size-9 p-0",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonState {
    #[default]
    Enabled,
    Disabled,
    #[cfg(feature = "interactive-ui")]
    Loading,
}

impl ButtonState {
    const fn is_disabled(self) -> bool {
        match self {
            Self::Enabled => false,
            Self::Disabled => true,
            #[cfg(feature = "interactive-ui")]
            Self::Loading => true,
        }
    }

    const fn is_loading(self) -> bool {
        match self {
            Self::Enabled => false,
            Self::Disabled => false,
            #[cfg(feature = "interactive-ui")]
            Self::Loading => true,
        }
    }
}

#[component]
pub(crate) fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] layout: ButtonLayout,
    #[props(default)] size: ButtonSize,
    #[props(default)] state: ButtonState,
    #[props(extends = GlobalAttributes)]
    #[props(extends = button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let base = attributes!(button {
        class: format!(
            "{BUTTON_CLASSES} {} {} {}",
            layout.classes(),
            variant.classes(),
            size.classes()
        ),
        r#type: "button",
        disabled: state.is_disabled(),
        aria_busy: state.is_loading().then_some("true"),
    });
    let attributes = merge_attributes(vec![attributes, base]);

    rsx! {
        button {
            onclick: move |event| {
                if let Some(handler) = &onclick {
                    handler.call(event);
                }
            },
            ..attributes,
            if state.is_loading() {
                LoadingSpinner {}
            }
            {children}
        }
    }
}
