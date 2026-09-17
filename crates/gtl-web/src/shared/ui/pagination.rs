use dioxus::prelude::*;
use lucide_dioxus::{ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight};

use super::{Button, ButtonSize, ButtonState, ButtonVariant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PagePosition {
    number: usize,
    count: usize,
}

impl PagePosition {
    pub(crate) fn new(number: usize, count: usize) -> Self {
        let count = count.max(1);
        Self {
            number: number.clamp(1, count),
            count,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PageNavigation {
    First,
    Previous,
    Next,
    Last,
}

#[component]
pub(crate) fn Pagination(
    position: PagePosition,
    label: String,
    #[props(default)] disabled: bool,
    onselect: EventHandler<PageNavigation>,
    children: Element,
) -> Element {
    let progress = (position.number as u128 * 100) / position.count as u128;
    rsx! {
        footer { class: "control-pagination min-h-14 gap-3 px-3 sm:px-4",
            div {
                class: "control-pagination-progress",
                role: "progressbar",
                aria_label: "{label} page position",
                aria_valuemin: "1",
                aria_valuemax: "{position.count}",
                aria_valuenow: "{position.number}",
                span {
                    class: "block h-full bg-acc",
                    style: "width: {progress}%;",
                }
            }
            div { class: "min-w-0 text-xs text-ink-3 tabular-nums", {children} }
            nav {
                class: "control-pagination-navigation gap-1",
                aria_label: "{label} pages",
                for (navigation, title) in [(PageNavigation::First, "First page"), (PageNavigation::Previous, "Previous page")] {
                    Button {
                        key: "{title}",
                        size: ButtonSize::IconTouch,
                        variant: ButtonVariant::Ghost,
                        state: if disabled || position.number == 1 { ButtonState::Disabled } else { ButtonState::Enabled },
                        aria_label: title,
                        title,
                        onclick: move |_| onselect.call(navigation),
                        span { aria_hidden: "true",
                            if navigation == PageNavigation::First {
                                ChevronsLeft { size: 16 }
                            } else {
                                ChevronLeft { size: 16 }
                            }
                        }
                    }
                }
                output {
                    class: "min-w-16 px-2 text-center text-xs text-ink tabular-nums",
                    aria_label: "Page {position.number} of {position.count}",
                    "{position.number:02} / {position.count:02}"
                }
                for (navigation, title) in [(PageNavigation::Next, "Next page"), (PageNavigation::Last, "Last page")] {
                    Button {
                        key: "{title}",
                        size: ButtonSize::IconTouch,
                        variant: ButtonVariant::Ghost,
                        state: if disabled || position.number == position.count { ButtonState::Disabled } else { ButtonState::Enabled },
                        aria_label: title,
                        title,
                        onclick: move |_| onselect.call(navigation),
                        span { aria_hidden: "true",
                            if navigation == PageNavigation::Last {
                                ChevronsRight { size: 16 }
                            } else {
                                ChevronRight { size: 16 }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_clamps_to_the_available_pages() {
        assert_eq!(
            PagePosition::new(0, 0),
            PagePosition {
                number: 1,
                count: 1
            }
        );
        assert_eq!(
            PagePosition::new(3, 2),
            PagePosition {
                number: 2,
                count: 2
            }
        );
    }
}
