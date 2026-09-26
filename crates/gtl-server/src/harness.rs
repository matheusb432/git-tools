use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, anyhow};
use gtl_infra::user_config::TomlSettingsStore;
use gtl_local_transport::{LocalEndpoint, LocalListener};
use tokio::{sync::oneshot, task::JoinHandle};
#[cfg(test)]
use tonic::transport::Channel;

use crate::{server::serve, state::AppState};

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(250);

pub struct ServerHarness {
    #[cfg(feature = "benchmark-support")]
    endpoint: LocalEndpoint,
    #[cfg(any(test, feature = "benchmark-support"))]
    state: AppState,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<anyhow::Result<()>>,
    tab_saving: crate::viewer_runtime::ViewerTabSaving,
    #[cfg(test)]
    native_channel: Channel,
}

impl ServerHarness {
    pub async fn start(data_root: &Path, settings_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let endpoint = LocalEndpoint::from_root(data_root)?;
        let listener = LocalListener::bind(&endpoint, Duration::from_secs(1)).await?;
        let state = AppState::open_with_settings(data_root, TomlSettingsStore::new(settings_path))?;
        let tab_saving = crate::viewer_runtime::restore_viewer_tabs(&state)?;
        let (shutdown, shutdown_receiver) = oneshot::channel();
        let server_state = state.clone();
        let task = tokio::spawn(serve(
            listener,
            wait_for_shutdown(shutdown_receiver),
            SHUTDOWN_GRACE_PERIOD,
            server_state,
        ));
        tokio::task::yield_now().await;
        #[cfg(test)]
        let native_channel = connect_harness_channel(&endpoint).await?;

        Ok(Self {
            #[cfg(feature = "benchmark-support")]
            endpoint: endpoint.clone(),
            #[cfg(any(test, feature = "benchmark-support"))]
            state,
            shutdown: Some(shutdown),
            task,
            tab_saving,
            #[cfg(test)]
            native_channel,
        })
    }

    #[cfg(feature = "benchmark-support")]
    #[must_use]
    pub const fn endpoint(&self) -> &LocalEndpoint {
        &self.endpoint
    }

    #[cfg(feature = "benchmark-support")]
    pub fn open_viewer_recipe_batch(
        &self,
        batch: gtl_application::recipes::RecipeBatch,
    ) -> anyhow::Result<()> {
        crate::viewer_runtime::open_recipe_batch(&self.state, batch).map_err(anyhow::Error::from)
    }

    #[cfg(any(test, feature = "benchmark-support"))]
    #[must_use]
    pub fn project_status_observations(&self) -> (u64, usize) {
        use std::sync::atomic::Ordering;
        (
            self.state
                .viewer_project_status_checks
                .load(Ordering::Relaxed),
            self.state
                .viewer_project_watch_registrations
                .load(Ordering::Relaxed),
        )
    }

    pub fn begin_shutdown(&mut self) -> anyhow::Result<()> {
        self.shutdown
            .take()
            .context("gRPC server shutdown already started")?
            .send(())
            .map_err(|()| anyhow!("gRPC server stopped before shutdown"))
    }

    pub async fn wait(self) -> anyhow::Result<()> {
        let served = self.task.await;
        self.tab_saving.stop().await;
        served??;
        Ok(())
    }

    pub async fn stop(mut self) -> anyhow::Result<()> {
        self.begin_shutdown()?;
        self.wait().await
    }

    #[cfg(test)]
    pub(crate) fn project_status_workers(&self) -> std::sync::Arc<tokio::sync::Semaphore> {
        self.state.viewer_project_status_workers.clone()
    }

    #[cfg(test)]
    pub(crate) fn native_channel(&self) -> Channel {
        self.native_channel.clone()
    }
}

async fn wait_for_shutdown(shutdown_receiver: oneshot::Receiver<()>) {
    let _ = shutdown_receiver.await;
}

#[cfg(test)]
async fn connect_harness_channel(endpoint: &LocalEndpoint) -> anyhow::Result<Channel> {
    endpoint
        .connect(Duration::from_secs(1), Duration::from_secs(30))
        .await
        .map_err(anyhow::Error::from)
}
