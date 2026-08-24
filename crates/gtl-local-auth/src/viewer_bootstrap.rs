use std::{net::SocketAddr, sync::Arc};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    CapabilityToken, LocalAuthError, ServerEndpoint, ServerInstanceId,
    private_directory::PrivateDirectory,
};

pub(crate) const VIEWER_BOOTSTRAP_FILE_NAME: &str = "viewer.json";

#[derive(Clone)]
pub struct ViewerBootstrap {
    endpoint: ServerEndpoint,
    capability: CapabilityToken,
    protocol_version: u32,
}

impl ViewerBootstrap {
    #[must_use]
    pub const fn new(
        endpoint: ServerEndpoint,
        capability: CapabilityToken,
        protocol_version: u32,
    ) -> Self {
        Self {
            endpoint,
            capability,
            protocol_version,
        }
    }

    #[must_use]
    pub const fn endpoint(&self) -> &ServerEndpoint {
        &self.endpoint
    }

    #[must_use]
    pub const fn capability(&self) -> &CapabilityToken {
        &self.capability
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

impl std::fmt::Debug for ViewerBootstrap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ViewerBootstrap")
            .field("endpoint", &self.endpoint)
            .field("capability", &"REDACTED")
            .field("protocol_version", &self.protocol_version)
            .finish()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ViewerBootstrapRecord {
    address: SocketAddr,
    instance_id: Uuid,
    capability: String,
    protocol_version: u32,
}

impl From<&ViewerBootstrap> for ViewerBootstrapRecord {
    fn from(bootstrap: &ViewerBootstrap) -> Self {
        Self {
            address: bootstrap.endpoint.address(),
            instance_id: bootstrap.endpoint.instance_id().as_uuid(),
            capability: bootstrap.capability.expose_secret().to_owned(),
            protocol_version: bootstrap.protocol_version,
        }
    }
}

pub struct PublishedViewerBootstrap {
    directory: Arc<PrivateDirectory>,
    instance_id: ServerInstanceId,
}

impl Drop for PublishedViewerBootstrap {
    fn drop(&mut self) {
        let Ok(current) = load(&self.directory) else {
            return;
        };
        if current.endpoint.instance_id() == &self.instance_id {
            let _ = self.directory.remove(VIEWER_BOOTSTRAP_FILE_NAME);
        }
    }
}

pub(crate) fn publish(
    directory: PrivateDirectory,
    bootstrap: &ViewerBootstrap,
) -> Result<PublishedViewerBootstrap, LocalAuthError> {
    let path = directory.path(VIEWER_BOOTSTRAP_FILE_NAME);
    let contents = serde_json::to_vec(&ViewerBootstrapRecord::from(bootstrap))
        .map_err(|_| LocalAuthError::MalformedViewerBootstrap { path })?;
    directory.write_and_replace(VIEWER_BOOTSTRAP_FILE_NAME, &contents)?;
    Ok(PublishedViewerBootstrap {
        directory: Arc::new(directory),
        instance_id: bootstrap.endpoint.instance_id().clone(),
    })
}

pub(crate) fn load(directory: &PrivateDirectory) -> Result<ViewerBootstrap, LocalAuthError> {
    let path = directory.path(VIEWER_BOOTSTRAP_FILE_NAME);
    let contents = match directory.read(VIEWER_BOOTSTRAP_FILE_NAME) {
        Ok(contents) => contents,
        Err(LocalAuthError::Inspect { source, .. })
            if source.kind() == std::io::ErrorKind::NotFound =>
        {
            return Err(LocalAuthError::ViewerBootstrapNotPublished { path });
        }
        Err(error) => return Err(error),
    };
    let record = serde_json::from_slice::<ViewerBootstrapRecord>(&contents)
        .map_err(|_| LocalAuthError::MalformedViewerBootstrap { path: path.clone() })?;
    let endpoint = ServerEndpoint::try_new(
        record.address,
        ServerInstanceId::from_uuid(record.instance_id),
    )
    .map_err(|_| LocalAuthError::MalformedViewerBootstrap { path: path.clone() })?;
    let capability = CapabilityToken::parse(record.capability.as_bytes())
        .ok_or(LocalAuthError::MalformedViewerBootstrap { path })?;
    Ok(ViewerBootstrap::new(
        endpoint,
        capability,
        record.protocol_version,
    ))
}
