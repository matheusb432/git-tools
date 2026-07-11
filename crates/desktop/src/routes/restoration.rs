use std::sync::{Condvar, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestoreState {
    NotStarted,
    InProgress,
    Complete,
}

#[derive(Debug)]
pub(super) struct RestorationGate {
    state: Mutex<RestoreState>,
    changed: Condvar,
}

impl Default for RestorationGate {
    fn default() -> Self {
        Self {
            state: Mutex::new(RestoreState::NotStarted),
            changed: Condvar::new(),
        }
    }
}

impl RestorationGate {
    pub(super) fn run_once(
        &self,
        operation: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "restore gate poisoned".to_string())?;
        loop {
            match *state {
                RestoreState::Complete => return Ok(false),
                RestoreState::NotStarted => {
                    *state = RestoreState::InProgress;
                    break;
                }
                RestoreState::InProgress => {
                    state = self
                        .changed
                        .wait(state)
                        .map_err(|_| "restore gate poisoned".to_string())?;
                }
            }
        }
        drop(state);

        let result = operation();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "restore gate poisoned".to_string())?;
        *state = if result.is_ok() {
            RestoreState::Complete
        } else {
            RestoreState::NotStarted
        };
        self.changed.notify_all();
        result.map(|()| true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    };

    use super::RestorationGate;

    #[test]
    fn failure_returns_to_not_started_for_retry() {
        let gate = RestorationGate::default();
        assert!(gate.run_once(|| Err("failed".into())).is_err());
        assert!(gate.run_once(|| Ok(())).expect("retry succeeds"));
        assert!(
            !gate
                .run_once(|| panic!("already complete"))
                .expect("complete")
        );
    }

    #[test]
    fn concurrent_request_waits_for_the_single_owner() {
        let gate = Arc::new(RestorationGate::default());
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let calls = Arc::new(AtomicUsize::new(0));
        let owner = {
            let gate = Arc::clone(&gate);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            let calls = Arc::clone(&calls);
            std::thread::spawn(move || {
                gate.run_once(|| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    entered.wait();
                    release.wait();
                    Ok(())
                })
            })
        };
        entered.wait();
        let waiter = {
            let gate = Arc::clone(&gate);
            let calls = Arc::clone(&calls);
            std::thread::spawn(move || {
                gate.run_once(|| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            })
        };
        release.wait();
        assert!(owner.join().expect("owner joins").expect("owner succeeds"));
        assert!(
            !waiter
                .join()
                .expect("waiter joins")
                .expect("waiter succeeds")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
