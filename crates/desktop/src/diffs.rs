//! Pending-recipe handoff from CLI argv to the frontend. The Rust side queues
//! every `gtl-recipe://` batch before emitting a best-effort wake event; the
//! frontend receives batches only by atomically draining this queue.
use std::sync::Mutex;

use gtl_recipe::{OpenRecipes, decode_token};

/// The first `gtl-recipe://…` argument in an argv, decoded into its batch, if
/// any. Ignores argv[0] and flags; never panics on malformed input since
/// `decode_token` itself returns `None` for any mismatch.
pub fn recipes_from_argv(argv: &[String]) -> Option<OpenRecipes> {
    let token = argv
        .iter()
        .find(|a| a.starts_with(gtl_recipe::RECIPE_TOKEN_PREFIX))?;
    decode_token(token)
}

/// Managed Tauri state: recipe batches awaiting the frontend.
#[derive(Default)]
pub struct PendingRecipes(pub Mutex<Vec<OpenRecipes>>);

impl PendingRecipes {
    pub fn push(&self, batch: OpenRecipes) {
        self.0.lock().expect("pending-recipes lock").push(batch);
    }

    /// Take and clear everything queued so far.
    pub fn drain(&self) -> Vec<OpenRecipes> {
        std::mem::take(&mut *self.0.lock().expect("pending-recipes lock"))
    }
}

pub(super) fn enqueue_and_wake<E>(
    pending: &PendingRecipes,
    batch: OpenRecipes,
    wake: impl FnOnce(OpenRecipes) -> Result<(), E>,
) {
    pending.push(batch.clone());
    let _ = wake(batch);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_batch() -> OpenRecipes {
        OpenRecipes {
            batch_id: "batch-1".into(),
            recipes: vec![gtl_recipe::Recipe {
                source: gtl_recipe::RecipeSource::LocalRepo("/repos/gt".into()),
                op: gtl_recipe::RecipeOp::SquashPreview,
            }],
        }
    }

    #[test]
    fn extracts_and_decodes_first_recipe_token() {
        let batch = sample_batch();
        let argv = vec!["gtl-viewer".to_string(), gtl_recipe::encode_token(&batch)];
        assert_eq!(recipes_from_argv(&argv), Some(batch));
    }

    #[test]
    fn none_when_argv_has_a_diff_url_instead_of_a_recipe_token() {
        assert_eq!(
            recipes_from_argv(&["gtl-viewer".to_string(), "diff://abc/def".to_string()]),
            None
        );
    }

    #[test]
    fn none_when_no_recipe_token_present() {
        assert_eq!(recipes_from_argv(&["gtl-viewer".to_string()]), None);
    }

    #[test]
    fn none_when_argv_is_empty() {
        assert_eq!(recipes_from_argv(&[]), None);
    }

    #[test]
    fn recipe_queue_pushes_and_drains_fifo_then_empties() {
        let q = PendingRecipes::default();
        let batch = sample_batch();
        q.push(batch.clone());
        assert_eq!(q.drain(), vec![batch]);
        assert!(q.drain().is_empty());
    }

    #[test]
    fn failed_wake_keeps_the_forwarded_batch_available_to_drain() {
        let q = PendingRecipes::default();
        let batch = sample_batch();

        enqueue_and_wake(&q, batch.clone(), |_| Err("event unavailable"));

        assert_eq!(q.drain(), vec![batch]);
    }
}
