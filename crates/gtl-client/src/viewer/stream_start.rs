use super::ViewerClientError;

pub(super) async fn complete<Stream>(
    start: impl std::future::Future<Output = Result<Stream, ViewerClientError>>,
    recipient: futures_channel::oneshot::Sender<Result<Stream, ViewerClientError>>,
) {
    // Failed delivery drops the stream owner, including when native startup finishes late.
    let _ = recipient.send(start.await);
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct StreamOwner(Arc<AtomicUsize>);

    impl Drop for StreamOwner {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    async fn delayed_start(
        start: futures_channel::oneshot::Receiver<()>,
        owned: Arc<AtomicUsize>,
    ) -> Result<StreamOwner, ViewerClientError> {
        start.await.map_err(|_| ViewerClientError::Unavailable)?;
        owned.fetch_add(1, Ordering::SeqCst);
        Ok(StreamOwner(owned))
    }

    #[tokio::test]
    async fn abandoned_starts_release_late_handles_beyond_the_registry_capacity() {
        let active = Arc::new(AtomicUsize::new(0));
        for _ in 0..25 {
            let (ready, start) = futures_channel::oneshot::channel();
            let (recipient, waiting) = futures_channel::oneshot::channel();
            let owned = active.clone();
            let task = tokio::spawn(complete(delayed_start(start, owned), recipient));
            drop(waiting);
            ready.send(()).unwrap();
            task.await.unwrap();
            assert_eq!(active.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn delivered_streams_remain_owned_until_the_consumer_drops_them() {
        let active = Arc::new(AtomicUsize::new(1));
        let (recipient, waiting) = futures_channel::oneshot::channel();
        complete(
            std::future::ready(Ok(StreamOwner(active.clone()))),
            recipient,
        )
        .await;
        let stream = waiting.await.unwrap().unwrap();
        assert_eq!(active.load(Ordering::SeqCst), 1);
        drop(stream);
        assert_eq!(active.load(Ordering::SeqCst), 0);
    }
}
