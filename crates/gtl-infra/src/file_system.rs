use std::path::{Path, PathBuf};

use gtl_application::ports::{
    FileSystemClient, FileSystemClientError, FileSystemClientErrorKind, FileSystemEntryKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileSystemClient;

impl FileSystemClient for LocalFileSystemClient {
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FileSystemClientError> {
        std::fs::canonicalize(path).map_err(|error| map_error(path, &error))
    }

    fn entry_kind(&self, path: &Path) -> Result<FileSystemEntryKind, FileSystemClientError> {
        std::fs::metadata(path)
            .map(|metadata| {
                if metadata.is_file() {
                    FileSystemEntryKind::File
                } else {
                    FileSystemEntryKind::Other
                }
            })
            .map_err(|error| map_error(path, &error))
    }
}

fn map_error(path: &Path, error: &std::io::Error) -> FileSystemClientError {
    let kind = match error.kind() {
        std::io::ErrorKind::NotFound => FileSystemClientErrorKind::NotFound,
        _ => FileSystemClientErrorKind::Other,
    };
    FileSystemClientError::new(
        kind,
        format!("access filesystem path {}: {error}", path.display()),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gtl_application::ports::{
        FileSystemClient, FileSystemClientErrorKind, FileSystemEntryKind,
    };

    use super::LocalFileSystemClient;

    #[test]
    fn canonicalize_returns_the_real_path() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let file = temporary.path().join("nested.txt");
        std::fs::write(&file, "content").expect("fixture file");

        assert_eq!(
            LocalFileSystemClient
                .canonicalize(&file)
                .expect("path canonicalizes"),
            std::fs::canonicalize(file).expect("expected canonical path"),
        );
    }

    #[test]
    fn entry_kind_distinguishes_files_from_other_entries() {
        let temporary = tempfile::tempdir().expect("temporary directory");
        let file = temporary.path().join("file.txt");
        std::fs::write(&file, "content").expect("fixture file");

        assert_eq!(
            LocalFileSystemClient
                .entry_kind(&file)
                .expect("file metadata"),
            FileSystemEntryKind::File,
        );
        assert_eq!(
            LocalFileSystemClient
                .entry_kind(temporary.path())
                .expect("directory metadata"),
            FileSystemEntryKind::Other,
        );
    }

    #[test]
    fn missing_paths_preserve_the_not_found_kind() {
        let error = LocalFileSystemClient
            .canonicalize(Path::new("/definitely/not/here"))
            .expect_err("missing path fails");

        assert_eq!(error.kind(), FileSystemClientErrorKind::NotFound);
    }
}
