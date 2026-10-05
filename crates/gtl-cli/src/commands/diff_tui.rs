mod browser;
mod code;
mod document;
mod input;
mod render;
mod screen;
mod search;
#[cfg(test)]
mod tests;
mod theme;

use std::{path::Path, time::Duration};

use document::{Document, Layout};
use futures_util::StreamExt as _;
use gtl_wire::v1;
use ratatui::{
    crossterm::{
        event::{
            DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind,
            MouseEventKind,
        },
        execute,
    },
    layout::Rect,
    widgets::ListState,
};

use crate::cli::DiffTarget;

pub(crate) fn repository_request(root: &Path, target: &DiffTarget) -> v1::ReadTerminalDiffRequest {
    v1::ReadTerminalDiffRequest {
        working_directory: root.to_string_lossy().into_owned(),
        target: Some(super::diff::grpc_target(target)),
        text: None,
    }
}

/// Uploads the diff text at `path` and requests it instead of a repository.
pub(crate) fn text_request(path: &Path) -> anyhow::Result<v1::ReadTerminalDiffRequest> {
    let input = super::text_diff::read(&crate::cli::PatchSource::File(path.to_path_buf()))?;
    let stored = crate::server_client::ServerClient::connect()?.store_diff_text(&input.text)?;
    Ok(stored_text_request(stored.text_id, input.label))
}

/// Resolves a remote diff and requests it instead of a repository.
pub(crate) fn remote_request(
    request: &v1::ResolveRemoteDiffRequest,
) -> anyhow::Result<v1::ReadTerminalDiffRequest> {
    let client = crate::server_client::ServerClient::connect()?;
    let stored = super::remote_diff::resolve(&client, request)?;
    Ok(stored_text_request(stored.text_id, stored.label))
}

fn stored_text_request(text_id: String, label: String) -> v1::ReadTerminalDiffRequest {
    v1::ReadTerminalDiffRequest {
        working_directory: String::new(),
        target: None,
        text: Some(v1::TerminalTextDiff { text_id, label }),
    }
}

pub(crate) fn run(request: v1::ReadTerminalDiffRequest) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _restore = RestoreTerminal;
    let mut terminal = ratatui::try_init()?;
    execute!(std::io::stdout(), EnableMouseCapture)?;
    runtime.block_on(session(&mut terminal, request))
}

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        ratatui::restore();
    }
}

type Fetch = tokio::task::JoinHandle<anyhow::Result<Document>>;

fn fetch(request: v1::ReadTerminalDiffRequest) -> Fetch {
    tokio::spawn(async move {
        let client = gtl_client::GtlClient::connect_local().await?;
        let snapshot = client.read_terminal_diff(request).await?;
        Ok(tokio::task::spawn_blocking(move || Document::from(snapshot)).await?)
    })
}

