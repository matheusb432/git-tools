use dioxus::prelude::*;

#[derive(Clone, Copy)]
pub(crate) struct UnsavedChangesConfirmation {
    pub open: Memo<bool>,
    pub request_confirmation: Callback<Callback<()>>,
    pub confirm_leave: Callback<()>,
    pub cancel_leave: Callback<()>,
}

/// Runs clean-form actions immediately and defers dirty-form actions until confirmation.
/// The latest request replaces the pending action; cancellation drops it.
pub(crate) fn use_unsaved_changes_confirmation(
    dirty: ReadSignal<bool>,
) -> UnsavedChangesConfirmation {
    let mut pending_action = use_signal(|| None::<Callback<()>>);

    UnsavedChangesConfirmation {
        open: use_memo(move || pending_action().is_some()),
        request_confirmation: use_callback(move |action: Callback<()>| {
            if *dirty.peek() {
                pending_action.set(Some(action));
            } else {
                pending_action.set(None);
                action.call(());
            }
        }),
        confirm_leave: use_callback(move |()| {
            let action = pending_action.write().take();
            if let Some(action) = action {
                action.call(());
            }
        }),
        cancel_leave: use_callback(move |()| pending_action.set(None)),
    }
}

#[cfg(test)]
mod tests;
