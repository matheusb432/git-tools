use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context as _, anyhow};
use gtl_infra::user_config::TomlSettingsStore;
#[cfg(test)]
use gtl_local_auth::CapabilityToken;
use gtl_local_auth::{
    LocalAuth, PublishedEndpoint, PublishedViewerBootstrap, ServerEndpoint, ServerInstanceId,
    ViewerBootstrap,
};
use gtl_wire::viewer::VIEWER_PROTOCOL_VERSION;
use tokio::{sync::oneshot, task::JoinHandle};
#[cfg(test)]
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
};

#[cfg(unix)]
use crate::uds_listener::BoundUdsListener;
use crate::{
    server::{ServerListeners, serve},
    state::AppState,
};

const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_millis(250);

pub struct ServerHarness {
    #[cfg(feature = "benchmark-support")]
    auth: LocalAuth,
    #[cfg(feature = "benchmark-support")]
    state: AppState,
    published_endpoint: PublishedEndpoint,
    published_viewer: PublishedViewerBootstrap,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<anyhow::Result<()>>,
    #[cfg(test)]
    native_channel: Channel,
    #[cfg(test)]
    authorization: ServerHarnessAuthorization,
}

impl ServerHarness {
    pub async fn start(data_root: &Path, settings_path: Option<PathBuf>) -> anyhow::Result<Self> {
        let auth = LocalAuth::from_data_root(data_root)?;
        let capability = auth.load_or_create_server_token()?;
        let state = AppState::open_with_settings(data_root, TomlSettingsStore::new(settings_path))?;
        crate::viewer_runtime::restore_saved_live_views(&state)?;
        let instance_id = ServerInstanceId::generate();
        #[cfg(unix)]
        let (endpoint, listeners) = bind_harness_listeners(&auth, instance_id.clone())?;
        #[cfg(windows)]
        let (endpoint, listeners) = bind_harness_listeners(&auth, instance_id.clone()).await?;
        let published_viewer = auth.publish_viewer_bootstrap(&ViewerBootstrap::new(
            instance_id,
            VIEWER_PROTOCOL_VERSION,
        ))?;
        let published_endpoint = auth.publish_endpoint(endpoint.clone())?;
        #[cfg(test)]
        let authorization = ServerHarnessAuthorization::new(&capability)?;
        let (shutdown, shutdown_receiver) = oneshot::channel();
        let server_state = state.clone();
        let task = tokio::spawn(async move {
            serve(
                listeners,
                async move {
                    let _ = shutdown_receiver.await;
                },
                SHUTDOWN_GRACE_PERIOD,
                capability,
                server_state,
            )
            .await
        });
        tokio::task::yield_now().await;
        #[cfg(test)]
        let native_channel = connect_harness_channel(&endpoint).await?;

        Ok(Self {
            #[cfg(feature = "benchmark-support")]
            auth,
            #[cfg(feature = "benchmark-support")]
            state,
            published_endpoint,
            published_viewer,
            shutdown: Some(shutdown),
            task,
            #[cfg(test)]
            native_channel,
            #[cfg(test)]
            authorization,
        })
    }

    #[cfg(feature = "benchmark-support")]
    pub const fn auth(&self) -> &LocalAuth {
        &self.auth
    }

    #[cfg(feature = "benchmark-support")]
    pub fn open_viewer_recipe_batch(
        &self,
        batch: gtl_application::recipes::RecipeBatch,
    ) -> anyhow::Result<()> {
        crate::viewer_runtime::open_recipe_batch(&self.state, batch).map_err(anyhow::Error::from)
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
        drop(self.published_viewer);
        Ok(())
    }

    pub async fn stop(mut self) -> anyhow::Result<()> {
        self.begin_shutdown()?;
        self.wait().await
    }

    #[cfg(test)]
    pub(crate) fn native_channel(&self) -> Channel {
        self.native_channel.clone()
    }

    #[cfg(test)]
    pub(crate) fn authorization(&self) -> ServerHarnessAuthorization {
        self.authorization.clone()
    }
}

#[cfg(unix)]
fn bind_harness_listeners(
    auth: &LocalAuth,
    instance_id: ServerInstanceId,
) -> anyhow::Result<(ServerEndpoint, ServerListeners)> {
    let endpoint = auth.server_endpoint(instance_id)?;
    let native_listener = BoundUdsListener::bind(endpoint.uds_path())?;
    Ok((endpoint, ServerListeners::new(native_listener)))
}

#[cfg(windows)]
async fn bind_harness_listeners(
    _auth: &LocalAuth,
    instance_id: ServerInstanceId,
) -> anyhow::Result<(ServerEndpoint, ServerListeners)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("binding harness gRPC server")?;
    let address = listener
        .local_addr()
        .context("reading harness gRPC server address")?;
    Ok((
        ServerEndpoint::try_new(address, instance_id)?,
        ServerListeners::new(listener),
    ))
}

#[cfg(test)]
async fn connect_harness_channel(endpoint: &ServerEndpoint) -> anyhow::Result<Channel> {
    #[cfg(unix)]
    let native_target = format!(
        "unix://{}",
        endpoint
            .uds_path()
            .to_str()
            .context("harness UDS path is UTF-8")?
    );
    #[cfg(windows)]
    let native_target = format!("http://{}", endpoint.tcp_address());
    let channel = tonic::transport::Endpoint::from_shared(native_target)?
        .connect()
        .await?;
    Ok(channel)
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