async fn session(
    terminal: &mut ratatui::DefaultTerminal,
    request: v1::ReadTerminalDiffRequest,
) -> anyhow::Result<()> {
    let mut pager = Pager::default();
    let mut pending = Some(fetch(request.clone()));
    let mut pending_review: Option<
        tokio::task::JoinHandle<anyhow::Result<gtl_wire::diff_review::SetDiffFileReviewed>>,
    > = None;
    let mut dirty = true;
    let mut loaded = false;
    let mut events = EventStream::new();
    let mut next_frame = tokio::time::Instant::now();
    loop {
        tokio::select! {
            biased;
            result = async { match &mut pending_review { Some(task) => task.await, None => std::future::pending().await } } => {
                pending_review = None;
                match result? {
                    Ok(request) => {
                        for file in &mut pager.document.files {
                            if let Some(review) = file.review.as_mut().filter(|review| review.reference == request.file) {
                                review.reviewed = request.reviewed;
                            }
                        }
                        pager.browser.list = None;
                        pager.status = if request.reviewed { "File marked reviewed" } else { "File marked unreviewed" }.into();
                    },
                    Err(error) => pager.status = crate::failure::CommandFailure::from_error(&error).text(None),
                }
                dirty = true;
            }
            result = async { match &mut pending { Some(task) => task.await, None => std::future::pending().await } } => {
                pending = None;
                match result? {
                    Ok(document) => { pager.replace(document); loaded = true; },
                    Err(error) if !loaded => return Err(error),
                    Err(error) => pager.status = crate::failure::CommandFailure::from_error(&error).text(None),
                }
                dirty = true;
            }
            () = tokio::time::sleep_until(next_frame), if dirty => {
                terminal.draw(|frame| {
                    pager.resize(frame.area().width, frame.area().height);
                    render::draw(frame, &mut pager, pending.is_some());
                })?;
                dirty = false;
                next_frame = tokio::time::Instant::now() + Duration::from_millis(16);
            }
            event = events.next() => {
                let Some(event) = event else { break; };
                let (action, changed) = dispatch(&mut pager, &event?);
                dirty |= changed;
                match action {
                    input::Action::Quit => break,
                    input::Action::Refresh if pending.is_none() => {
                        pending = Some(fetch(request.clone()));
                        pager.status.clear();
                    }
                    input::Action::ToggleReview(file) if pending_review.is_none() => {
                        if let Some(review) = pager.document.files.get(file).and_then(|file| file.review.clone()) {
                            let request = gtl_wire::diff_review::SetDiffFileReviewed { file: review.reference, reviewed: !review.reviewed };
                            pending_review = Some(tokio::spawn(async move {
                                let client = gtl_client::GtlClient::connect_local().await?;
                                client.set_diff_file_reviewed(request.clone()).await?;
                                Ok(request)
                            }));
                            pager.status = "Saving review progress…".into();
                        }
                    }
                    input::Action::Refresh | input::Action::Continue | input::Action::ToggleReview(_) => {}
                }
                tokio::task::consume_budget().await;
            }
        }
    }
    if let Some(task) = pending {
        task.abort();
    }
    if let Some(task) = pending_review {
        task.await??;
    }
    Ok(())
}

fn dispatch(pager: &mut Pager, event: &Event) -> (input::Action, bool) {
    match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => (input::handle(pager, *key), true),
        Event::Resize(_, _) => (input::Action::Continue, true),
        Event::Mouse(mouse) => {
            let previous = pager.hover;
            let action = input::mouse(pager, *mouse);
            let changed = previous != pager.hover
                || !matches!(mouse.kind, MouseEventKind::Moved | MouseEventKind::Up(_));
            (action, changed)
        }
        _ => (input::Action::Continue, false),
    }
}

#[derive(Default)]
enum Mode {
    #[default]
    Pager,
    Files,
    Search(String),
    Help {
        offset: u16,
    },
}

#[derive(Default)]
struct FileBrowser {
    filter: String,
    state: ListState,
    editing: bool,
    matches: Vec<usize>,
    list: Option<browser::FileList>,
}

struct Pager {
    document: Document,
    layout: Layout,
    mode: Mode,
    offset: usize,
    horizontal: usize,
    wrap: bool,
    full: bool,
    width: u16,
    page: usize,
    search: search::Search,
    status: String,
    browser: FileBrowser,
    screen: screen::Screen,
    hover: Option<screen::Target>,
    added: u64,
    removed: u64,
}

impl Default for Pager {
    fn default() -> Self {
        Self {
            document: Document::default(),
            layout: Layout::default(),
            mode: Mode::Pager,
            offset: 0,
            horizontal: 0,
            wrap: true,
            full: false,
            width: 0,
            page: 1,
            search: search::Search::default(),
            status: String::new(),
            browser: FileBrowser::default(),
            screen: screen::Screen::default(),
            hover: None,
            added: 0,
            removed: 0,
        }
    }
}

