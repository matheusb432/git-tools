mod planning;

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};
use planning::{Plan, plan};

const WATCHES_MAX: usize = 4096;
const PLAN_BYTES_MAX: usize = 512 * 1024;

#[derive(Clone, Copy, Default)]
struct Change {
    first: Option<Instant>,
    last: Option<Instant>,
    replan: bool,
    immediate: bool,
}

impl Change {
    fn due(self, now: Instant) -> bool {
        self.immediate
            || self
                .first
                .is_some_and(|first| now.duration_since(first) >= Duration::from_secs(5))
            || self
                .last
                .is_some_and(|last| now.duration_since(last) >= Duration::from_secs(1))
    }
}

pub struct ProjectStatusWatch {
    watcher: Option<RecommendedWatcher>,
    plans: Arc<Mutex<Vec<Plan>>>,
    changes: Arc<Mutex<Vec<Change>>>,
    failed: Arc<AtomicBool>,
    registered: BTreeSet<PathBuf>,
    native: Vec<bool>,
}

impl ProjectStatusWatch {
    #[must_use]
    pub fn new(projects: usize) -> Self {
        let plans = Arc::new(Mutex::new(
            (0..projects).map(|_| Plan::default()).collect::<Vec<_>>(),
        ));
        let changes = Arc::new(Mutex::new(vec![Change::default(); projects]));
        let failed = Arc::new(AtomicBool::new(false));
        let event_plans = plans.clone();
        let event_changes = changes.clone();
        let event_failed = failed.clone();
        let watcher = RecommendedWatcher::new(
            move |event| {
                let Ok(mut changes) = event_changes.lock() else {
                    return;
                };
                let Ok(plans) = event_plans.lock() else {
                    return;
                };
                record_event(event, &plans, &mut changes, &event_failed);
            },
            Config::default().with_follow_symlinks(false),
        )
        .ok();
        Self {
            watcher,
            plans,
            changes,
            failed,
            registered: BTreeSet::new(),
            native: vec![false; projects],
        }
    }

    #[must_use]
    pub fn interval(&self, project: usize) -> Duration {
        Duration::from_secs(
            if self.native[project] && !self.failed.load(Ordering::Relaxed) {
                60
            } else {
                30
            },
        )
    }

    #[must_use]
    pub fn changed(&self, project: usize, now: Instant) -> bool {
        self.changes
            .lock()
            .ok()
            .and_then(|changes| changes.get(project).copied())
            .is_some_and(|change| change.due(now))
    }

    #[must_use]
    pub fn begin_check(&self, project: usize) -> bool {
        self.changes.lock().map_or(true, |mut changes| {
            std::mem::take(&mut changes[project]).replan
        })
    }

    pub fn detach(&mut self, project: usize) {
        if let Ok(mut plans) = self.plans.lock() {
            plans[project] = Plan::default();
        }
        self.native[project] = false;
        self.remove_unused();
    }

