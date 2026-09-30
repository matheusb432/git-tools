pub(crate) mod time;

use std::time::Duration;

use dioxus::prelude::*;
use gtl_models::{
    diffs::{CommitId, CommitIdAbbreviation},
    paths::RepositoryRoot,
};
use gtl_wire::viewer::commit_search::{
    OpenViewerCommit, SearchViewerCommits, ViewerCommitSearchResult, ViewerCommitSearchScope,
};
use lucide_dioxus::{GitBranch, Layers, Search, X};

use self::time::{CommitSearchTimeError, CommitSearchTimeInput};
use crate::{
    app::{application_layout::ViewerContext, application_router::Route},
    entities::diffs::viewer_server,
    shared::{
        browser,
        date_display::DateDisplayTime,
        failure_notice::client_error_message,
        i18n::{t, use_language},
        ui::{
            Button, ButtonLayout, ButtonSize, ButtonState, ButtonVariant, TextInput,
            TextInputLabelVisibility,
            popover::{PopoverPlacement, PopoverSurface},
        },
        viewer_client::ViewerClientError,
    },
};

#[derive(Clone)]
pub(super) struct CommitSearchResponse {
    pub request: SearchViewerCommits,
    pub result: Result<ViewerCommitSearchResult, ViewerClientError>,
}

pub(super) fn use_commit_search(
    request: Memo<Option<SearchViewerCommits>>,
) -> Resource<Option<CommitSearchResponse>> {
    use_resource(move || {
        let request = request();
        async move {
            let request = request?;
            dioxus_sdk_time::sleep(Duration::from_millis(180)).await;
            let result = viewer_server::search_commits(request.clone()).await;
            Some(CommitSearchResponse { request, result })
        }
    })
}

#[component]
pub(crate) fn CommitSearchInput(
    id: String,
    query: String,
    #[props(default)] time: CommitSearchTimeInput,
    time_error: Option<CommitSearchTimeError>,
    #[props(default)] snapshot: bool,
    #[props(default)] active_branch: bool,
    onchange: EventHandler<String>,
    ontime: EventHandler<CommitSearchTimeInput>,
    onscope: Option<EventHandler<bool>>,
    onfirst: Option<EventHandler<()>>,
    onopenfirst: Option<EventHandler<()>>,
    ondismiss: Option<EventHandler<()>>,
) -> Element {
    let language = use_language();
    let keyboard_query = query.clone();
    let clear_id = id.clone();
    let scope_id = format!("{id}-scope");
    let scope_input_id = id.clone();
    let time_id = id.clone();
    let hint = if snapshot && !active_branch {
        t!(language, "commit-search-snapshot-hint")
    } else {
        t!(language, "commit-search-branch-hint")
    };
    let scope_label = if snapshot && !active_branch {
        t!(language, "commit-search-snapshot")
    } else {
        t!(language, "commit-search-branch")
    };
    rsx! {
        section {
            class: "commit-search-input",
            role: "search",
            aria_label: t!(language, "commit-search-label"),
            div { class: "relative",
                span { class: "commit-search-icon", aria_hidden: "true",
                    Search { size: 14 }
                }
                TextInput {
                    id,
                    label: t!(language, "commit-search-label"),
                    label_visibility: TextInputLabelVisibility::Hidden,
                    class: if snapshot { "pl-8 pr-14" } else { "pl-8 pr-9" },
                    value: query.clone(),
                    r#type: "search",
                    autocomplete: "off",
                    "spellcheck": "false",
                    maxlength: "256",
                    placeholder: t!(language, "commit-search-placeholder"),
                    supporting_content: rsx! {
                        span { class: "commit-search-hint",
                            span { {t!(language, "commit-search-hint")} }
                            span { title: hint, aria_live: "polite", "{scope_label}" }
                        }
                    },
                    oninput: move |event: FormEvent| onchange.call(event.value()),
                    "data-dialog-content-initial-focus": "true",
                    onkeydown: move |event: KeyboardEvent| {
                        match event.key() {
                            Key::Escape if !keyboard_query.is_empty() => {
                                event.prevent_default();
                                event.stop_propagation();
                                onchange.call(String::new());
                            }
                            Key::Escape if ondismiss.is_some() => {
                                event.prevent_default();
                                event.stop_propagation();
                                if let Some(ondismiss) = ondismiss {
                                    ondismiss.call(());
                                }
                            }
                            Key::ArrowDown => {
                                event.prevent_default();
                                if let Some(onfirst) = onfirst {
                                    onfirst.call(());
                                }
                            }
                            Key::Enter => {
                                event.prevent_default();
                                if let Some(onopenfirst) = onopenfirst {
                                    onopenfirst.call(());
                                }
                            }
                            _ => {}
                        }
                    },
                }
                div { class: "commit-search-actions",
                    if !query.is_empty() {
                        Button {
                            variant: ButtonVariant::Ghost,
                            size: ButtonSize::IconCompact,
                            aria_label: t!(language, "commit-search-clear"),
                            title: t!(language, "commit-search-clear"),
                            onclick: move |_| {
                                onchange.call(String::new());
                                browser::focus_element(clear_id.clone());
                            },
                            X { size: 14 }
                        }
                    }
                    if snapshot {
                        CommitSearchScope {
                            id: scope_id,
                            input_id: scope_input_id,
                            active_branch,
                            onchange: move |value| {
                                if let Some(onscope) = onscope {
                                    onscope.call(value);
                                }
                            },
                        }
                    }
                }
            }
            CommitSearchTimeFields {
                id: time_id,
                input: time,
                error: time_error,
                onchange: ontime,
            }
        }
    }
}

