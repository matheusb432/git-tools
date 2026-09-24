use std::{collections::VecDeque, convert::Infallible, time::Duration};

use dioxus::prelude::*;
use gtl_web_contracts::test_ids;
use lucide_dioxus::{CircleCheck, CircleX, Info, TriangleAlert, X};

use super::{Button, ButtonSize, ButtonVariant, animation::computed_animation_duration};

const TOAST_WAITING_COUNT_MAX: usize = 5;
const TOAST_STACK_DEPTH_MAX: usize = 2;
const TOAST_LIFETIME: Duration = Duration::from_secs(4);
const TOAST_LIFETIME_ERROR: Duration = Duration::from_secs(6);
const TOAST_COUNTDOWN_TICK: Duration = Duration::from_millis(100);
const TOAST_EXIT_DURATION_FALLBACK: Duration = Duration::from_millis(160);
const TOAST_LEAVING_SELECTOR: &str = ".toast-card[data-state=\"leaving\"]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToastId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToastKind {
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

    const fn lifetime(self) -> Duration {
        match self {
            Self::Error => TOAST_LIFETIME_ERROR,
            Self::Ok | Self::Warn | Self::Info => TOAST_LIFETIME,
        }
    }

    const fn icon_classes(self) -> &'static str {
        match self {
            Self::Ok => "text-add",
            Self::Warn => "text-warn",
            Self::Info => "text-acc",
            Self::Error => "text-del",
        }
    }

    const fn timer_classes(self) -> &'static str {
        match self {
            Self::Ok => "bg-add",
            Self::Warn => "bg-warn",
            Self::Info => "bg-acc",
            Self::Error => "bg-del",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastEntrance {
    /// The toast appeared while no toast was visible.
    Rise,
    /// The toast moved forward from the waiting stack.
    Promote,
}

impl ToastEntrance {
    const fn value(self) -> &'static str {
        match self {
            Self::Rise => "rise",
            Self::Promote => "promote",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastPhase {
    Shown(ToastEntrance),
    /// The exit animation is playing; the toast leaves the queue once it finishes.
    Leaving,
}

impl ToastPhase {
    const fn is_shown(self) -> bool {
        matches!(self, Self::Shown(_))
    }

    const fn state(self) -> &'static str {
        match self {
            Self::Shown(_) => "shown",
            Self::Leaving => "leaving",
        }
    }

    const fn entrance(self) -> Option<&'static str> {
        match self {
            Self::Shown(entrance) => Some(entrance.value()),
            Self::Leaving => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ToastMessage {
    id: ToastId,
    kind: ToastKind,
    message: String,
    /// Secondary verbatim text, such as tool output, shown below the message.
    detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VisibleToast {
    message: ToastMessage,
    phase: ToastPhase,
}

/// FIFO notifications: one visible toast and the toasts waiting behind it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ToastQueue {
    visible: Option<VisibleToast>,
    waiting: VecDeque<ToastMessage>,
    next_id: u64,
}

impl ToastQueue {
    fn enqueue(
        &mut self,
        kind: ToastKind,
        message: impl Into<String>,
        detail: Option<String>,
    ) -> ToastId {
        self.next_id = self.next_id.wrapping_add(1);
        let id = ToastId(self.next_id);
        let message = ToastMessage {
            id,
            kind,
            message: message.into(),
            detail,
        };
        if self.visible.is_none() {
            self.visible = Some(VisibleToast {
                message,
                phase: ToastPhase::Shown(ToastEntrance::Rise),
            });
            return id;
        }
        if self.waiting.len() == TOAST_WAITING_COUNT_MAX {
            self.waiting.pop_front();
        }
        self.waiting.push_back(message);
        id
    }

    /// Starts the exit of the visible toast; stale and repeated requests are ignored.
    fn begin_leave(&mut self, id: ToastId) -> bool {
        let Some(visible) = self
            .visible
            .as_mut()
            .filter(|visible| visible.message.id == id && visible.phase.is_shown())
        else {
            return false;
        };
        visible.phase = ToastPhase::Leaving;
        true
    }

    /// Removes the leaving toast and promotes the oldest waiting toast.
    fn finish_leave(&mut self, id: ToastId) -> bool {
        let is_leaving = self.visible.as_ref().is_some_and(|visible| {
            visible.message.id == id && visible.phase == ToastPhase::Leaving
        });
        if is_leaving {
            self.visible = self.waiting.pop_front().map(|message| VisibleToast {
                message,
                phase: ToastPhase::Shown(ToastEntrance::Promote),
            });
        }
        is_leaving
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ToastHandle {
    queue: Signal<ToastQueue>,
}

impl ToastHandle {
    pub(crate) fn ok(self, message: impl Into<String>) {
        self.show(ToastKind::Ok, message, None);
    }

    pub(crate) fn warn(self, message: impl Into<String>) {
        self.show(ToastKind::Warn, message, None);
    }

    #[allow(dead_code, reason = "the atom API includes every supported severity")]
    pub(crate) fn info(self, message: impl Into<String>) {
        self.show(ToastKind::Info, message, None);
    }

    pub(crate) fn error(self, message: impl Into<String>) {
        self.show(ToastKind::Error, message, None);
    }

    /// Enqueues a toast whose `detail` appears as secondary text below the message.
    pub(crate) fn show(
        mut self,
        kind: ToastKind,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        self.queue.write().enqueue(kind, message, detail);
    }
}

pub(crate) fn use_toast() -> ToastHandle {
    use_context()
}

#[component]
pub(crate) fn ToastHost(children: Element) -> Element {
    let mut queue = use_signal(ToastQueue::default);
    use_context_provider(|| ToastHandle { queue });
    let mut hovered = use_signal(|| false);
    let mut focused = use_signal(|| false);
    let mut countdown_toast_id = use_signal(|| None::<ToastId>);
    let mut countdown_remaining = use_signal(|| Duration::ZERO);

    let mut exit = use_action(move |id: ToastId| async move {
        let exit_duration = leaving_toast_exit_duration().await;
        dioxus_sdk_time::sleep(exit_duration).await;
        queue.write().finish_leave(id);
        Ok::<(), Infallible>(())
    });
    let leave = use_callback(move |id: ToastId| {
        if queue.write().begin_leave(id) {
            exit.call(id);
        }
    });
    // Pausing cancels this action; resuming restarts it from the remaining time.
    let mut countdown = use_action(move |id: ToastId| async move {
        while !countdown_remaining.peek().is_zero() {
            dioxus_sdk_time::sleep(TOAST_COUNTDOWN_TICK).await;
            let remaining = countdown_remaining
                .peek()
                .saturating_sub(TOAST_COUNTDOWN_TICK);
            countdown_remaining.set(remaining);
        }
        leave.call(id);
        Ok::<(), Infallible>(())
    });

    let queue_snapshot = queue();
    let visible = queue_snapshot.visible.as_ref();
    let visible_id = visible.map(|toast| toast.message.id);
    let visible_lifetime = visible.map_or(Duration::ZERO, |toast| toast.message.kind.lifetime());
    let paused = hovered() || focused();
    let countdown_running = !paused && visible.is_some_and(|toast| toast.phase.is_shown());
    use_effect(use_reactive(
        (&visible_id, &visible_lifetime, &countdown_running),
        move |(visible_id, visible_lifetime, countdown_running)| {
            if *countdown_toast_id.peek() != visible_id {
                countdown_toast_id.set(visible_id);
                countdown_remaining.set(visible_lifetime);
                // Removing the focused or hovered card does not report focusout or mouseleave.
                if *focused.peek() {
                    focused.set(false);
                }
                if visible_id.is_none() && *hovered.peek() {
                    hovered.set(false);
                }
            }
            match visible_id {
                Some(id) if countdown_running => {
                    countdown.call(id);
                }
                _ => countdown.cancel(),
            }
        },
    ));

    rsx! {
        {children}
        ToastViewport {
            queue: queue_snapshot.clone(),
            paused,
            ondismiss: leave,
            onhoverchange: move |value| hovered.set(value),
            onfocuschange: move |value| focused.set(value),
        }
    }
}

async fn leaving_toast_exit_duration() -> Duration {
    dioxus_sdk_time::sleep(Duration::ZERO).await;
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| {
            document
                .query_selector(TOAST_LEAVING_SELECTOR)
                .ok()
                .flatten()
        })
        .and_then(|card| computed_animation_duration(&card))
        .unwrap_or(TOAST_EXIT_DURATION_FALLBACK)
}

#[component]
fn ToastViewport(
    queue: ToastQueue,
    paused: bool,
    ondismiss: EventHandler<ToastId>,
    onhoverchange: EventHandler<bool>,
    onfocuschange: EventHandler<bool>,
) -> Element {
    let waiting_count = queue.waiting.len();

    rsx! {
        div { class: "toast-viewport", aria_label: "Notifications",
            if let Some(visible) = queue.visible {
                div {
                    class: "toast-stack",
                    onmouseenter: move |_| onhoverchange.call(true),
                    onmouseleave: move |_| onhoverchange.call(false),
                    onfocusin: move |_| onfocuschange.call(true),
                    onfocusout: move |_| onfocuschange.call(false),
                    for depth in 1..=waiting_count.min(TOAST_STACK_DEPTH_MAX) {
                        div {
                            key: "{depth}",
                            class: "toast-stack-layer",
                            "data-depth": "{depth}",
                            aria_hidden: "true",
                        }
                    }
                    ToastCard {
                        key: "{visible.message.id.0}",
                        visible,
                        waiting_count,
                        paused,
                        ondismiss,
                    }
                }
            }
        }
    }
}

#[component]
fn ToastCard(
    visible: VisibleToast,
    waiting_count: usize,
    paused: bool,
    ondismiss: EventHandler<ToastId>,
) -> Element {
    let VisibleToast {
        message: toast,
        phase,
    } = visible;
    let toast_id = toast.id;
    let lifetime_ms = toast.kind.lifetime().as_millis();
    let waiting_label = if waiting_count == 1 {
        String::from("1 more notification")
    } else {
        format!("{waiting_count} more notifications")
    };

    rsx! {
        div {
            class: "toast-card",
            "data-state": phase.state(),
            "data-entrance": phase.entrance(),
            "data-paused": paused.to_string(),
            role: toast.kind.role(),
            aria_live: toast.kind.aria_live(),
            aria_atomic: "true",
            "data-testid": test_ids::TOAST.value(),
            span {
                class: "toast-icon {toast.kind.icon_classes()}",
                aria_hidden: "true",
                ToastIcon { kind: toast.kind }
            }
            div { class: "toast-body",
                p { class: "toast-message", "{toast.message}" }
                if let Some(detail) = &toast.detail {
                    p { class: "toast-detail", "{detail}" }
                }
            }
            if waiting_count > 0 {
                span {
                    class: "toast-waiting-count",
                    title: waiting_label,
                    aria_hidden: "true",
                    "+{waiting_count}"
                }
            }
            Button {
                class: "shrink-0 text-ink-3",
                size: ButtonSize::IconSmall,
                variant: ButtonVariant::Ghost,
                aria_label: "Dismiss notification",
                title: "Dismiss notification",
                "data-testid": test_ids::TOAST_DISMISS.value(),
                onclick: move |_| ondismiss.call(toast_id),
                span { aria_hidden: "true",
                    X { size: 14 }
                }
            }
            div {
                class: "toast-timer {toast.kind.timer_classes()}",
                style: "animation-duration: {lifetime_ms}ms",
                aria_hidden: "true",
            }
        }
    }
}

#[component]
fn ToastIcon(kind: ToastKind) -> Element {
    match kind {
        ToastKind::Ok => rsx! {
            CircleCheck { size: 16 }
        },
        ToastKind::Warn => rsx! {
            TriangleAlert { size: 16 }
        },
        ToastKind::Info => rsx! {
            Info { size: 16 }
        },
        ToastKind::Error => rsx! {
            CircleX { size: 16 }
        },
    }
}

#[cfg(test)]
mod tests {
    use dioxus::prelude::*;
    use gtl_web_contracts::test_ids;

    use super::{
        TOAST_WAITING_COUNT_MAX, ToastEntrance, ToastHandle, ToastHost, ToastKind, ToastPhase,
        ToastQueue, ToastViewport, ToastViewportProps,
    };

    fn messages(queue: &ToastQueue) -> Vec<&str> {
        queue
            .visible
            .iter()
            .map(|visible| &visible.message)
            .chain(&queue.waiting)
            .map(|toast| toast.message.as_str())
            .collect()
    }

    fn visible_phase(queue: &ToastQueue) -> Option<ToastPhase> {
        queue.visible.as_ref().map(|visible| visible.phase)
    }

    fn render(queue: ToastQueue) -> String {
        let event_handler_owner = VirtualDom::new(VNode::empty);
        let props = event_handler_owner.in_scope(ScopeId::ROOT, || ToastViewportProps {
            queue,
            paused: false,
            ondismiss: EventHandler::new(|_| {}),
            onhoverchange: EventHandler::new(|_| {}),
            onfocuschange: EventHandler::new(|_| {}),
        });
        let mut viewport = VirtualDom::new_with_props(ToastViewport, props);
        viewport.rebuild_in_place();
        dioxus_ssr::render(&viewport)
    }

    #[test]
    fn queue_preserves_fifo_order() {
        let mut queue = ToastQueue::default();

        queue.enqueue(ToastKind::Ok, "first", None);
        queue.enqueue(ToastKind::Warn, "second", None);
        queue.enqueue(ToastKind::Info, "third", None);

        assert_eq!(messages(&queue), ["first", "second", "third"]);
    }

    #[test]
    fn full_queue_keeps_the_visible_toast_and_latest_waiting_toasts() {
        let mut queue = ToastQueue::default();
        for number in 1..=TOAST_WAITING_COUNT_MAX + 3 {
            queue.enqueue(ToastKind::Info, number.to_string(), None);
        }

        assert_eq!(queue.waiting.len(), TOAST_WAITING_COUNT_MAX);
        assert_eq!(messages(&queue), ["1", "4", "5", "6", "7", "8"]);
    }

    #[test]
    fn leaving_toast_stays_visible_until_its_exit_finishes() {
        let mut queue = ToastQueue::default();
        let first = queue.enqueue(ToastKind::Ok, "first", None);
        queue.enqueue(ToastKind::Warn, "second", None);

        assert!(queue.begin_leave(first));
        assert!(!queue.begin_leave(first));
        assert_eq!(visible_phase(&queue), Some(ToastPhase::Leaving));
        assert_eq!(messages(&queue), ["first", "second"]);

        assert!(queue.finish_leave(first));
        assert_eq!(
            visible_phase(&queue),
            Some(ToastPhase::Shown(ToastEntrance::Promote))
        );
        assert_eq!(messages(&queue), ["second"]);
    }

    #[test]
    fn stale_leave_requests_cannot_remove_the_next_toast() {
        let mut queue = ToastQueue::default();
        let first = queue.enqueue(ToastKind::Ok, "first", None);
        let second = queue.enqueue(ToastKind::Error, "second", None);

        assert!(!queue.begin_leave(second));
        assert!(!queue.finish_leave(first));
        assert!(queue.begin_leave(first));
        assert!(!queue.finish_leave(second));
        assert!(queue.finish_leave(first));
        assert!(!queue.finish_leave(first));
        assert!(!queue.begin_leave(first));
        assert_eq!(messages(&queue), ["second"]);
    }

    #[test]
    fn toast_after_an_empty_queue_rises_instead_of_promoting() {
        let mut queue = ToastQueue::default();
        let first = queue.enqueue(ToastKind::Ok, "first", None);
        assert!(queue.begin_leave(first));
        assert!(queue.finish_leave(first));
        assert_eq!(visible_phase(&queue), None);

        queue.enqueue(ToastKind::Ok, "second", None);

        assert_eq!(
            visible_phase(&queue),
            Some(ToastPhase::Shown(ToastEntrance::Rise))
        );
    }

    #[test]
    fn single_toast_is_accessible_without_a_waiting_stack() {
        let mut queue = ToastQueue::default();
        queue.enqueue(ToastKind::Ok, "Changes saved.", None);

        let html = render(queue);

        assert!(html.contains(&format!(r#"data-testid="{}""#, test_ids::TOAST.value())));
        assert!(html.contains(r#"role="status""#));
        assert!(html.contains(r#"aria-live="polite""#));
        assert!(html.contains(r#"aria-atomic="true""#));
        assert!(html.contains("Changes saved."));
        assert!(html.contains(&format!(
            r#"data-testid="{}""#,
            test_ids::TOAST_DISMISS.value()
        )));
        assert!(!html.contains("toast-stack-layer"));
        assert!(!html.contains("toast-waiting-count"));
    }

    #[test]
    fn error_toasts_count_down_longer_than_other_severities() {
        let mut ok = ToastQueue::default();
        ok.enqueue(ToastKind::Ok, "Saved.", None);
        let mut error = ToastQueue::default();
        error.enqueue(ToastKind::Error, "Failed.", None);

        assert!(render(ok).contains("animation-duration: 4000ms"));
        let error_html = render(error);
        assert!(error_html.contains("animation-duration: 6000ms"));
        assert!(error_html.contains(r#"role="alert""#));
    }

    #[test]
    fn detail_renders_below_the_message_only_when_present() {
        let mut plain = ToastQueue::default();
        plain.enqueue(ToastKind::Error, "Git could not complete the push.", None);
        let mut detailed = ToastQueue::default();
        detailed.enqueue(
            ToastKind::Error,
            "Git could not complete the push.",
            Some("fatal: the remote end hung up".into()),
        );

        assert!(!render(plain).contains("toast-detail"));
        let html = render(detailed);
        assert!(html.contains(r#"class="toast-detail""#));
        assert!(html.find("Git could not complete") < html.find("fatal: the remote end hung up"));
    }

    #[test]
    fn waiting_toasts_render_a_bounded_stack_and_count() {
        let mut queue = ToastQueue::default();
        queue.enqueue(ToastKind::Warn, "Current", None);
        for _ in 0..3 {
            queue.enqueue(ToastKind::Info, "Waiting", None);
        }

        let html = render(queue);

        assert_eq!(html.matches(r#"class="toast-stack-layer""#).count(), 2);
        assert!(html.contains(r#"title="3 more notifications""#));
        assert!(html.contains("+3"));
        assert!(html.contains(r#"role="alert""#));
        assert!(!html.contains("Waiting"));
    }

    #[test]
    fn host_renders_one_global_viewport_after_route_content() {
        let html = dioxus_ssr::render_element(rsx! {
            ToastHost {
                main { "Route content" }
            }
        });
        let viewport = r#"class="toast-viewport""#;
        assert!(html.contains("Route content"));
        assert!(html.contains(viewport));
        assert!(html.find("Route content") < html.find(viewport));
        assert_eq!(html.matches(viewport).count(), 1);
    }

    #[allow(dead_code)]
    fn public_atom_api_accepts_into_string(toast: ToastHandle) {
        toast.ok("ok");
        toast.warn(String::from("warn"));
        toast.info("info");
        toast.error(String::from("error"));
    }
}
