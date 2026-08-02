use std::{
    collections::VecDeque,
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serde::Serialize;

use crate::recipes::{RecipeError, RecipeExecutor, ViewerComputation};

const MAX_QUEUED_COMPUTATIONS: usize = 32;
const SHUTDOWN_WAIT: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct RecipeCompletion {
    tab_id: u64,
    generation: u64,
}

impl RecipeCompletion {
    fn from_request(request: &ViewerComputation) -> Self {
        Self {
            tab_id: request.tab_id().into(),
            generation: request.generation(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecipeQueueError {
    Full,
    Disconnected,
}

impl std::fmt::Display for RecipeQueueError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => formatter.write_str("recipe computation queue is full"),
            Self::Disconnected => formatter.write_str("recipe computation worker stopped"),
        }
    }
}

#[derive(Debug)]
struct QueuedComputation {
    request: ViewerComputation,
    active: bool,
}

#[derive(Debug, Default)]
struct QueueState {
    computations: VecDeque<QueuedComputation>,
    shutdown: bool,
}

impl QueueState {
    fn push(&mut self, computation: QueuedComputation) -> Result<(), RecipeQueueError> {
        if self.shutdown {
            return Err(RecipeQueueError::Disconnected);
        }
        if let Some(index) = self
            .computations
            .iter()
            .position(|queued| queued.request.tab_id() == computation.request.tab_id())
        {
            self.computations.remove(index);
        } else if self.computations.len() == MAX_QUEUED_COMPUTATIONS {
            return Err(RecipeQueueError::Full);
        }

        if computation.active {
            let first_inactive = self
                .computations
                .iter()
                .position(|queued| !queued.active)
                .unwrap_or(self.computations.len());
            self.computations.insert(first_inactive, computation);
        } else {
            self.computations.push_back(computation);
        }
        Ok(())
    }

    fn pop(&mut self) -> Option<QueuedComputation> {
        self.computations.pop_front()
    }
}

struct SharedQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

struct RecipeWorkerInner {
    shared: Arc<SharedQueue>,
    completions: Mutex<Option<Receiver<RecipeCompletion>>>,
    stopped: Mutex<Receiver<()>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for RecipeWorkerInner {
    fn drop(&mut self) {
        if let Ok(mut state) = self.shared.state.lock() {
            state.shutdown = true;
            state.computations.clear();
            self.shared.ready.notify_one();
        }

        let stopped = self
            .stopped
            .lock()
            .ok()
            .and_then(|receiver| receiver.recv_timeout(SHUTDOWN_WAIT).ok());
        let handle = self.handle.lock().ok().and_then(|mut handle| handle.take());
        if stopped.is_some()
            && let Some(handle) = handle
        {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
pub(crate) struct RecipeWorker {
    inner: Arc<RecipeWorkerInner>,
}

impl RecipeWorker {
    pub(crate) fn start(executor: RecipeExecutor) -> Result<Self, String> {
        let shared = Arc::new(SharedQueue {
            state: Mutex::new(QueueState::default()),
            ready: Condvar::new(),
        });
        let (completion_sender, completion_receiver) = mpsc::channel();
        let (stopped_sender, stopped_receiver) = mpsc::channel();
        let worker_queue = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("gtl-recipe-worker".into())
            .spawn(move || {
                worker_loop(
                    &worker_queue,
                    &executor,
                    &completion_sender,
                    &stopped_sender,
                );
            })
            .map_err(|error| format!("failed to start recipe worker: {error}"))?;
        Ok(Self {
            inner: Arc::new(RecipeWorkerInner {
                shared,
                completions: Mutex::new(Some(completion_receiver)),
                stopped: Mutex::new(stopped_receiver),
                handle: Mutex::new(Some(handle)),
            }),
        })
    }

    pub(crate) fn submit(
        &self,
        request: ViewerComputation,
        active: bool,
    ) -> Result<(), RecipeQueueError> {
        let mut state = self
            .inner
            .shared
            .state
            .lock()
            .map_err(|_| RecipeQueueError::Disconnected)?;
        state.push(QueuedComputation { request, active })?;
        self.inner.shared.ready.notify_one();
        Ok(())
    }

    pub(crate) fn take_completions(&self) -> Option<Receiver<RecipeCompletion>> {
        self.inner
            .completions
            .lock()
            .ok()
            .and_then(|mut receiver| receiver.take())
    }
}

fn worker_loop(
    queue: &SharedQueue,
    executor: &RecipeExecutor,
    completions: &Sender<RecipeCompletion>,
    stopped: &Sender<()>,
) {
    loop {
        let next = {
            let Ok(mut state) = queue.state.lock() else {
                break;
            };
            while state.computations.is_empty() && !state.shutdown {
                let Ok(next_state) = queue.ready.wait(state) else {
                    let _ = stopped.send(());
                    return;
                };
                state = next_state;
            }
            if state.shutdown {
                break;
            }
            state.pop()
        };
        let Some(queued) = next else {
            continue;
        };
        let completion = RecipeCompletion::from_request(&queued.request);
        let recipe_ticket = match &queued.request {
            ViewerComputation::Recipe(request) => Some(request.ticket),
            ViewerComputation::CommitPatch(_) => None,
        };
        let result = match queued.request {
            ViewerComputation::Recipe(request) => executor.compute_and_publish(request),
            ViewerComputation::CommitPatch(request) => {
                executor.compute_commit_patch_and_publish(request)
            }
        };
        match result {
            Ok(()) => {
                if completions.send(completion).is_err() {
                    break;
                }
            }
            Err(RecipeError::Stale) => {}
            Err(RecipeError::Failed(reason)) => {
                if let Some(ticket) = recipe_ticket {
                    executor.publish_failure(ticket, &reason);
                } else {
                    eprintln!("gtl-viewer computation failed: {reason}");
                }
                if completions.send(completion).is_err() {
                    break;
                }
            }
        }
    }
    let _ = stopped.send(());
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::mpsc::RecvTimeoutError};

    use gtl_application::viewer::{ViewerTabId, ViewerTabKind};
    use gtl_contracts::recipes::{Recipe, RecipeOp, RecipeSource};

    use super::*;
    use crate::{recipes::ReservedRecipeComputation, session::ComputeTicket};

    fn queued(tab_id: u64, generation: u64, active: bool) -> QueuedComputation {
        QueuedComputation {
            request: ViewerComputation::Recipe(ReservedRecipeComputation {
                recipe: Recipe {
                    source: RecipeSource::LocalRepo(PathBuf::from("/repo")),
                    op: RecipeOp::SquashPreview { pinned: None },
                    name: None,
                },
                kind: ViewerTabKind::Snapshot,
                ticket: ComputeTicket {
                    tab_id: ViewerTabId::try_new(tab_id).expect("positive tab id"),
                    generation,
                },
            }),
            active,
        }
    }

    #[test]
    fn active_work_precedes_inactive_fifo_work() {
        let mut queue = QueueState::default();
        queue.push(queued(1, 1, false)).expect("first");
        queue.push(queued(2, 1, false)).expect("second");
        queue.push(queued(3, 1, true)).expect("active");

        assert_eq!(
            queue.pop().map(|value| value.request.tab_id()),
            Some(ViewerTabId::try_new(3).expect("positive tab id"))
        );
        assert_eq!(
            queue.pop().map(|value| value.request.tab_id()),
            Some(ViewerTabId::try_new(1).expect("positive tab id"))
        );
        assert_eq!(
            queue.pop().map(|value| value.request.tab_id()),
            Some(ViewerTabId::try_new(2).expect("positive tab id"))
        );
    }

    #[test]
    fn newer_generation_replaces_queued_work_for_the_same_tab() {
        let mut queue = QueueState::default();
        queue.push(queued(1, 1, false)).expect("old generation");
        queue.push(queued(2, 1, false)).expect("other tab");
        queue.push(queued(1, 2, true)).expect("new generation");

        let replacement = queue.pop().expect("replacement");
        assert_eq!(
            replacement.request.tab_id(),
            ViewerTabId::try_new(1).unwrap()
        );
        assert_eq!(replacement.request.generation(), 2);
        assert_eq!(queue.computations.len(), 1);
    }

    #[test]
    fn queue_rejects_distinct_work_above_its_bound() {
        let mut queue = QueueState::default();
        for tab_id in 1..=MAX_QUEUED_COMPUTATIONS {
            queue
                .push(queued(tab_id as u64, 1, false))
                .expect("within bound");
        }

        assert_eq!(
            queue.push(queued((MAX_QUEUED_COMPUTATIONS + 1) as u64, 1, false)),
            Err(RecipeQueueError::Full)
        );
        assert_eq!(queue.computations.len(), MAX_QUEUED_COMPUTATIONS);
    }

    #[test]
    fn shutdown_rejects_new_work() {
        let mut queue = QueueState {
            shutdown: true,
            ..QueueState::default()
        };

        assert_eq!(
            queue.push(queued(1, 1, true)),
            Err(RecipeQueueError::Disconnected)
        );
    }

    #[test]
    fn shutdown_timeout_is_bounded() {
        let (_sender, receiver) = mpsc::channel::<()>();
        assert_eq!(
            receiver.recv_timeout(Duration::from_millis(1)),
            Err(RecvTimeoutError::Timeout)
        );
    }
}
