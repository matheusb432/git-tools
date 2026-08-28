use std::{
    fs::{self, Metadata},
    io,
    os::unix::fs::{FileTypeExt as _, MetadataExt as _, PermissionsExt as _},
    path::{Path, PathBuf},
    pin::Pin,
    task::{Context, Poll},
};

use tokio::net::{UnixListener, UnixStream};

const SOCKET_MODE: u32 = 0o600;

#[derive(Debug, thiserror::Error)]
pub(crate) enum UdsBindError {
    #[error("native gRPC UDS path is a symbolic link: {}", path.display())]
    SymbolicLink { path: PathBuf },
    #[error("native gRPC UDS path is not a socket: {}", path.display())]
    NotSocket { path: PathBuf },
    #[error(
        "native gRPC UDS has owner {actual_uid}, expected effective user {expected_uid}: {}",
        path.display()
    )]
    WrongOwner {
        path: PathBuf,
        actual_uid: u32,
        expected_uid: u32,
    },
    #[error(
        "native gRPC UDS has mode {actual_mode:#o}, expected {SOCKET_MODE:#o}: {}",
        path.display()
    )]
    UnsafePermissions { path: PathBuf, actual_mode: u32 },
    #[error("another gtl-server is already listening at {}", path.display())]
    AlreadyListening { path: PathBuf },
    #[error("failed to inspect native gRPC UDS path {}", path.display())]
    Inspect {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to probe native gRPC UDS path {}", path.display())]
    Probe {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to remove stale native gRPC UDS {}", path.display())]
    RemoveStale {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to bind native gRPC UDS {}", path.display())]
    Bind {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to set native gRPC UDS permissions on {}", path.display())]
    SetPermissions {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

pub(crate) struct BoundUdsListener {
    listener: UnixListener,
    path: PathBuf,
    identity: SocketIdentity,
}

impl BoundUdsListener {
    pub(crate) fn bind(path: &Path) -> Result<Self, UdsBindError> {
        prepare_path(path)?;
        let listener = UnixListener::bind(path).map_err(|source| UdsBindError::Bind {
            path: path.to_path_buf(),
            source,
        })?;
        let metadata = match inspect(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                drop(listener);
                let _ = fs::remove_file(path);
                return Err(error);
            }
        };
        let identity = SocketIdentity::from(&metadata);
        if let Err(source) = fs::set_permissions(path, fs::Permissions::from_mode(SOCKET_MODE)) {
            remove_if_same_socket(path, identity);
            return Err(UdsBindError::SetPermissions {
                path: path.to_path_buf(),
                source,
            });
        }
        let verification = inspect(path).and_then(|metadata| verify_socket(path, &metadata));
        if let Err(error) = verification {
            remove_if_same_socket(path, identity);
            return Err(error);
        }
        Ok(Self {
            listener,
            path: path.to_path_buf(),
            identity,
        })
    }
}

impl tokio_stream::Stream for BoundUdsListener {
    type Item = io::Result<UnixStream>;

    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let listener = &self.get_mut().listener;
        match listener.poll_accept(context) {
            Poll::Ready(Ok((stream, _address))) => Poll::Ready(Some(Ok(stream))),
            Poll::Ready(Err(error)) => Poll::Ready(Some(Err(error))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Drop for BoundUdsListener {
    fn drop(&mut self) {
        remove_if_same_socket(&self.path, self.identity);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SocketIdentity {
    device: u64,
    inode: u64,
}

impl From<&Metadata> for SocketIdentity {
    fn from(metadata: &Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
        }
    }
}

fn prepare_path(path: &Path) -> Result<(), UdsBindError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => {
            return Err(UdsBindError::Inspect {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    verify_socket(path, &metadata)?;
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_stream) => Err(UdsBindError::AlreadyListening {
            path: path.to_path_buf(),
        }),
        Err(source) if source.kind() == io::ErrorKind::ConnectionRefused => fs::remove_file(path)
            .map_err(|source| UdsBindError::RemoveStale {
                path: path.to_path_buf(),
                source,
            }),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(UdsBindError::Probe {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn inspect(path: &Path) -> Result<Metadata, UdsBindError> {
    fs::symlink_metadata(path).map_err(|source| UdsBindError::Inspect {
        path: path.to_path_buf(),
        source,
    })
}

fn verify_socket(path: &Path, metadata: &Metadata) -> Result<(), UdsBindError> {
    if metadata.file_type().is_symlink() {
        return Err(UdsBindError::SymbolicLink {
            path: path.to_path_buf(),
        });
    }
    if !metadata.file_type().is_socket() {
        return Err(UdsBindError::NotSocket {
            path: path.to_path_buf(),
        });
    }
    // SAFETY: geteuid has no preconditions and only reads the process credential.
    let expected_uid = unsafe { libc::geteuid() };
    if metadata.uid() != expected_uid {
        return Err(UdsBindError::WrongOwner {
            path: path.to_path_buf(),
            actual_uid: metadata.uid(),
            expected_uid,
        });
    }
    let actual_mode = metadata.mode() & 0o777;
    if actual_mode != SOCKET_MODE {
        return Err(UdsBindError::UnsafePermissions {
            path: path.to_path_buf(),
            actual_mode,
        });
    }
    Ok(())
}

fn remove_if_same_socket(path: &Path, expected: SocketIdentity) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_socket() && SocketIdentity::from(&metadata) == expected {
        let _ = fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn binds_private_socket_and_removes_it_on_drop() {
        let directory = tempfile::tempdir().expect("temporary socket directory");
        let path = directory.path().join("native-grpc.sock");
        let listener = BoundUdsListener::bind(&path).expect("bind UDS");

        assert_eq!(
            fs::symlink_metadata(&path).expect("socket metadata").mode() & 0o777,
            SOCKET_MODE
        );
        assert!(matches!(
            BoundUdsListener::bind(&path),
            Err(UdsBindError::AlreadyListening { .. })
        ));

        drop(listener);
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn replaces_a_stale_socket() {
        let directory = tempfile::tempdir().expect("temporary socket directory");
        let path = directory.path().join("native-grpc.sock");
        let stale = std::os::unix::net::UnixListener::bind(&path).expect("bind stale socket");
        fs::set_permissions(&path, fs::Permissions::from_mode(SOCKET_MODE))
            .expect("set stale socket permissions");
        drop(stale);

        let listener = BoundUdsListener::bind(&path).expect("replace stale UDS");

        drop(listener);
        assert!(!path.exists());
    }

    #[test]
    fn refuses_a_non_socket_path() {
        let directory = tempfile::tempdir().expect("temporary socket directory");
        let path = directory.path().join("native-grpc.sock");
        fs::write(&path, b"not a socket").expect("write conflicting file");

        assert!(matches!(
            BoundUdsListener::bind(&path),
            Err(UdsBindError::NotSocket { .. })
        ));
    }

    #[tokio::test]
    async fn cleanup_does_not_remove_a_replacement_socket() {
        let directory = tempfile::tempdir().expect("temporary socket directory");
        let path = directory.path().join("native-grpc.sock");
        let listener = BoundUdsListener::bind(&path).expect("bind original UDS");
        fs::remove_file(&path).expect("unlink original UDS");
        let replacement =
            std::os::unix::net::UnixListener::bind(&path).expect("bind replacement UDS");

        drop(listener);

        assert!(path.exists());
        drop(replacement);
    }
}
