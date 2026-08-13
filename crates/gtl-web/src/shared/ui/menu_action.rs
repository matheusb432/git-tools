use dioxus::prelude::*;

pub(crate) const MENU_ACTION_HOST_CLASSES: &str = "group/action flex min-h-12 w-full cursor-pointer items-center justify-start gap-2 whitespace-nowrap rounded-sm border border-transparent bg-transparent px-2 py-1.5 text-left text-ink-2 hover:border-line-2 hover:bg-surface-2 hover:text-ink active:border-line-2 active:bg-line active:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-acc disabled:cursor-not-allowed disabled:opacity-50 aria-[current=page]:border-acc-line aria-[current=page]:bg-acc-soft aria-[current=page]:text-ink aria-[current=page]:hover:border-acc aria-[current=page]:active:border-acc aria-[current=page]:active:bg-acc aria-[current=page]:active:text-bg";

#[component]
pub(crate) fn MenuActionContent(
    icon: Element,
    label: String,
    description: String,
    children: Option<Element>,
) -> Element {
    rsx! {
        span {
            class: "inline-flex size-7 flex-none items-center justify-center rounded-sm border border-line bg-sunk text-ink-2 group-hover/action:border-acc-line group-hover/action:text-acc",
            aria_hidden: "true",
            {icon}
        }
        span { class: "min-w-0 flex-1",
            strong { class: "block text-xs font-semibold text-inherit", "{label}" }
            small { class: "mt-0.5 block truncate text-xs text-ink-3", "{description}" }
        }
        {children}
    }
}
