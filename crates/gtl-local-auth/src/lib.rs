//! Private local bootstrap credentials and endpoint discovery for `gtl-server`.

mod endpoint;
mod error;
mod private_directory;
mod token;
mod viewer_bootstrap;

use std::path::{Path, PathBuf};

use directories::ProjectDirs;
pub use endpoint::{PublishedEndpoint, ServerEndpoint, ServerInstanceId};
pub use error::LocalAuthError;
pub use token::CapabilityToken;
pub use viewer_bootstrap::{PublishedViewerBootstrap, ViewerBootstrap};

const DATA_DIRECTORY_ENVIRONMENT_VARIABLE: &str = "GIT_TOOLS_DATA_DIR";
const SERVER_DIRECTORY_NAME: &str = "server";

#[derive(Debug, Clone)]
pub struct LocalAuth {
    data_root: PathBuf,
}

impl LocalAuth {
    /// Resolves the local bootstrap store from the process environment.
    pub fn from_environment() -> Result<Self, LocalAuthError> {
        let data_root = std::env::var_os(DATA_DIRECTORY_ENVIRONMENT_VARIABLE)
            .map(PathBuf::from)
            .or_else(|| {
                ProjectDirs::from("", "", "git-tools")
                    .map(|directories| directories.data_dir().to_path_buf())
            })
            .ok_or(LocalAuthError::DataDirectoryUnavailable)?;

        Self::from_data_root(data_root)
    }

    /// Uses an explicit application data root.
    pub fn from_data_root(data_root: impl Into<PathBuf>) -> Result<Self, LocalAuthError> {
        let data_root = data_root.into();
        if !data_root.is_absolute() {
            return Err(LocalAuthError::DataDirectoryRelative { path: data_root });
        }
        Ok(Self { data_root })
    }

    /// Loads the persistent capability, creating it when the server first starts.
    pub fn load_or_create_server_token(&self) -> Result<CapabilityToken, LocalAuthError> {
        token::load_or_create(&self.server_directory()?)
    }

    /// Loads the capability provisioned by `gtl-server`.
    pub fn load_client_token(&self) -> Result<CapabilityToken, LocalAuthError> {
        token::load(&self.server_directory()?)
    }

    /// Resolves the deterministic native gRPC endpoint within the private server directory.
    #[cfg(unix)]
    pub fn server_endpoint(
        &self,
        instance_id: ServerInstanceId,
    ) -> Result<ServerEndpoint, LocalAuthError> {
        endpoint::for_directory(&self.server_directory()?, instance_id)
    }

    /// Publishes the currently listening server endpoint.
    pub fn publish_endpoint(
        &self,
        endpoint: ServerEndpoint,
    ) -> Result<PublishedEndpoint, LocalAuthError> {
        endpoint::publish(self.server_directory()?, endpoint)
    }

    /// Loads the currently published server endpoint.
    pub fn load_endpoint(&self) -> Result<ServerEndpoint, LocalAuthError> {
        endpoint::load(&self.server_directory()?)
    }

    /// Publishes the viewer protocol metadata for the active server instance.
    pub fn publish_viewer_bootstrap(
        &self,
        bootstrap: &ViewerBootstrap,
    ) -> Result<PublishedViewerBootstrap, LocalAuthError> {
        viewer_bootstrap::publish(self.server_directory()?, bootstrap)
    }

    /// Loads viewer protocol metadata for the active server instance.
    pub fn load_viewer_bootstrap(&self) -> Result<ViewerBootstrap, LocalAuthError> {
        let directory = self.server_directory()?;
        let bootstrap = viewer_bootstrap::load(&directory)?;
        let endpoint = endpoint::load(&directory)?;
        if bootstrap.instance_id() != endpoint.instance_id() {
            return Err(LocalAuthError::MalformedViewerBootstrap {
                path: directory.path(viewer_bootstrap::VIEWER_BOOTSTRAP_FILE_NAME),
            });
        }
        Ok(bootstrap)
    }

