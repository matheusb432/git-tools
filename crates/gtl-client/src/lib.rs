//! Authenticated local gRPC client for `gtl-server`.

use std::time::Duration;

use gtl_local_auth::{CapabilityToken, LocalAuth, LocalAuthError, ServerEndpoint};
use tonic::{
    Request, Status,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::{Channel, Endpoint},
};
use tonic_health::pb::{HealthCheckRequest, health_client::HealthClient};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const AUTHORIZATION_METADATA_KEY: &str = "authorization";

#[derive(Debug, thiserror::Error)]
pub enum ConnectError {
    #[error(transparent)]
    LocalBootstrap(#[from] LocalAuthError),
    #[error("local gtl-server endpoint is not a valid URI")]
    InvalidEndpoint(#[source] tonic::transport::Error),
    #[error("local capability token cannot be encoded as gRPC metadata")]
    AuthorizationMetadata(#[source] tonic::metadata::errors::InvalidMetadataValue),
    #[error("could not connect to local gtl-server")]
    Transport(#[source] tonic::transport::Error),
    #[error("local gtl-server health check failed")]
    Health(#[source] Status),
}

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("gtl-server request failed: {0}")]
    Rpc(#[from] Status),
}

#[derive(Debug, Clone)]
pub struct GtlClient {
    channel: Channel,
    authorization: Authorization,
    endpoint: ServerEndpoint,
}

impl GtlClient {
    /// Discovers and authenticates the OS-managed local server.
    pub async fn connect_local() -> Result<Self, ConnectError> {
        let auth = LocalAuth::from_environment()?;
        Self::connect(&auth).await
    }

    /// Discovers and authenticates the server published in `auth`'s data root.
    pub async fn connect(auth: &LocalAuth) -> Result<Self, ConnectError> {
        let endpoint = auth.load_endpoint()?;
        let token = auth.load_client_token()?;
        let client = Self::connect_endpoint(&endpoint, &token).await?;
        client
            .check_health_inner()
            .await
            .map_err(ConnectError::Health)?;
        Ok(client)
    }

    pub async fn check_health(&self) -> Result<(), ClientError> {
        self.check_health_inner().await.map_err(ClientError::from)
    }

    #[must_use]
    pub const fn endpoint(&self) -> &ServerEndpoint {
        &self.endpoint
    }

    async fn connect_endpoint(
        endpoint: &ServerEndpoint,
        token: &CapabilityToken,
    ) -> Result<Self, ConnectError> {
        let channel_endpoint = Endpoint::from_shared(format!("http://{}", endpoint.address()))
            .map_err(ConnectError::InvalidEndpoint)?
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT);
        let channel = channel_endpoint
            .connect()
            .await
            .map_err(ConnectError::Transport)?;
        let authorization = Authorization::try_new(token)?;
        Ok(Self {
            channel,
            authorization,
            endpoint: endpoint.clone(),
        })
    }

    async fn check_health_inner(&self) -> Result<(), Status> {
        let mut client =
            HealthClient::with_interceptor(self.channel.clone(), self.authorization.clone());
        let mut request = Request::new(HealthCheckRequest {
            service: String::new(),
        });
        request.set_timeout(REQUEST_TIMEOUT);
        let response = client.check(request).await?.into_inner();
        if response.status != tonic_health::ServingStatus::Serving as i32 {
            return Err(Status::unavailable("gtl-server is not serving"));
        }
        Ok(())
    }
}

#[derive(Clone)]
struct Authorization {
    value: MetadataValue<Ascii>,
}

impl std::fmt::Debug for Authorization {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Authorization(REDACTED)")
    }
}

impl Authorization {
    fn try_new(token: &CapabilityToken) -> Result<Self, ConnectError> {
        let value = format!("Bearer {}", token.expose_secret())
            .parse()
            .map_err(ConnectError::AuthorizationMetadata)?;
        Ok(Self { value })
    }
}

impl Interceptor for Authorization {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        request
            .metadata_mut()
            .insert(AUTHORIZATION_METADATA_KEY, self.value.clone());
        Ok(request)
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, time::Duration};

    use gtl_local_auth::{ServerEndpoint, ServerInstanceId};
    use tokio::sync::oneshot;
    use tonic::{
        service::InterceptorLayer,
        transport::{Server, server::TcpIncoming},
    };

    use super::*;

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    #[tokio::test]
    async fn connects_through_private_discovery_and_authentication() -> TestResult {
        let harness = TestHarness::start(true).await?;

        let client = GtlClient::connect(&harness.auth).await?;

        client.check_health().await?;
        assert_eq!(client.endpoint().address(), harness.address);
        harness.stop().await?;
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_server_with_a_different_capability() -> TestResult {
        let harness = TestHarness::start(false).await?;

        let error = GtlClient::connect(&harness.auth)
            .await
            .expect_err("mismatched capability must fail");

        assert!(matches!(
            error,
            ConnectError::Health(status) if status.code() == tonic::Code::Unauthenticated
        ));
        harness.stop().await?;
        Ok(())
    }

    struct TestHarness {
        _directory: tempfile::TempDir,
        _published_endpoint: gtl_local_auth::PublishedEndpoint,
        auth: LocalAuth,
        address: std::net::SocketAddr,
        shutdown: oneshot::Sender<()>,
        task: tokio::task::JoinHandle<Result<(), tonic::transport::Error>>,
    }

    impl TestHarness {
        async fn start(matching_capability: bool) -> TestResult<Self> {
            let directory = tempfile::tempdir()?;
            let auth = LocalAuth::from_data_root(directory.path())?;
            let stored_capability = auth.load_or_create_server_token()?;
            let server_capability = if matching_capability {
                stored_capability
            } else {
                CapabilityToken::generate()?
            };
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let endpoint = ServerEndpoint::try_new(address, ServerInstanceId::generate())?;
            let published_endpoint = auth.publish_endpoint(endpoint)?;
            let expected_capability = server_capability;
            let authentication = move |request: Request<()>| {
                let authenticated = request
                    .metadata()
                    .get(AUTHORIZATION_METADATA_KEY)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.strip_prefix("Bearer "))
                    .is_some_and(|candidate| expected_capability.authenticates(candidate));
                if authenticated {
                    Ok(request)
                } else {
                    Err(Status::unauthenticated("authentication required"))
                }
            };
            let (health_reporter, health_service) = tonic_health::server::health_reporter();
            health_reporter
                .set_service_status("", tonic_health::ServingStatus::Serving)
                .await;
            let (shutdown, shutdown_receiver) = oneshot::channel();
            let task = tokio::spawn(async move {
                Server::builder()
                    .layer(InterceptorLayer::new(authentication))
                    .add_service(health_service)
                    .serve_with_incoming_shutdown(TcpIncoming::from(listener), async move {
                        let _ = shutdown_receiver.await;
                    })
                    .await
            });

            Ok(Self {
                _directory: directory,
                _published_endpoint: published_endpoint,
                auth,
                address,
                shutdown,
                task,
            })
        }

        async fn stop(self) -> TestResult {
            self.shutdown
                .send(())
                .map_err(|()| "test server stopped before shutdown")?;
            tokio::time::timeout(Duration::from_secs(1), self.task).await???;
            Ok(())
        }
    }
}
