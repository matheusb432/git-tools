use std::time::Duration;

use gtl_application::viewer::{
    refresh_live_view::{self, LiveViewCheck},
    session::PublishOutcome,
};
use gtl_models::viewer::ViewerTabId;
use gtl_wire::v1;
use tokio::sync::mpsc;
use tonic::Status;

use crate::state::AppState;

pub(super) fn spawn(
    state: AppState,
    tab_id: ViewerTabId,
    sender: mpsc::Sender<Result<v1::WatchViewerResponse, Status>>,
) {
    tokio::spawn(async move {
        let started = std::time::Instant::now();
        let mut delay = Duration::ZERO;
        let mut retry_delay = Duration::from_secs(2);
        loop {
            tokio::select! {
                biased;
                () = sender.closed() => break,
                () = tokio::time::sleep(delay) => {},
            }
            let check = tokio::select! {
                biased;
                () = sender.closed() => break,
                result = check(&state, tab_id) => result,
            };
            let error = match check {
                Ok(false) => {
                    delay = Duration::from_secs(2);
                    continue;
                }
                Ok(true) => {
                    retry_delay = Duration::from_secs(2);
                    None
                }
                Err(error) => {
                    tracing::warn!(%error, "live diff update failed");
                    Some(error.to_string().chars().take(2048).collect())
                }
            };
            delay = if error.is_some() {
                let delay = retry_delay;
                retry_delay = (retry_delay * 2).min(Duration::from_secs(5));
                delay
            } else {
                Duration::from_secs(2)
            };
            let Ok(version) = state.viewer.version() else {
                break;
            };
            if sender
                .send(Ok(v1::WatchViewerResponse {
                    version: version.value(),
                    live_check: Some(v1::ViewerLiveCheck {
                        tab_id: tab_id.into(),
                        error,
                        elapsed_ms: u64::try_from(started.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                    }),
                }))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

async fn check(state: &AppState, tab_id: ViewerTabId) -> anyhow::Result<bool> {
    let permit = state.live_refresh_permits.clone().acquire_owned().await?;
    let worker = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        refresh_live_view::prepare(
            tab_id,
            &worker.viewer,
            &worker.user_settings,
            &worker.git,
            &worker.database,
        )
    })
    .await??;
    match result {
        LiveViewCheck::Inactive | LiveViewCheck::ChangedDuringComputation => Ok(false),
        LiveViewCheck::Unchanged => Ok(true),
        LiveViewCheck::Prepared(work) => {
            Ok(refresh_live_view::publish(work, &state.viewer)? == PublishOutcome::Published)
        }
    }
}
