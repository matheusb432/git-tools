use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use gtl_application::projects::{get_viewer_project_status, list_viewer_projects};
use gtl_infra::{
    git_client::status_context::StatusContextScope, project_status_watch::ProjectStatusWatch,
};
use gtl_models::{
    projects::catalogue::ProjectId,
    repository::status::{RepositoryStatus, StatusChanges, StatusHead},
};
use gtl_wire::{
    proto, v1,
    viewer::projects::{ViewerProject, ViewerProjectSelection, ViewerProjectStatusUpdate},
};
use tokio::sync::mpsc;
use tonic::Status;

use crate::state::AppState;

type Sender = mpsc::Sender<Result<v1::WatchViewerResponse, Status>>;

pub(super) fn spawn(
    state: AppState,
    selection: ViewerProjectSelection,
    sender: Sender,
) -> Result<(), Status> {
    let admission = state
        .viewer_project_watch_requests
        .clone()
        .try_acquire_owned()
        .map_err(|_| Status::resource_exhausted("project status watch limit reached"))?;
    tokio::spawn(async move {
        let _admission = admission;
        supervise(state, selection, sender).await;
    });
    Ok(())
}

async fn supervise(state: AppState, selection: ViewerProjectSelection, sender: Sender) {
    let cancellation = Arc::new(AtomicBool::new(false));
    let started: Arc<Mutex<Option<Instant>>> = Arc::default();
    let permit = tokio::select! {
        () = sender.closed() => return,
        permit = state.viewer_project_watch_workers.clone().acquire_owned() => match permit { Ok(permit) => permit, Err(_) => return },
    };
    let worker = Worker {
        state,
        sender: sender.clone(),
        cancellation: cancellation.clone(),
        started: started.clone(),
    };
    let mut task = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        let _registrations = Registrations(worker.state.viewer_project_watch_registrations.clone());
        let _scope = StatusContextScope::new(worker.cancellation.clone());
        worker.run(&selection)
    });
    let mut deadline = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            () = sender.closed() => break,
            result = &mut task => {
                if !matches!(result, Ok(Ok(()))) { let _ = sender.try_send(Err(Status::unavailable("project status watch stopped"))); }
                break;
            }
            _ = deadline.tick() => {
                if started.lock().is_ok_and(|started| started.is_some_and(|started| started.elapsed() >= Duration::from_secs(30))) {
                    let _ = sender.try_send(Err(Status::deadline_exceeded("project status check timed out")));
                    break;
                }
            }
        }
    }
    cancellation.store(true, Ordering::Relaxed);
}

struct Worker {
    state: AppState,
    sender: Sender,
    cancellation: Arc<AtomicBool>,
    started: Arc<Mutex<Option<Instant>>>,
}

#[derive(Default)]
struct ProjectCheck {
    project: Option<ViewerProject>,
    last_check: Option<Instant>,
    published: Option<ViewerProjectStatusUpdate>,
    pending: Option<ViewerProjectStatusUpdate>,
}

impl ProjectCheck {
    fn queue(&mut self, update: ViewerProjectStatusUpdate) {
        self.pending = (self.published.as_ref() != Some(&update)).then_some(update);
    }
}