#[component]
fn CommitSearchTimeFields(
    id: String,
    input: CommitSearchTimeInput,
    error: Option<CommitSearchTimeError>,
    onchange: EventHandler<CommitSearchTimeInput>,
) -> Element {
    let language = use_language();
    let from_input = input.clone();
    let until_input = input.clone();
    rsx! {
        div { class: "commit-search-time",
            TextInput {
                id: format!("{id}-from"),
                label: t!(language, "commit-search-time-from"),
                r#type: "datetime-local",
                step: "1",
                value: input.from.clone(),
                error: (error == Some(CommitSearchTimeError::From))
                    .then(|| t!(language, "commit-search-time-invalid")),
                onchange: move |event: FormEvent| {
                    onchange
                        .call(CommitSearchTimeInput {
                            from: event.value(),
                            ..from_input.clone()
                        });
                },
            }
            TextInput {
                id: format!("{id}-until"),
                label: t!(language, "commit-search-time-until"),
                r#type: "datetime-local",
                step: "1",
                value: input.until.clone(),
                error: match error {
                    Some(CommitSearchTimeError::Until) => {
                        Some(t!(language, "commit-search-time-invalid"))
                    }
                    Some(CommitSearchTimeError::Reversed) => {
                        Some(t!(language, "commit-search-time-reversed"))
                    }
                    _ => None,
                },
                onchange: move |event: FormEvent| {
                    onchange
                        .call(CommitSearchTimeInput {
                            until: event.value(),
                            ..until_input.clone()
                        });
                },
            }
        }
        div { class: "commit-search-hint",
            span { {t!(language, "commit-search-time-hint")} }
            Button {
                variant: ButtonVariant::Ghost,
                size: ButtonSize::IconCompact,
                state: if input.has_bounds() { ButtonState::Enabled } else { ButtonState::Disabled },
                aria_label: t!(language, "commit-search-time-clear"),
                title: t!(language, "commit-search-time-clear"),
                onclick: move |_| onchange.call(CommitSearchTimeInput::default()),
                X { size: 14 }
            }
        }
    }
}

