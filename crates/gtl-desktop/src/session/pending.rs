use std::{collections::VecDeque, sync::Mutex};

use gtl_contracts::recipes::OpenRecipes;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PendingRecipesError {
    Poisoned,
}

impl std::fmt::Display for PendingRecipesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("pending recipe queue is unavailable")
    }
}

impl std::error::Error for PendingRecipesError {}

/// Complete recipe batches waiting for the viewer backend to consume them.
#[derive(Debug, Default)]
pub(crate) struct PendingRecipes {
    batches: Mutex<VecDeque<OpenRecipes>>,
    consumer: Mutex<()>,
}

impl PendingRecipes {
    pub(crate) fn with_consumer<T>(
        &self,
        operation: impl FnOnce() -> T,
    ) -> Result<T, PendingRecipesError> {
        let _consumer = self
            .consumer
            .lock()
            .map_err(|_| PendingRecipesError::Poisoned)?;
        Ok(operation())
    }
    pub(crate) fn push(&self, batch: OpenRecipes) -> Result<(), PendingRecipesError> {
        self.batches
            .lock()
            .map_err(|_| PendingRecipesError::Poisoned)?
            .push_back(batch);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn drain(&self) -> Vec<OpenRecipes> {
        self.try_drain().expect("test queue is not poisoned")
    }

    pub(crate) fn try_drain(&self) -> Result<Vec<OpenRecipes>, PendingRecipesError> {
        Ok(self
            .batches
            .lock()
            .map_err(|_| PendingRecipesError::Poisoned)?
            .drain(..)
            .collect())
    }

    pub(crate) fn prepend(&self, batches: Vec<OpenRecipes>) -> Result<(), PendingRecipesError> {
        let mut queue = self
            .batches
            .lock()
            .map_err(|_| PendingRecipesError::Poisoned)?;
        for batch in batches.into_iter().rev() {
            queue.push_front(batch);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Barrier};

    use gtl_contracts::recipes::{OpenRecipes, RecipeBatchKind};

    use super::*;

    fn batch(id: &str) -> OpenRecipes {
        OpenRecipes {
            batch_id: id.into(),
            kind: RecipeBatchKind::Snapshot,
            recipes: Vec::new(),
        }
    }

    #[test]
    fn drain_atomically_takes_every_batch_in_fifo_order() {
        let pending = PendingRecipes::default();
        pending.push(batch("first")).expect("push");
        pending.push(batch("second")).expect("push");

        assert_eq!(pending.drain(), vec![batch("first"), batch("second")]);
        assert!(pending.drain().is_empty());
    }

    #[test]
    fn pending_recipes_can_be_shared_across_tauri_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PendingRecipes>();
    }

    #[test]
    fn prepend_places_failed_work_ahead_of_concurrent_enqueues() {
        let pending = PendingRecipes::default();
        pending.push(batch("drained")).expect("push");
        let drained = pending.try_drain().expect("drain");
        pending.push(batch("concurrent")).expect("concurrent push");
        pending.prepend(drained).expect("requeue");

        assert_eq!(pending.drain(), vec![batch("drained"), batch("concurrent")]);
    }

    #[test]
    fn poisoned_queue_operations_return_typed_errors() {
        let pending = PendingRecipes::default();
        let _ = std::panic::catch_unwind(|| {
            let _guard = pending.batches.lock().expect("initial lock");
            panic!("poison queue");
        });

        assert_eq!(
            pending.push(batch("push")),
            Err(PendingRecipesError::Poisoned)
        );
        assert_eq!(pending.try_drain(), Err(PendingRecipesError::Poisoned));
        assert_eq!(
            pending.prepend(vec![batch("prepend")]),
            Err(PendingRecipesError::Poisoned)
        );
    }

    #[test]
    fn second_consumer_waits_for_failed_remainder_before_newer_producer_work() {
        let pending = Arc::new(PendingRecipes::default());
        pending.push(batch("older")).expect("older push");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let first = {
            let pending = Arc::clone(&pending);
            let entered = Arc::clone(&entered);
            let release = Arc::clone(&release);
            std::thread::spawn(move || {
                pending.with_consumer(|| {
                    let failed = pending.try_drain().expect("first drain");
                    entered.wait();
                    release.wait();
                    pending.prepend(failed).expect("restore failure");
                })
            })
        };
        entered.wait();
        pending.push(batch("newer")).expect("producer remains live");
        let second = {
            let pending = Arc::clone(&pending);
            std::thread::spawn(move || {
                pending.with_consumer(|| {
                    pending
                        .try_drain()
                        .expect("second drain")
                        .into_iter()
                        .map(|batch| batch.batch_id)
                        .collect::<Vec<_>>()
                })
            })
        };
        release.wait();
        first.join().expect("first joins").expect("first consumer");
        assert_eq!(
            second
                .join()
                .expect("second joins")
                .expect("second consumer"),
            vec!["older", "newer"]
        );
    }
}