impl Pager {
    fn resize(&mut self, width: u16, height: u16) {
        let area = Rect::new(0, 0, width, height);
        if self.screen.area == area {
            return;
        }
        self.screen = screen::Screen::new(area);
        self.page = usize::from(self.screen.content.height).max(1);
        if self.screen.content.width != self.width {
            self.width = self.screen.content.width;
            self.rebuild();
        }
        self.clamp();
    }
    fn replace(&mut self, document: Document) {
        let anchor = self.layout.anchor(&self.document, self.offset);
        self.document = document;
        (self.added, self.removed) =
            self.document
                .files
                .iter()
                .fold((0_u64, 0_u64), |(added, removed), file| {
                    (
                        added.saturating_add(file.added),
                        removed.saturating_add(file.removed),
                    )
                });
        self.layout = Layout::new(
            &self.document,
            usize::from(self.width).max(22),
            self.wrap,
            self.full,
        );
        self.search.rebuild(&self.document, &self.layout);
        self.offset = anchor
            .and_then(|anchor| self.layout.locate(&self.document, &anchor))
            .unwrap_or(0);
        self.status = self.document.notes.join(" · ");
        self.filter_files();
        self.browser
            .state
            .select((!self.browser.matches.is_empty()).then(|| {
                self.browser
                    .state
                    .selected()
                    .unwrap_or(0)
                    .min(self.browser.matches.len() - 1)
            }));
        self.clamp();
    }
    fn rebuild(&mut self) {
        let anchor = self.layout.anchor(&self.document, self.offset);
        self.layout = Layout::new(
            &self.document,
            usize::from(self.width).max(22),
            self.wrap,
            self.full,
        );
        self.search.rebuild(&self.document, &self.layout);
        self.offset = anchor
            .and_then(|anchor| self.layout.locate(&self.document, &anchor))
            .unwrap_or(0);
        self.clamp();
    }
    fn clamp(&mut self) {
        self.offset = self.offset.min(self.layout.screen_rows.saturating_sub(1));
    }
    fn scroll(&mut self, down: bool, amount: usize) {
        self.search.reset_cursor();
        self.offset = if down {
            self.offset.saturating_add(amount)
        } else {
            self.offset.saturating_sub(amount)
        };
        self.clamp();
    }
    fn jump(&mut self, files: bool, forward: bool) {
        self.search.reset_cursor();
        let positions = if files {
            &self.layout.files
        } else {
            &self.layout.hunks
        };
        let destination = if forward {
            positions.get(positions.partition_point(|position| *position <= self.offset))
        } else {
            positions
                .partition_point(|position| *position < self.offset)
                .checked_sub(1)
                .and_then(|index| positions.get(index))
        };
        if let Some(position) = destination {
            self.offset = *position;
            self.clamp();
        }
    }
    fn set_search(&mut self, query: String) {
        self.search = search::Search::new(query, &self.document, &self.layout);
    }

    fn find(&mut self, forward: bool, include_current: bool) {
        if self.search.query().is_empty() {
            return;
        }
        if let Some((index, position)) = self.search.find(self.offset, forward, include_current) {
            self.offset = position;
            self.clamp();
            self.status = format!(
                "Match {index}/{}: {}",
                self.search.match_count(),
                self.search.query()
            );
        } else {
            self.status = format!("No matches: {}", self.search.query());
        }
    }
    fn filter_files(&mut self) {
        let filter = self.browser.filter.to_lowercase();
        self.browser.matches = self
            .document
            .files
            .iter()
            .enumerate()
            .filter(|(_, file)| filter.is_empty() || file.path.to_lowercase().contains(&filter))
            .map(|(index, _)| index)
            .collect();
        self.browser.list = None;
    }

    fn open_file(&mut self, file: usize) {
        self.search.reset_cursor();
        if let Some(position) = self.layout.files.get(file) {
            self.offset = *position;
            self.clamp();
        }
        self.mode = Mode::Pager;
        self.browser.editing = false;
    }

    fn focus_files(&mut self) {
        self.mode = Mode::Files;
        let current = self.layout.entry_at(self.offset).map(|entry| entry.file);
        let matches = &self.browser.matches;
        self.browser.state.select(
            matches
                .iter()
                .position(|file| Some(*file) == current)
                .or((!matches.is_empty()).then_some(0)),
        );
    }
}