#[component]
fn CommitSearchScope(
    id: String,
    input_id: String,
    active_branch: bool,
    onchange: EventHandler<bool>,
) -> Element {
    let language = use_language();
    let anchor_name = format!("--{id}");
    let hint = if active_branch {
        t!(language, "commit-search-branch-hint")
    } else {
        t!(language, "commit-search-snapshot-hint")
    };
    rsx! {
        span { class: "icon-popover group/icon-popover",
            Button {
                class: "icon-popover-trigger",
                variant: ButtonVariant::Ghost,
                size: ButtonSize::IconCompact,
                popovertarget: id.clone(),
                popovertargetaction: "toggle",
                aria_label: t!(language, "commit-search-scope"),
                aria_controls: id.clone(),
                style: "anchor-name: {anchor_name};",
                title: hint,
                if active_branch {
                    GitBranch { size: 14 }
                } else {
                    Layers { size: 14 }
                }
            }
            PopoverSurface {
                id: id.clone(),
                class: "commit-search-scope-menu",
                placement: PopoverPlacement::TriggerEnd,
                role: "group",
                aria_label: t!(language, "commit-search-scope"),
                style: "position-anchor: {anchor_name};",
                div { class: "grid gap-1 p-1",
                    for (branch, label) in [
                        (false, t!(language, "commit-search-snapshot-option")),
                        (true, t!(language, "commit-search-branch-option")),
                    ]
                    {
                        {
                            let popover_id = id.clone();
                            let focus_id = input_id.clone();
                            rsx! {
                                Button {
                                    key: "{branch}",
                                    variant: ButtonVariant::Toggle,
                                    size: ButtonSize::Small,
                                    layout: ButtonLayout::FullWidthStart,
                                    aria_pressed: (active_branch == branch).to_string(),
                                    onclick: move |_| {
                                        onchange.call(branch);
                                        browser::hide_popover(&popover_id);
                                        browser::focus_element(focus_id.clone());
                                    },
                                    if branch {
                                        GitBranch { size: 14 }
                                    } else {
                                        Layers { size: 14 }
                                    }
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub(crate) fn CommitSearchResults(
    id: String,
    result: Option<Result<ViewerCommitSearchResult, ViewerClientError>>,
    #[props(default)] loading: bool,
    #[props(default)] disabled: bool,
    selected: Option<CommitId>,
    onselect: EventHandler<CommitId>,
    onretry: EventHandler<()>,
) -> Element {
    let language = use_language();
    let commits = result.as_ref().and_then(|result| result.as_ref().ok());
    let count = commits.map_or(0, |result| result.commits.len());
    rsx! {
        div { class: "commit-search-results", aria_busy: loading.to_string(),
            p {
                class: "commit-search-status flex items-center justify-between gap-2",
                role: "status",
                aria_live: "polite",
                if loading {
                    {t!(language, "commit-search-loading")}
                } else if let Some(result) = commits {
                    span {
                        {
                            if result.total_matches > result.commits.len() as u64 {
                                t!(
                                    language, "commit-search-count-limited", shown = result.commits.len(),
                                    total = result.total_matches
                                )
                            } else {
                                t!(language, "commit-search-count", total = result.total_matches)
                            }
                        }
                    }
                    if let Some(gtl_models::git::GitHead::Branch(branch)) = &result.branch {
                        span {
                            class: "max-w-40 truncate",
                            title: branch.to_string(),
                            "{branch}"
                        }
                    }
                }
            }
            if let Some(Err(error)) = &result {
                div { class: "px-3 py-2", role: "alert",
                    p { class: "text-xs text-warn", "{client_error_message(error, language)}" }
                    Button {
                        variant: ButtonVariant::Ghost,
                        size: ButtonSize::Small,
                        onclick: move |_| onretry.call(()),
                        {t!(language, "action-retry")}
                    }
                }
            }
            if let Some(result) = commits {
                if result.commits.is_empty() && !loading {
                    p { class: "px-3 py-2 text-xs leading-5 text-ink-3",
                        if result.branch == Some(gtl_models::git::GitHead::Detached) {
                            {t!(language, "commit-search-detached")}
                        } else {
                            {t!(language, "commit-search-empty")}
                        }
                    }
                }
                ul {
                    class: "m-0 list-none p-0",
                    aria_label: t!(language, "commit-search-results"),
                    for (index, commit) in result.commits.iter().enumerate() {
                        {
                            let commit_id = commit.id.clone();
                            let abbreviated = commit.id.abbreviated(CommitIdAbbreviation::TenCharacters);
                            let input_id = id.clone();
                            let result_prefix = id.clone();
                            let result_id = format!("{id}-result-{index}");
                            rsx! {
                                li {
                                    key: "{commit.id}",
                                    onkeydown: move |event: KeyboardEvent| {
                                        let target = match event.key() {
                                            Key::ArrowUp if index == 0 => Some(input_id.clone()),
                                            Key::ArrowUp => Some(format!("{result_prefix}-result-{}", index - 1)),
                                            Key::ArrowDown if index + 1 < count => {
                                                Some(format!("{result_prefix}-result-{}", index + 1))
                                            }
                                            Key::Home => Some(format!("{result_prefix}-result-0")),
                                            Key::End => {
                                                Some(format!("{result_prefix}-result-{}", count.saturating_sub(1)))
                                            }
                                            _ => None,
                                        };
                                        if let Some(target) = target {
                                            event.prevent_default();
                                            browser::focus_element(target);
                                        }
                                    },

                                    Button {
                                        id: result_id,
                                        variant: ButtonVariant::Bare,
                                        layout: crate::shared::ui::ButtonLayout::Block,
                                        size: ButtonSize::Content,
                                        class: "commit-search-result",
                                        state: if disabled || loading { ButtonState::Disabled } else { ButtonState::Enabled },
                                        title: "{commit.subject}\n{commit.id}",
                                        aria_pressed: selected.as_ref().map(|id| (id == &commit.id).to_string()),
                                        aria_label: t!(
                                            language, "commit-search-open", subject = commit.subject.as_str(), id =
                                            abbreviated.clone()
                                        ),
                                        onclick: move |_| onselect.call(commit_id.clone()),
                                        span { class: "block break-words text-sm text-ink", "{commit.subject}" }
                                        span { class: "mt-1 flex flex-wrap gap-x-3 text-xs text-ink-3",
                                            code { "{abbreviated}" }
                                            DateDisplayTime { timestamp: commit.committed_at.clone() }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CommitOpen {
    pub open: Callback<CommitId>,
    pub pending: ReadSignal<bool>,
    pub error: ReadSignal<Option<ViewerClientError>>,
}

pub(super) fn use_open_commit(scope: ViewerCommitSearchScope) -> CommitOpen {
    let viewer = try_consume_context::<ViewerContext>();
    let router = use_hook(try_router);
    let mut pending = use_signal(|| false);
    let mut error = use_signal(|| None);
    let mut action = use_action(move |request: OpenViewerCommit| async move {
        let Some(viewer) = viewer else {
            return Ok::<_, std::convert::Infallible>(());
        };
        let Some(router) = router else {
            pending.set(false);
            return Ok::<_, std::convert::Infallible>(());
        };
        let instance = viewer.server_instance_id();
        let result = async {
            let opened = viewer_server::open_commit(request).await?;
            let shell = viewer_server::get_shell().await?;
            Ok::<_, ViewerClientError>((opened.tab_id, shell))
        }
        .await;
        pending.set(false);
        if viewer.server_instance_id() != instance {
            return Ok::<_, std::convert::Infallible>(());
        }
        match result {
            Ok((tab_id, shell)) => {
                viewer.replace_shell(shell);
                router.push(Route::Diff { tab_id });
            }
            Err(failure) => error.set(Some(failure)),
        }
        Ok::<_, std::convert::Infallible>(())
    });
    let open = use_callback(move |id| {
        if *pending.peek() || viewer.is_none_or(|viewer| !viewer.actions_enabled()) {
            return;
        }
        pending.set(true);
        error.set(None);
        action.call(OpenViewerCommit {
            scope: scope.clone(),
            id,
        });
    });
    CommitOpen {
        open,
        pending: pending.into(),
        error: error.into(),
    }
}

#[component]
pub(super) fn CommitFinder(path: RepositoryRoot) -> Element {
    let language = use_language();
    let mut query = use_signal(String::new);
    let mut time = use_signal(CommitSearchTimeInput::default);
    let time_range = use_memo(move || time().parse(browser::local_offset_at));
    let search_path = path.clone();
    let request = use_memo(move || {
        Some(SearchViewerCommits {
            scope: ViewerCommitSearchScope::ActiveBranch(search_path.clone()),
            query: query(),
            time_range: time_range().ok()?,
        })
    });
    let mut search = use_commit_search(request);
    let opening = use_open_commit(ViewerCommitSearchScope::ActiveBranch(path));
    let response = search.read();
    let response = response.as_ref().and_then(Option::as_ref);
    let current = *search.state().read() != UseResourceState::Pending
        && response.is_some_and(|response| Some(&response.request) == request.read().as_ref());
    let result = response.map(|response| response.result.clone());
    let first = current
        .then(|| {
            result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .and_then(|result| result.commits.first())
                .map(|commit| commit.id.clone())
        })
        .flatten();
    rsx! {
        div { class: "commit-finder",
            CommitSearchInput {
                id: "project-commit-search-input",
                query: query(),
                time: time(),
                time_error: time_range().err(),
                onchange: move |value| query.set(value),
                ontime: move |value| time.set(value),
                onfirst: move |()| browser::focus_element("project-commit-search-input-result-0".into()),
                onopenfirst: move |()| {
                    if let Some(id) = first.clone() {
                        opening.open.call(id);
                    }
                },
            }
            if let Some(error) = (opening.error)() {
                p { class: "px-3 text-xs text-warn", role: "alert",
                    "{client_error_message(&error, language)}"
                }
            }
            crate::shared::ui::ScrollArea { class: "min-h-0",
                CommitSearchResults {
                    id: "project-commit-search-input",
                    result: request().and(result),
                    loading: request().is_some() && !current,
                    disabled: (opening.pending)(),
                    onselect: opening.open,
                    onretry: move |()| search.restart(),
                }
            }
        }
    }
}
