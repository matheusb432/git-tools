use std::path::{Path, PathBuf};

use gtl_application::ports::{
    FileSystemClient, FileSystemClientError, FileSystemClientErrorKind, FileSystemEntryKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct LocalFileSystemClient;

impl FileSystemClient for LocalFileSystemClient {
    fn canonical_working_directory(&self) -> Result<PathBuf, FileSystemClientError> {
        let working_directory = std::env::current_dir().map_err(|error| {
            FileSystemClientError::new(
                FileSystemClientErrorKind::Other,
                format!("resolve current working directory: {error}"),
            )
        })?;
        self.canonicalize(&working_directory)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FileSystemClientError> {
        std::fs::canonicalize(path).map_err(|error| map_error(path, &error))
    }

    fn entry_kind(&self, path: &Path) -> Result<FileSystemEntryKind, FileSystemClientError> {
        std::fs::metadata(path)
            .map(|metadata| entry_kind_from_metadata(&metadata))
            .map_err(|error| map_error(path, &error))
    }
}

fn entry_kind_from_metadata(metadata: &std::fs::Metadata) -> FileSystemEntryKind {
    if metadata.is_file() {
        FileSystemEntryKind::File
    } else {
        FileSystemEntryKind::Other
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
        let temporary = tempfile::tempdir().unwrap();
        let file = temporary.path().join("nested.txt");
        std::fs::write(&file, "content").unwrap();

        assert_eq!(
            LocalFileSystemClient.canonicalize(&file).unwrap(),
            std::fs::canonicalize(file).unwrap(),
        );
    }

    #[test]
    fn canonical_working_directory_returns_the_client_directory() {
        let expected = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();

        assert_eq!(
            LocalFileSystemClient.canonical_working_directory().unwrap(),
            expected,
        );
    }

    #[test]
    fn entry_kind_distinguishes_files_from_other_entries() {
        let temporary = tempfile::tempdir().unwrap();
        let file = temporary.path().join("file.txt");
        std::fs::write(&file, "content").unwrap();

        assert_eq!(
            LocalFileSystemClient.entry_kind(&file).unwrap(),
            FileSystemEntryKind::File,
        );
        assert_eq!(
            LocalFileSystemClient.entry_kind(temporary.path()).unwrap(),
            FileSystemEntryKind::Other,
        );
    }

    #[test]
    fn missing_paths_preserve_the_not_found_kind() {
        let error = LocalFileSystemClient
            .canonicalize(Path::new("/definitely/not/here"))
            .unwrap_err();

        assert_eq!(error.kind(), FileSystemClientErrorKind::NotFound);
    }
}
