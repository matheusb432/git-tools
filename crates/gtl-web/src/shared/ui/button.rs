use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

const BUTTON_CLASSES: &str = "inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap rounded-sm border font-semibold focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc active:translate-y-px motion-safe:transition-transform disabled:cursor-not-allowed disabled:opacity-50 disabled:active:translate-y-0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Destructive,
    Outline,
    Ghost,
}

impl ButtonVariant {
    const fn classes(self) -> &'static str {
        match self {
            Self::Primary => "border-acc bg-acc text-bg hover:bg-acc-2",
            Self::Secondary => {
                "border-acc-line bg-acc-soft text-acc hover:border-acc hover:bg-acc hover:text-bg"
            }
            Self::Destructive => {
                "border-del-line bg-del-bg text-del hover:border-del hover:bg-del hover:text-bg"
            }
            Self::Outline => {
                "border-line-2 bg-surface text-ink hover:border-acc-line hover:bg-surface-2"
            }
            Self::Ghost => {
                "border-transparent bg-transparent text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonSize {
    Small,
    #[default]
    Medium,
    Large,
    IconSmall,
    IconMedium,
    IconLarge,
}

impl ButtonSize {
    const fn classes(self) -> &'static str {
        match self {
            Self::Small => "h-7 gap-1 px-2 text-xs",
            Self::Medium => "h-9 gap-2 px-4 text-sm",
            Self::Large => "h-11 gap-2 px-5 text-sm",
            Self::IconSmall => "size-7 p-0 text-xs",
            Self::IconMedium => "size-9 p-0 text-sm",
            Self::IconLarge => "size-11 p-0 text-base",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonState {
    #[default]
    Enabled,
    Disabled,
    Loading,
}

impl ButtonState {
    const fn is_disabled(self) -> bool {
        matches!(self, Self::Disabled | Self::Loading)
    }

    const fn is_loading(self) -> bool {
        matches!(self, Self::Loading)
    }
}

#[component]
pub(crate) fn Button(
    #[props(default)] variant: ButtonVariant,
    #[props(default)] size: ButtonSize,
    #[props(default)] state: ButtonState,
    #[props(extends = GlobalAttributes)]
    #[props(extends = button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    children: Element,
) -> Element {
    let base = attributes!(button {
        class: format!("{BUTTON_CLASSES} {} {}", variant.classes(), size.classes()),
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
                span {
                    class: "size-3.5 shrink-0 animate-spin rounded-full border-2 border-current border-r-transparent motion-reduce:animate-none",
                    aria_hidden: "true",
                }
            }
            {children}
        }
    }
}
