use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, anyhow};
use gtl_infra::user_config::TomlSettingsStore;
#[cfg(test)]
use gtl_local_auth::CapabilityToken;
use gtl_local_auth::{LocalAuth, PublishedEndpoint, ServerEndpoint, ServerInstanceId};
use tokio::{sync::oneshot, task::JoinHandle};
#[cfg(test)]
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
};

use crate::{server::serve, state::AppState};

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(250);

pub struct ServerHarness {
    #[cfg(feature = "benchmark-support")]
    auth: LocalAuth,
    published_endpoint: PublishedEndpoint,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<anyhow::Result<()>>,
    #[cfg(test)]
    channel: Channel,
    #[cfg(test)]
    authorization: ServerHarnessAuthorization,
}

impl ServerHarness {
    pub async fn start(data_root: &Path, settings_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let auth = LocalAuth::from_data_root(data_root)?;
        let capability = auth.load_or_create_server_token()?;
        let state = AppState::open_with_settings(data_root, TomlSettingsStore::new(settings_path))?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .context("binding harness gRPC server")?;
        let address = listener
            .local_addr()
            .context("reading harness gRPC server address")?;
        let endpoint = ServerEndpoint::try_new(address, ServerInstanceId::generate())?;
        let published_endpoint = auth.publish_endpoint(endpoint)?;
        #[cfg(test)]
        let authorization = ServerHarnessAuthorization::new(&capability)?;
        let (shutdown, shutdown_receiver) = oneshot::channel();
        let task = tokio::spawn(async move {
            serve(
                listener,
                async move {
                    let _ = shutdown_receiver.await;
                },
                SHUTDOWN_GRACE_PERIOD,
                capability,
                state,
            )
            .await
        });
        #[cfg(test)]
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{address}"))?
            .connect()
            .await?;

        Ok(Self {
            #[cfg(feature = "benchmark-support")]
            auth,
            published_endpoint,
            shutdown: Some(shutdown),
            task,
            #[cfg(test)]
            channel,
            #[cfg(test)]
            authorization,
        })
    }

    #[cfg(feature = "benchmark-support")]
    pub const fn auth(&self) -> &LocalAuth {
        &self.auth
    }

    pub fn begin_shutdown(&mut self) -> anyhow::Result<()> {
        self.shutdown
            .take()
            .context("gRPC server shutdown already started")?
            .send(())
            .map_err(|()| anyhow!("gRPC server stopped before shutdown"))
    }

    pub async fn wait(self) -> anyhow::Result<()> {
        self.task.await??;
        drop(self.published_endpoint);
        Ok(())
    }

    pub async fn stop(mut self) -> anyhow::Result<()> {
        self.begin_shutdown()?;
        self.wait().await
    }

    #[cfg(test)]
    pub(crate) fn channel(&self) -> Channel {
        self.channel.clone()
    }

    #[cfg(test)]
    pub(crate) fn authorization(&self) -> ServerHarnessAuthorization {
        self.authorization.clone()
    }
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct ServerHarnessAuthorization {
    value: MetadataValue<Ascii>,
}

#[cfg(test)]
impl ServerHarnessAuthorization {
    fn new(capability: &CapabilityToken) -> anyhow::Result<Self> {
        Ok(Self {
            value: format!("Bearer {}", capability.expose_secret()).parse()?,
        })
    }
}

#[cfg(test)]
impl Interceptor for ServerHarnessAuthorization {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert("authorization", self.value.clone());
        Ok(request)
    }
}
