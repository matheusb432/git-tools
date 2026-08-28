use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LocalAuthError, ServerInstanceId, private_directory::PrivateDirectory};

pub(crate) const VIEWER_BOOTSTRAP_FILE_NAME: &str = "viewer.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewerBootstrap {
    instance_id: ServerInstanceId,
    protocol_version: u32,
}

impl ViewerBootstrap {
    #[must_use]
    pub const fn new(instance_id: ServerInstanceId, protocol_version: u32) -> Self {
        Self {
            instance_id,
            protocol_version,
        }
    }

    #[must_use]
    pub const fn instance_id(&self) -> &ServerInstanceId {
        &self.instance_id
    }

    #[must_use]
    pub const fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewerBootstrapRecord {
    instance_id: Uuid,
    protocol_version: u32,
}

impl From<&ViewerBootstrap> for ViewerBootstrapRecord {
    fn from(bootstrap: &ViewerBootstrap) -> Self {
        Self {
            instance_id: bootstrap.instance_id.as_uuid(),
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
        if current.instance_id() == &self.instance_id {
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
        instance_id: bootstrap.instance_id.clone(),
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
    Ok(ViewerBootstrap::new(
        ServerInstanceId::from_uuid(record.instance_id),
        record.protocol_version,
    ))
}