impl Worker {
    fn run(&self, selection: &ViewerProjectSelection) -> anyhow::Result<()> {
        let mut watches = ProjectStatusWatch::new(selection.ids().len());
        let mut checks = selection
            .ids()
            .iter()
            .map(|_| ProjectCheck::default())
            .collect::<Vec<_>>();
        while !self.cancelled() {
            for (index, (id, check)) in selection
                .ids()
                .iter()
                .zip(&mut checks)
                .enumerate()
                .take_while(|_| !self.cancelled())
            {
                self.publish_pending(check)?;
                self.check(index, id, check, &mut watches)?;
                self.publish_pending(check)?;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    }

    fn cancelled(&self) -> bool {
        self.cancellation.load(Ordering::Relaxed) || self.sender.is_closed()
    }

    fn publish_pending(&self, check: &mut ProjectCheck) -> anyhow::Result<()> {
        let Some(update) = check.pending.take() else {
            return Ok(());
        };
        let event = v1::WatchViewerResponse {
            version: self.state.viewer.version()?.value(),
            live_check: None,
            project_status: Some(proto::viewer::projects::encode_status_update(
                update.clone(),
            )),
        };
        match self.sender.try_send(Ok(event)) {
            Ok(()) => check.published = Some(update),
            Err(mpsc::error::TrySendError::Full(_)) => check.pending = Some(update),
            Err(mpsc::error::TrySendError::Closed(_)) => (),
        }
        Ok(())
    }

    fn check(
        &self,
        index: usize,
        id: &ProjectId,
        check: &mut ProjectCheck,
        watches: &mut ProjectStatusWatch,
    ) -> anyhow::Result<()> {
        let now = Instant::now();
        let reconcile = check
            .last_check
            .is_none_or(|last| now.duration_since(last) >= watches.interval(index));
        if !reconcile && !watches.changed(index, now) {
            return Ok(());
        }
        let Ok(_permit) = self
            .state
            .viewer_project_status_workers
            .clone()
            .try_acquire_owned()
        else {
            return Ok(());
        };
        *self
            .started
            .lock()
            .map_err(|_| anyhow::anyhow!("status deadline poisoned"))? = Some(now);
        let _deadline = CheckDeadline(self.started.clone());
        let project = {
            let connection = self.state.database.connection_lock()?;
            list_viewer_projects::get_project(id, &connection)?
        };
        let replan = watches.begin_check(index) || project != check.project || reconcile;
        if replan && let Some(previous) = &check.project {
            StatusContextScope::invalidate(previous.path.as_ref());
        }
        self.prepare(index, project.as_ref(), check, watches, replan)?;
        self.state
            .viewer_project_status_checks
            .fetch_add(1, Ordering::Relaxed);
        self.state
            .viewer_project_watch_registrations
            .store(watches.registrations(), Ordering::Relaxed);
        let update = self.read_status(id, project.as_ref())?;
        if self.cancelled() {
            return Ok(());
        }
        tracing::debug!(project = %id, elapsed_ms = now.elapsed().as_millis(), watches = watches.registrations(), "project status checked");
        check.project = project;
        check.last_check = Some(Instant::now());
        check.queue(update);
        Ok(())
    }

    fn prepare(
        &self,
        index: usize,
        project: Option<&ViewerProject>,
        check: &mut ProjectCheck,
        watches: &mut ProjectStatusWatch,
        replan: bool,
    ) -> anyhow::Result<()> {
        let Some(project) = project else {
            watches.detach(index);
            return Ok(());
        };
        if check.last_check.is_none() {
            let cached = self
                .state
                .viewer_project_status_cache
                .lock()
                .map_err(|_| anyhow::anyhow!("status cache poisoned"))?
                .get(&project.id, &project.path, Instant::now());
            check.pending = cached.map(ViewerProjectStatusUpdate::Status);
            self.publish_pending(check)?;
        }
        if replan {
            watches.configure(index, project.path.as_ref(), &self.cancellation);
            // Close the first traversal's registration race while keeping the first watches alive.
            if check.last_check.is_none() {
                watches.configure(index, project.path.as_ref(), &self.cancellation);
            }
        }
        Ok(())
    }

    fn read_status(
        &self,
        id: &ProjectId,
        project: Option<&ViewerProject>,
    ) -> anyhow::Result<ViewerProjectStatusUpdate> {
        let Some(project) = project else {
            return Ok(ViewerProjectStatusUpdate::Unavailable(id.clone()));
        };
        let Ok(mut status) = get_viewer_project_status::execute(project.clone(), &self.state.git)
        else {
            return Ok(ViewerProjectStatusUpdate::Unavailable(id.clone()));
        };
        if self.cancelled()
            || matches!(
                status.status,
                RepositoryStatus::Present {
                    head: StatusHead::Unavailable,
                    ..
                } | RepositoryStatus::Present {
                    changes: StatusChanges::Unavailable,
                    ..
                }
            )
        {
            return Ok(ViewerProjectStatusUpdate::Unavailable(id.clone()));
        }
        if let gtl_wire::viewer::projects::ViewerProjectBranchComparison::Unavailable { reason } =
            &mut status.branch_comparison
        {
            reason.truncate(reason.floor_char_boundary(2048));
        }
        {
            let connection = self.state.database.connection_lock()?;
            gtl_application::projects::status_index::record(project, Some(&status), &connection)?;
        }
        self.state
            .viewer_project_status_cache
            .lock()
            .map_err(|_| anyhow::anyhow!("status cache poisoned"))?
            .insert(project.path.clone(), status.clone(), Instant::now());
        Ok(ViewerProjectStatusUpdate::Status(status))
    }
}

struct CheckDeadline(Arc<Mutex<Option<Instant>>>);
impl Drop for CheckDeadline {
    fn drop(&mut self) {
        if let Ok(mut started) = self.0.lock() {
            *started = None;
        }
    }
}

struct Registrations(Arc<AtomicUsize>);
impl Drop for Registrations {
    fn drop(&mut self) {
        self.0.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reverted_status_removes_an_unsent_intermediate_value() {
        let initial =
            ViewerProjectStatusUpdate::Status(gtl_wire::viewer::projects::ViewerProjectStatus {
                project_id: "TST".try_into().unwrap(),
                status: RepositoryStatus::Absent,
                comparison_branch: gtl_models::projects::comparison::ComparisonBranch::default(),
                branch_comparison:
                    gtl_wire::viewer::projects::ViewerProjectBranchComparison::Upstream,
            });
        let mut check = ProjectCheck {
            published: Some(initial.clone()),
            ..ProjectCheck::default()
        };
        check.queue(ViewerProjectStatusUpdate::Unavailable(
            "TST".try_into().unwrap(),
        ));
        assert!(check.pending.is_some());
        check.queue(initial);
        assert!(check.pending.is_none());
    }
}
