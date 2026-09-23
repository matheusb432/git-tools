use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

use super::LoadingSpinner;

const BUTTON_CLASSES: &str = "control-button";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonType {
    #[default]
    Button,
    #[cfg(feature = "interactive-ui")]
    Submit,
}

impl ButtonType {
    const fn as_html_type(self) -> &'static str {
        match self {
            Self::Button => "button",
            #[cfg(feature = "interactive-ui")]
            Self::Submit => "submit",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonLayout {
    #[default]
    Inline,
    Content,
    FullWidthStart,
    Block,
}

impl ButtonLayout {
    const fn classes(self) -> &'static str {
        match self {
            Self::Inline => "control-button-layout-inline",
            Self::Content => "",
            Self::FullWidthStart => "control-button-layout-full-width-start w-full",
            Self::Block => "block",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonVariant {
    #[default]
    Primary,
    #[cfg(feature = "interactive-ui")]
    Secondary,
    #[cfg(feature = "interactive-ui")]
    Destructive,
    #[cfg(feature = "interactive-ui")]
    #[cfg_attr(
        not(feature = "component-preview"),
        allow(dead_code, reason = "reserved for the viewer push feature")
    )]
    Warning,
    Failure,
    Outline,
    Ghost,
    #[cfg(feature = "interactive-ui")]
    Accent,
    Toggle,
    Bare,
}

impl ButtonVariant {
    pub(crate) const fn classes(self) -> &'static str {
        match self {
            Self::Primary => "control-button-variant-primary",
            #[cfg(feature = "interactive-ui")]
            Self::Secondary => "control-button-variant-secondary",
            #[cfg(feature = "interactive-ui")]
            Self::Destructive => "control-button-variant-destructive",
            #[cfg(feature = "interactive-ui")]
            Self::Warning => "control-button-variant-warning",
            Self::Failure => "control-button-variant-failure",
            Self::Outline => "control-button-variant-outline",
            Self::Ghost => "control-button-variant-ghost",
            #[cfg(feature = "interactive-ui")]
            Self::Accent => "control-button-variant-accent",
            Self::Toggle => "control-button-variant-ghost control-button-variant-toggle",
            Self::Bare => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonSize {
    Content,
    #[cfg(feature = "component-preview")]
    Inline,
    Small,
    #[default]
    Medium,
    #[cfg(feature = "interactive-ui")]
    IconCompact,
    IconSmall,
    #[cfg(feature = "component-preview")]
    IconMedium,
    IconTouch,
}

impl ButtonSize {
    const fn classes(self) -> &'static str {
        match self {
            Self::Content => "",
            #[cfg(feature = "component-preview")]
            Self::Inline => "min-h-5 gap-1 px-1.5 py-px text-xs",
            Self::Small => "min-h-7 gap-1.5 px-2",
            Self::Medium => "h-9 gap-2 px-4",
            #[cfg(feature = "interactive-ui")]
            Self::IconCompact => "size-6 p-0",
            Self::IconSmall => "size-8 p-0",
            #[cfg(feature = "component-preview")]
            Self::IconMedium => "size-9 p-0",
            Self::IconTouch => "size-11 p-0",
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

pub(crate) fn button_classes(
    layout: ButtonLayout,
    variant: ButtonVariant,
    size: ButtonSize,
) -> String {
    format!(
        "{BUTTON_CLASSES} {} {} {}",
        layout.classes(),
        variant.classes(),
        size.classes()
    )
}

#[component]
pub(crate) fn Button(
    #[props(default)] button_type: ButtonType,
    #[props(default)] variant: ButtonVariant,
    #[props(default)] layout: ButtonLayout,
    #[props(default)] size: ButtonSize,
    #[props(default)] state: ButtonState,
    #[props(extends = GlobalAttributes)]
    #[props(extends = button)]
    attributes: Vec<Attribute>,
    onclick: Option<EventHandler<MouseEvent>>,
    icon: Option<Element>,
    children: Element,
) -> Element {
    let base = attributes!(button {
        class: button_classes(layout, variant, size),
        r#type: button_type.as_html_type(),
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
            if let Some(icon) = icon {
                span { class: "inline-flex size-3.5 shrink-0 items-center justify-center",
                    if state.is_loading() {
                        LoadingSpinner {}
                    } else {
                        {icon}
                    }
                }
            } else if state.is_loading() {
                LoadingSpinner {}
            }
            {children}
        }
    }
}

#[cfg(all(test, feature = "interactive-ui"))]
mod tests {
    use dioxus::prelude::*;

    use super::{Button, ButtonType};
    #[cfg(feature = "component-preview")]
    use super::{ButtonLayout, ButtonSize, ButtonVariant, button_classes};

    #[cfg(feature = "component-preview")]
    #[test]
    fn warning_variant_uses_the_warning_button_treatment() {
        assert!(
            button_classes(
                ButtonLayout::Inline,
                ButtonVariant::Warning,
                ButtonSize::Medium,
            )
            .contains("control-button-variant-warning")
        );
    }

    #[test]
    fn submit_type_uses_native_form_semantics() {
        let html = dioxus_ssr::render_element(rsx! {
            Button { button_type: ButtonType::Submit, "Save settings" }
        });

        assert!(html.contains("type=\"submit\""));
    }
}
