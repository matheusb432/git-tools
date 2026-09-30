use dioxus::core::NoOpMutations;

use super::*;
use crate::test_support::TestResult;

#[derive(Clone, Copy)]
struct ConfirmationHarness {
    dirty: Signal<bool>,
    calls: Signal<Vec<&'static str>>,
    confirmation: UnsavedChangesConfirmation,
    close: Callback<()>,
    snapshots: Callback<()>,
}

fn confirmation_harness() -> Element {
    let dirty = use_signal(|| false);
    let mut calls = use_signal(Vec::new);
    let confirmation = use_unsaved_changes_confirmation(dirty.into());
    let close = use_callback(move |()| calls.write().push("close"));
    let snapshots = use_callback(move |()| calls.write().push("snapshots"));
    use_context_provider(|| ConfirmationHarness {
        dirty,
        calls,
        confirmation,
        close,
        snapshots,
    });
    rsx! { "{(confirmation.open)()}" }
}

fn mounted_confirmation() -> TestResult<(VirtualDom, ConfirmationHarness)> {
    let mut dom = VirtualDom::new(confirmation_harness);
    dom.rebuild_in_place();
    let harness = dom
        .runtime()
        .consume_context::<ConfirmationHarness>(ScopeId::APP)
        .ok_or("confirmation harness")?;
    Ok((dom, harness))
}

#[test]
fn requests_read_current_dirty_state_and_cancellation_preserves_the_form() -> TestResult {
    let (_dom, mut harness) = mounted_confirmation()?;
    let confirmation = harness.confirmation;
    (confirmation.request_confirmation)(harness.close);
    assert_eq!(*harness.calls.peek(), ["close"]);
    assert!(!(confirmation.open)());

    harness.dirty.set(true);
    (confirmation.request_confirmation)(harness.snapshots);
    assert!((confirmation.open)());
    assert_eq!(*harness.calls.peek(), ["close"]);

    (confirmation.cancel_leave)(());
    (confirmation.confirm_leave)(());
    assert!(!(confirmation.open)());
    assert_eq!(*harness.calls.peek(), ["close"]);

    harness.dirty.set(false);
    (confirmation.request_confirmation)(harness.snapshots);
    assert_eq!(*harness.calls.peek(), ["close", "snapshots"]);
    assert!(!(confirmation.open)());
    Ok(())
}

#[test]
fn confirmation_runs_only_the_latest_action_once() -> TestResult {
    let (mut dom, mut harness) = mounted_confirmation()?;
    let confirmation = harness.confirmation;
    harness.dirty.set(true);
    (confirmation.request_confirmation)(harness.close);
    dom.render_immediate(&mut NoOpMutations);
    (confirmation.request_confirmation)(harness.snapshots);
    assert!(harness.calls.peek().is_empty());

    (confirmation.confirm_leave)(());
    (confirmation.confirm_leave)(());
    assert_eq!(*harness.calls.peek(), ["snapshots"]);
    assert!(!(confirmation.open)());
    Ok(())
}