    #[must_use]
    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    fn server_directory(&self) -> Result<private_directory::PrivateDirectory, LocalAuthError> {
        private_directory::PrivateDirectory::open(self.data_root.join(SERVER_DIRECTORY_NAME))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_auth(directory: &tempfile::TempDir) -> LocalAuth {
        LocalAuth::from_data_root(directory.path()).unwrap()
    }

    #[cfg(unix)]
    fn server_endpoint(auth: &LocalAuth, instance_id: ServerInstanceId) -> ServerEndpoint {
        auth.server_endpoint(instance_id).unwrap()
    }

    #[cfg(windows)]
    fn server_endpoint(_auth: &LocalAuth, instance_id: ServerInstanceId) -> ServerEndpoint {
        ServerEndpoint::try_new("127.0.0.1:4318".parse::<SocketAddr>().unwrap(), instance_id)
            .unwrap()
    }

    #[test]
    fn capability_persists_across_server_restarts() {
        let directory = tempfile::tempdir().unwrap();
        let auth = local_auth(&directory);

        let provisioned = auth.load_or_create_server_token().unwrap();
        let restarted = auth.load_or_create_server_token().unwrap();
        let client = auth.load_client_token().unwrap();

        assert!(provisioned.authenticates(restarted.expose_secret()));
        assert!(provisioned.authenticates(client.expose_secret()));
        assert_eq!(format!("{provisioned:?}"), "CapabilityToken(REDACTED)");
    }

    #[test]
    fn endpoint_publication_is_instance_owned() {
        let directory = tempfile::tempdir().unwrap();
        let auth = local_auth(&directory);
        let endpoint = server_endpoint(&auth, ServerInstanceId::generate());

        let published = auth.publish_endpoint(endpoint.clone()).unwrap();
        assert_eq!(auth.load_endpoint().unwrap(), endpoint);
        let record: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                auth.data_root()
                    .join(SERVER_DIRECTORY_NAME)
                    .join(endpoint::ENDPOINT_FILE_NAME),
            )
            .unwrap(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            assert_eq!(record["transport"], "uds");
            assert_eq!(record["path"].as_str(), endpoint.uds_path().to_str());
            assert!(record.get("address").is_none());
        }
        #[cfg(windows)]
        {
            assert_eq!(record["transport"], "tcp");
            assert_eq!(
                record["address"].as_str(),
                Some(endpoint.tcp_address().to_string().as_str())
            );
            assert!(record.get("path").is_none());
        }

        drop(published);
        assert!(matches!(
            auth.load_endpoint(),
            Err(LocalAuthError::EndpointNotPublished { .. })
        ));
    }

    #[test]
    fn viewer_bootstrap_must_match_the_active_server_instance() {
        let directory = tempfile::tempdir().unwrap();
        let auth = local_auth(&directory);
        let endpoint = server_endpoint(&auth, ServerInstanceId::generate());
        let _published_endpoint = auth.publish_endpoint(endpoint).unwrap();
        let bootstrap = ViewerBootstrap::new(ServerInstanceId::generate(), 1);
        let _published_viewer = auth.publish_viewer_bootstrap(&bootstrap).unwrap();

        assert!(matches!(
            auth.load_viewer_bootstrap(),
            Err(LocalAuthError::MalformedViewerBootstrap { .. })
        ));
    }

    #[cfg(windows)]
    #[test]
    fn endpoint_rejects_non_loopback_and_unbound_addresses() {
        let instance_id = ServerInstanceId::generate();

        assert!(
            ServerEndpoint::try_new("192.0.2.1:4317".parse().unwrap(), instance_id.clone())
                .is_err()
        );
        assert!(ServerEndpoint::try_new("127.0.0.1:0".parse().unwrap(), instance_id).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn endpoint_rejects_relative_and_overlong_uds_paths() {
        let instance_id = ServerInstanceId::generate();
        assert!(matches!(
            ServerEndpoint::try_new("native-grpc.sock", instance_id.clone()),
            Err(LocalAuthError::ServerEndpointPathRelative { .. })
        ));

        let overlong = std::path::Path::new("/").join("x".repeat(512));
        assert!(matches!(
            ServerEndpoint::try_new(overlong, instance_id),
            Err(LocalAuthError::ServerEndpointPathTooLong { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn capability_and_endpoint_are_user_private() {
        use std::os::unix::fs::MetadataExt as _;

        let directory = tempfile::tempdir().unwrap();
        let auth = local_auth(&directory);
        auth.load_or_create_server_token().unwrap();
        let instance_id = ServerInstanceId::generate();
        let endpoint = server_endpoint(&auth, instance_id.clone());
        let _published = auth.publish_endpoint(endpoint.clone()).unwrap();
        let server_directory = auth.data_root().join(SERVER_DIRECTORY_NAME);

        assert_eq!(
            std::fs::metadata(&server_directory).unwrap().mode() & 0o777,
            0o700
        );
        let viewer_bootstrap = ViewerBootstrap::new(instance_id, 1);
        let _published_viewer = auth.publish_viewer_bootstrap(&viewer_bootstrap).unwrap();
        let loaded_viewer = auth.load_viewer_bootstrap().unwrap();
        assert_eq!(loaded_viewer.instance_id(), endpoint.instance_id());
        assert_eq!(loaded_viewer.protocol_version(), 1);
        for name in [
            token::CAPABILITY_FILE_NAME,
            endpoint::ENDPOINT_FILE_NAME,
            viewer_bootstrap::VIEWER_BOOTSTRAP_FILE_NAME,
        ] {
            assert_eq!(
                std::fs::metadata(server_directory.join(name))
                    .unwrap()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }
}
