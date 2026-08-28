#[cfg(windows)]
use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(unix)]
use std::{
    os::unix::ffi::OsStrExt as _,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LocalAuthError, private_directory::PrivateDirectory};

#[cfg(not(any(unix, windows)))]
compile_error!("gtl-local-auth supports native server endpoints only on Unix and Windows");

pub(crate) const ENDPOINT_FILE_NAME: &str = "endpoint.json";
#[cfg(unix)]
pub(crate) const NATIVE_GRPC_SOCKET_FILE_NAME: &str = "native-grpc.sock";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInstanceId(Uuid);

impl ServerInstanceId {
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }

    pub(crate) const fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    pub(crate) const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for ServerInstanceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEndpoint {
    path: PathBuf,
    instance_id: ServerInstanceId,
}

#[cfg(unix)]
impl ServerEndpoint {
    pub fn try_new(
        path: impl Into<PathBuf>,
        instance_id: ServerInstanceId,
    ) -> Result<Self, LocalAuthError> {
        let path = path.into();
        if !path.is_absolute() {
            return Err(LocalAuthError::ServerEndpointPathRelative { path });
        }
        let path_bytes = path.as_os_str().as_bytes();
        if path_bytes.contains(&0) {
            return Err(LocalAuthError::ServerEndpointPathContainsNul { path });
        }
        if path.to_str().is_none() {
            return Err(LocalAuthError::ServerEndpointPathNotUtf8 { path });
        }
        let path_bytes = path_bytes.len();
        let maximum_bytes = unix_socket_path_maximum_bytes();
        if path_bytes > maximum_bytes {
            return Err(LocalAuthError::ServerEndpointPathTooLong {
                path,
                bytes: path_bytes,
                maximum_bytes,
            });
        }
        Ok(Self { path, instance_id })
    }

    #[must_use]
    pub fn uds_path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub const fn instance_id(&self) -> &ServerInstanceId {
        &self.instance_id
    }
}

#[cfg(unix)]
const fn unix_socket_path_maximum_bytes() -> usize {
    size_of::<libc::sockaddr_un>()
        - std::mem::offset_of!(libc::sockaddr_un, sun_path)
        - size_of::<libc::c_char>()
}

#[cfg(windows)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEndpoint {
    address: SocketAddr,
    instance_id: ServerInstanceId,
}

#[cfg(windows)]
impl ServerEndpoint {
    pub fn try_new(
        address: SocketAddr,
        instance_id: ServerInstanceId,
    ) -> Result<Self, LocalAuthError> {
        if !address.ip().is_loopback() || address.port() == 0 {
            return Err(LocalAuthError::InvalidServerEndpoint);
        }
        Ok(Self {
            address,
            instance_id,
        })
    }

    #[must_use]
    pub const fn tcp_address(&self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub const fn instance_id(&self) -> &ServerInstanceId {
        &self.instance_id
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum ServerTransport {
    #[serde(rename = "uds")]
    Uds,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum ServerTransport {
    #[serde(rename = "tcp")]
    Tcp,
}

#[cfg(unix)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointRecord {
    transport: ServerTransport,
    path: PathBuf,
    instance_id: Uuid,
}

#[cfg(windows)]
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointRecord {
    transport: ServerTransport,
    address: SocketAddr,
    instance_id: Uuid,
}

#[cfg(unix)]
impl From<ServerEndpoint> for EndpointRecord {
    fn from(endpoint: ServerEndpoint) -> Self {
        Self {
            transport: ServerTransport::Uds,
            path: endpoint.path,
            instance_id: endpoint.instance_id.0,
        }
    }
}

#[cfg(windows)]
impl From<ServerEndpoint> for EndpointRecord {
    fn from(endpoint: ServerEndpoint) -> Self {
        Self {
            transport: ServerTransport::Tcp,
            address: endpoint.address,
            instance_id: endpoint.instance_id.0,
        }
    }
}

#[cfg(unix)]
impl TryFrom<EndpointRecord> for ServerEndpoint {
    type Error = LocalAuthError;

    fn try_from(record: EndpointRecord) -> Result<Self, Self::Error> {
        Self::try_new(record.path, ServerInstanceId(record.instance_id))
    }
}

#[cfg(windows)]
impl TryFrom<EndpointRecord> for ServerEndpoint {
    type Error = LocalAuthError;

    fn try_from(record: EndpointRecord) -> Result<Self, Self::Error> {
        Self::try_new(record.address, ServerInstanceId(record.instance_id))
    }
}

pub struct PublishedEndpoint {
    directory: Arc<PrivateDirectory>,
    instance_id: ServerInstanceId,
}

impl std::fmt::Debug for PublishedEndpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PublishedEndpoint")
            .field("instance_id", &self.instance_id)
            .finish_non_exhaustive()
    }
}

impl Drop for PublishedEndpoint {
    fn drop(&mut self) {
        let Ok(current) = load(&self.directory) else {
            return;
        };
        if current.instance_id() == &self.instance_id {
            let _ = self.directory.remove(ENDPOINT_FILE_NAME);
        }
    }
}

#[cfg(unix)]
pub(crate) fn for_directory(
    directory: &PrivateDirectory,
    instance_id: ServerInstanceId,
) -> Result<ServerEndpoint, LocalAuthError> {
    ServerEndpoint::try_new(directory.path(NATIVE_GRPC_SOCKET_FILE_NAME), instance_id)
}

pub(crate) fn publish(
    directory: PrivateDirectory,
    endpoint: ServerEndpoint,
) -> Result<PublishedEndpoint, LocalAuthError> {
    #[cfg(unix)]
    ensure_expected_path(&directory, &endpoint)?;
    let instance_id = endpoint.instance_id().clone();
    let contents = serde_json::to_vec(&EndpointRecord::from(endpoint)).map_err(|_| {
        LocalAuthError::MalformedEndpoint {
            path: directory.path(ENDPOINT_FILE_NAME),
        }
    })?;
    directory.write_and_replace(ENDPOINT_FILE_NAME, &contents)?;
    Ok(PublishedEndpoint {
        directory: Arc::new(directory),
        instance_id,
    })
}

pub(crate) fn load(directory: &PrivateDirectory) -> Result<ServerEndpoint, LocalAuthError> {
    let path = directory.path(ENDPOINT_FILE_NAME);
    let contents = match directory.read(ENDPOINT_FILE_NAME) {
        Ok(contents) => contents,
        Err(LocalAuthError::Inspect { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            return Err(LocalAuthError::EndpointNotPublished { path });
        }
        Err(error) => return Err(error),
    };
    let record = serde_json::from_slice::<EndpointRecord>(&contents)
        .map_err(|_| LocalAuthError::MalformedEndpoint { path: path.clone() })?;
    let endpoint = ServerEndpoint::try_from(record)
        .map_err(|_| LocalAuthError::MalformedEndpoint { path: path.clone() })?;
    #[cfg(unix)]
    ensure_expected_path(directory, &endpoint)
        .map_err(|_| LocalAuthError::MalformedEndpoint { path })?;
    Ok(endpoint)
}

#[cfg(unix)]
fn ensure_expected_path(
    directory: &PrivateDirectory,
    endpoint: &ServerEndpoint,
) -> Result<(), LocalAuthError> {
    let expected = directory.path(NATIVE_GRPC_SOCKET_FILE_NAME);
    if endpoint.uds_path() != expected {
        return Err(LocalAuthError::UnexpectedServerEndpointPath {
            path: endpoint.uds_path().to_path_buf(),
            expected,
        });
    }
    Ok(())
}