    fn remove_unused(&mut self) {
        let unused = {
            let Ok(plans) = self.plans.lock() else {
                return;
            };
            self.registered
                .iter()
                .filter(|directory| {
                    !plans
                        .iter()
                        .any(|plan| plan.directories.contains(*directory))
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        // notify waits for its event thread, whose callback also acquires the plan lock.
        unwatch(watcher, &unused);
        for directory in unused {
            self.registered.remove(&directory);
        }
    }

    pub fn configure(&mut self, project: usize, path: &Path, cancellation: &AtomicBool) {
        if self.failed.swap(false, Ordering::Relaxed) {
            let projects = self.native.len();
            *self = Self::new(projects);
            if let Ok(mut changes) = self.changes.lock() {
                changes.fill(Change {
                    replan: true,
                    immediate: true,
                    ..Change::default()
                });
            }
        }
        let plan = plan(path, cancellation).unwrap_or_default();
        self.native[project] = false;
        let bytes = self.plans.lock().map_or(PLAN_BYTES_MAX, |plans| {
            plans
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != project)
                .map(|(_, plan)| plan.bytes())
                .sum::<usize>()
        });
        if bytes + plan.bytes() > PLAN_BYTES_MAX {
            self.detach(project);
            return;
        }
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        if self.registered.len() + plan.directories.difference(&self.registered).count()
            > WATCHES_MAX
        {
            self.detach(project);
            return;
        }
        let mut added: Vec<PathBuf> = Vec::new();
        for directory in &plan.directories {
            if self.registered.contains(directory) {
                continue;
            }
            if watcher
                .watch(directory, RecursiveMode::NonRecursive)
                .is_err()
            {
                unwatch(watcher, &added);
                self.registered
                    .retain(|directory| !added.contains(directory));
                self.detach(project);
                return;
            }
            self.registered.insert(directory.clone());
            added.push(directory.clone());
        }
        self.native[project] = !plan.directories.is_empty();
        if let Ok(mut plans) = self.plans.lock() {
            plans[project] = plan;
        }
        self.remove_unused();
    }

    #[must_use]
    pub fn registrations(&self) -> usize {
        self.registered.len()
    }
}

fn record_event(
    event: notify::Result<Event>,
    plans: &[Plan],
    changes: &mut [Change],
    failed: &AtomicBool,
) {
    let event = match event {
        Ok(event) if !event.need_rescan() => event,
        _ => {
            failed.store(true, Ordering::Relaxed);
            changes.fill(Change {
                replan: true,
                immediate: true,
                ..Change::default()
            });
            return;
        }
    };
    if matches!(event.kind, EventKind::Access(_)) {
        return;
    }
    for (plan, change) in plans.iter().zip(changes.iter_mut()) {
        if !event.paths.iter().any(|path| plan.accepts(path)) {
            continue;
        }
        let now = Instant::now();
        change.first.get_or_insert(now);
        change.last = Some(now);
        change.replan |= matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Remove(_)
                | EventKind::Modify(notify::event::ModifyKind::Name(_))
        ) || event.paths.iter().any(|path| plan.replans(path));
    }
}

fn unwatch(watcher: &mut RecommendedWatcher, directories: &[PathBuf]) {
    for directory in directories {
        let _ = watcher.unwatch(directory);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sustained_events_cannot_extend_the_debounce_past_five_seconds() {
        let first = Instant::now();
        let change = Change {
            first: Some(first),
            last: Some(first + Duration::from_millis(4900)),
            ..Change::default()
        };
        assert!(!change.due(first + Duration::from_millis(4999)));
        assert!(change.due(first + Duration::from_secs(5)));
    }

    #[test]
    fn overflow_invalidates_the_whole_page_for_immediate_reconciliation() {
        let mut changes = vec![Change::default(); 3];
        let failed = AtomicBool::new(false);
        record_event(
            Err(notify::Error::generic("overflow")),
            &[],
            &mut changes,
            &failed,
        );
        assert!(failed.load(Ordering::Relaxed));
        assert!(
            changes
                .iter()
                .all(|change| change.replan && change.due(Instant::now()))
        );
    }
    #[test]
    fn removing_watches_releases_the_plan_lock_before_waiting_for_callbacks() {
        let directory = tempfile::tempdir().unwrap();
        let mut watch = ProjectStatusWatch::new(1);
        watch.configure(0, directory.path(), &AtomicBool::new(false));
        assert!(watch.registrations() > 0);
        let changes = watch.changes.clone();
        let blocked_callback = changes.lock().unwrap();
        std::fs::write(directory.path().join("file"), "event").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            watch.detach(0);
            sender.send(watch.registrations()).unwrap();
        });
        std::thread::sleep(Duration::from_millis(50));
        drop(blocked_callback);
        assert_eq!(receiver.recv_timeout(Duration::from_secs(2)).unwrap(), 0);
        worker.join().unwrap();
    }
}
