use dioxus::prelude::*;
use dioxus_primitives::{dioxus_attributes::attributes, merge_attributes};

use super::LoadingSpinner;

const BUTTON_CLASSES: &str = "control-button";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ButtonType {
    #[default]
    Button,
    Submit,
}

impl ButtonType {
    const fn as_html_type(self) -> &'static str {
        match self {
            Self::Button => "button",
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
    Secondary,
    Destructive,
    #[cfg_attr(
        not(feature = "component-preview"),
        allow(dead_code, reason = "reserved for the viewer push feature")
    )]
    Warning,
    Failure,
    Outline,
    Ghost,
    Accent,
    Toggle,
    Bare,
}

impl ButtonVariant {
    pub(crate) const fn classes(self) -> &'static str {
        match self {
            Self::Primary => "control-button-variant-primary",
            Self::Secondary => "control-button-variant-secondary",
            Self::Destructive => "control-button-variant-destructive",
            Self::Warning => "control-button-variant-warning",
            Self::Failure => "control-button-variant-failure",
            Self::Outline => "control-button-variant-outline",
            Self::Ghost => "control-button-variant-ghost",
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
    Loading,
    /// Busy behind other work; shows a clock instead of the spinner.
    Waiting,
}

impl ButtonState {
    const fn is_disabled(self) -> bool {
        match self {
            Self::Enabled => false,
            Self::Disabled | Self::Loading | Self::Waiting => true,
        }
    }

    const fn is_busy(self) -> bool {
        match self {
            Self::Enabled | Self::Disabled => false,
            Self::Loading | Self::Waiting => true,
        }
    }

    fn indicator(self) -> Option<Element> {
        match self {
            Self::Enabled | Self::Disabled => None,
            Self::Loading => Some(rsx! {
                LoadingSpinner {}
            }),
            Self::Waiting => Some(rsx! {
                lucide_dioxus::Clock { size: 14 }
            }),
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
        aria_busy: state.is_busy().then_some("true"),
    });
    let attributes = merge_attributes(vec![attributes, base]);
    let indicator = state.indicator();

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
                    {indicator.unwrap_or(icon)}
                }
            } else if let Some(indicator) = indicator {
                {indicator}
            }
            {children}
        }
    }
}

#[cfg(test)]
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
