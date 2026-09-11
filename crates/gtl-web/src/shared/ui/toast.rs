use std::{collections::VecDeque, time::Duration};

use dioxus::prelude::*;
use gtl_web_contracts::test_ids;
use lucide_dioxus::{CircleCheck, CircleX, Info, TriangleAlert, X};

use super::{Button, ButtonSize, ButtonVariant};

const MAX_TOASTS: usize = 6;
const TOAST_LIFETIME: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToastId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Ok,
    Warn,
    #[allow(dead_code, reason = "the atom API reserves an informational severity")]
    Info,
    Error,
}

impl ToastKind {
    const fn role(self) -> &'static str {
        match self {
            Self::Ok | Self::Info => "status",
            Self::Warn | Self::Error => "alert",
        }
    }

    const fn aria_live(self) -> &'static str {
        match self {
            Self::Ok | Self::Info => "polite",
            Self::Warn | Self::Error => "assertive",
        }
    }

    const fn accent_classes(self) -> &'static str {
        match self {
            Self::Ok => "bg-add",
            Self::Warn => "bg-warn",
            Self::Info => "bg-acc",
            Self::Error => "bg-del",
        }
    }

    const fn icon_classes(self) -> &'static str {
        match self {
            Self::Ok => "border-add-line bg-add-bg text-add",
            Self::Warn => "border-warn-line bg-warn-bg text-warn",
            Self::Info => "border-acc-line bg-acc-soft text-acc",
            Self::Error => "border-del-line bg-del-bg text-del",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToastMessage {
    id: ToastId,
    kind: ToastKind,
    message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ToastQueue {
    entries: VecDeque<ToastMessage>,
    next_id: u64,
}

impl ToastQueue {
    fn enqueue(&mut self, kind: ToastKind, message: impl Into<String>) -> ToastId {
        self.next_id = self.next_id.wrapping_add(1);
        let id = ToastId(self.next_id);
        if self.entries.len() == MAX_TOASTS {
            self.entries.remove(1);
        }
        self.entries.push_back(ToastMessage {
            id,
            kind,
            message: message.into(),
        });
        id
    }

    fn dismiss(&mut self, id: ToastId) -> bool {
        let Some(index) = self.entries.iter().position(|toast| toast.id == id) else {
            return false;
        };
        self.entries.remove(index);
        true
    }

    fn expire(&mut self, id: ToastId) -> bool {
        if self.active_id() != Some(id) {
            return false;
        }
        self.entries.pop_front();
        true
    }

    fn active_id(&self) -> Option<ToastId> {
        self.entries.front().map(|toast| toast.id)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ToastHandle {
    queue: Signal<ToastQueue>,
}

impl ToastHandle {
    pub(crate) fn ok(mut self, message: impl Into<String>) {
        self.queue.write().enqueue(ToastKind::Ok, message);
    }

    pub(crate) fn warn(mut self, message: impl Into<String>) {
        self.queue.write().enqueue(ToastKind::Warn, message);
    }

    #[allow(dead_code, reason = "the atom API includes every supported severity")]
    pub(crate) fn info(mut self, message: impl Into<String>) {
        self.queue.write().enqueue(ToastKind::Info, message);
    }

    pub(crate) fn error(mut self, message: impl Into<String>) {
        self.queue.write().enqueue(ToastKind::Error, message);
    }
}

pub(crate) fn use_toast() -> ToastHandle {
    use_context()
}

#[component]
pub(crate) fn ToastHost(children: Element) -> Element {
    let mut queue = use_signal(ToastQueue::default);
    use_context_provider(|| ToastHandle { queue });

    let mut expiry = use_action(move |id: ToastId| async move {
        dioxus_sdk_time::sleep(TOAST_LIFETIME).await;
        queue.write().expire(id);
        Ok::<(), std::convert::Infallible>(())
    });
    let active_id = queue.read().active_id();
    use_effect(use_reactive(
        (&active_id,),
        move |(active_id,)| match active_id {
            Some(id) => {
                expiry.call(id);
            }
            None => expiry.reset(),
        },
    ));

    rsx! {
        {children}
        ToastViewport {
            queue: queue(),
            ondismiss: move |id| {
                queue.write().dismiss(id);
            },
        }
    }
}

#[component]
fn ToastViewport(queue: ToastQueue, ondismiss: EventHandler<ToastId>) -> Element {
    let Some(toast) = queue.entries.front().cloned() else {
        return rsx! {
            div {
                class: "toast-viewport mx-auto w-auto",
                aria_label: "Notifications",
                "data-testid": test_ids::TOAST_VIEWPORT.value(),
            }
        };
    };
    let shows_ledger = queue.entries.len() > 1;
    let card_classes = "toast-card";

    rsx! {
        div {
            class: "toast-viewport mx-auto w-auto",
            aria_label: "Notifications",
            "data-testid": test_ids::TOAST_VIEWPORT.value(),
            if shows_ledger {
                div {
                    class: "flex h-1 gap-1 px-4",
                    aria_hidden: "true",
                    "data-testid": test_ids::TOAST_LEDGER.value(),
                    for slot in 0..MAX_TOASTS {
                        {
                            let (state, classes) = match queue.entries.get(slot) {
                                Some(entry) if slot == 0 => ("current", entry.kind.accent_classes()),
                                Some(_) => ("waiting", "bg-ink-2"),
                                None => ("empty", "bg-line"),
                            };
                            rsx! {
                                span {
                                    key: "{slot}",
                                    class: "h-1 min-w-0 flex-1 rounded-full {classes}",
                                    "data-toast-ledger-slot": "",
                                    "data-state": state,
                                }
                            }
                        }
                    }
                }
            }
            div {
                key: "{toast.id.0}",
                class: card_classes,
                "data-ledger": shows_ledger.to_string(),
                role: toast.kind.role(),
                aria_live: toast.kind.aria_live(),
                aria_atomic: "true",
                "data-testid": test_ids::TOAST.value(),
                div {
                    class: "toast-accent w-1 {toast.kind.accent_classes()}",
                    aria_hidden: "true",
                }
                div { class: "toast-content min-h-20 gap-3 py-3 pr-2 pl-5",
                    span {
                        class: "toast-icon size-12 {toast.kind.icon_classes()}",
                        aria_hidden: "true",
                        ToastIcon { kind: toast.kind }
                    }
                    p { class: "min-w-0 flex-1 wrap-anywhere text-base leading-5 text-ink",
                        "{toast.message}"
                    }
                    Button {
                        class: "size-11 shrink-0",
                        size: ButtonSize::Content,
                        variant: ButtonVariant::Ghost,
                        aria_label: "Dismiss notification",
                        title: "Dismiss notification",
                        "data-testid": test_ids::TOAST_DISMISS.value(),
                        onclick: move |_| ondismiss.call(toast.id),
                        span { aria_hidden: "true",
                            X { size: 24 }
                        }
                    }
                }
                div {
                    class: "toast-expiry h-0.5 {toast.kind.accent_classes()}",
                    aria_hidden: "true",
                }
            }
        }
    }
}

#[component]
fn ToastIcon(kind: ToastKind) -> Element {
    match kind {
        ToastKind::Ok => rsx! {
            CircleCheck { size: 28 }
        },
        ToastKind::Warn => rsx! {
            TriangleAlert { size: 28 }
        },
        ToastKind::Info => rsx! {
            Info { size: 28 }
        },
        ToastKind::Error => rsx! {
            CircleX { size: 28 }
        },
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use gtl_web_contracts::test_ids;

    use super::{
        MAX_TOASTS, ToastHandle, ToastHost, ToastKind, ToastQueue, ToastViewport,
        ToastViewportProps,
    };

    fn messages(queue: &ToastQueue) -> Vec<&str> {
        queue
            .entries
            .iter()
            .map(|toast| toast.message.as_str())
            .collect()
    }

    fn render(queue: ToastQueue) -> String {
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ToastViewportProps {
            queue,
            ondismiss: EventHandler::new(|_| {}),
        });
        let mut viewport = VirtualDom::new_with_props(ToastViewport, props);
        viewport.rebuild_in_place();
        dioxus_ssr::render(&viewport)
    }

    #[test]
    fn queue_preserves_fifo_order() {
        let mut queue = ToastQueue::default();

        queue.enqueue(ToastKind::Ok, "first");
        queue.enqueue(ToastKind::Warn, "second");
        queue.enqueue(ToastKind::Info, "third");

        assert_eq!(messages(&queue), ["first", "second", "third"]);
    }

    #[test]
    fn full_queue_keeps_the_visible_toast_and_latest_waiting_toasts() {
        let mut queue = ToastQueue::default();
        for number in 1..=MAX_TOASTS + 2 {
            queue.enqueue(ToastKind::Info, number.to_string());
        }

        assert_eq!(queue.entries.len(), MAX_TOASTS);
        assert_eq!(messages(&queue), ["1", "4", "5", "6", "7", "8"]);
    }

    #[test]
    fn stale_expiry_cannot_dismiss_the_next_toast() {
        let mut queue = ToastQueue::default();
        let first = queue.enqueue(ToastKind::Ok, "first");
        let second = queue.enqueue(ToastKind::Error, "second");

        assert!(queue.dismiss(first));
        assert!(!queue.expire(first));
        assert_eq!(queue.active_id(), Some(second));
        assert!(queue.expire(second));
        assert!(queue.entries.is_empty());
    }

    #[test]
    fn explicit_dismissal_removes_only_the_selected_toast() {
        let mut queue = ToastQueue::default();
        let first = queue.enqueue(ToastKind::Ok, "first");
        let second = queue.enqueue(ToastKind::Warn, "second");

        assert!(queue.dismiss(first));
        assert_eq!(queue.active_id(), Some(second));
        assert_eq!(messages(&queue), ["second"]);
    }

    #[test]
    fn single_toast_is_accessible_without_a_ledger() {
        let mut queue = ToastQueue::default();
        queue.enqueue(ToastKind::Ok, "Live view deleted.");

        let html = render(queue);

        assert!(html.contains(&format!(r#"data-testid="{}""#, test_ids::TOAST.value())));
        assert!(html.contains(r#"role="status""#));
        assert!(html.contains(r#"aria-live="polite""#));
        assert!(html.contains(r#"aria-atomic="true""#));
        assert!(html.contains("Live view deleted."));
        assert!(html.contains(&format!(
            r#"data-testid="{}""#,
            test_ids::TOAST_DISMISS.value()
        )));
        assert!(!html.contains(&format!(
            r#"data-testid="{}""#,
            test_ids::TOAST_LEDGER.value()
        )));
    }

    #[test]
    fn waiting_toast_reveals_the_six_slot_event_ledger() {
        let mut queue = ToastQueue::default();
        queue.enqueue(ToastKind::Warn, "Current");
        queue.enqueue(ToastKind::Info, "Waiting");

        let html = render(queue);

        assert!(html.contains(&format!(
            r#"data-testid="{}""#,
            test_ids::TOAST_LEDGER.value()
        )));
        assert_eq!(html.matches("data-toast-ledger-slot=").count(), MAX_TOASTS);
        assert_eq!(html.matches(r#"data-state="current""#).count(), 1);
        assert_eq!(html.matches(r#"data-state="waiting""#).count(), 1);
        assert_eq!(html.matches(r#"data-state="empty""#).count(), 4);
        assert!(html.contains(r#"role="alert""#));
        assert!(html.contains(r#"aria-live="assertive""#));
        assert!(!html.contains("Waiting"));
    }

    #[test]
    fn host_renders_one_global_viewport_after_route_content() {
        let html = dioxus_ssr::render_element(rsx! {
            ToastHost {
                main { "Route content" }
            }
        });
        let viewport = format!(r#"data-testid="{}""#, test_ids::TOAST_VIEWPORT.value());
        assert!(html.contains("Route content"));
        assert!(html.contains(&viewport));
        assert!(html.find("Route content") < html.find(&viewport));
        assert_eq!(html.matches(&viewport).count(), 1);
    }

    #[allow(dead_code)]
    fn public_atom_api_accepts_into_string(toast: ToastHandle) {
        toast.ok("ok");
        toast.warn(String::from("warn"));
        toast.info("info");
        toast.error(String::from("error"));
    }
}
