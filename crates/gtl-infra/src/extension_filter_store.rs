use gtl_application::{
    diffs::{
        get_repository_extension_filter,
        save_repository_extension_filter::{self, SaveRepositoryExtensionFilter},
    },
    ports::{ExtensionFilterReader, ExtensionFilterWriter},
};
use gtl_models::{diffs::ExtensionFilter, paths::RepositoryRoot};

use crate::app_state::SqliteAppState;

impl ExtensionFilterReader for SqliteAppState {
    fn extension_filter(&self, repository: &RepositoryRoot) -> anyhow::Result<ExtensionFilter> {
        get_repository_extension_filter::execute(repository, &*self.connection_lock()?)
    }
}

impl ExtensionFilterWriter for SqliteAppState {
    fn save_extension_filter(
        &self,
        repository: &RepositoryRoot,
        filter: &ExtensionFilter,
    ) -> anyhow::Result<()> {
        save_repository_extension_filter::execute(
            &SaveRepositoryExtensionFilter { repository, filter },
            &*self.connection_lock()?,
        )
    }
}

#[cfg(test)]
mod tests {
    use gtl_models::diffs::{ExtensionFilterMode, FileExtensions};

    use super::*;

    fn repository(directory: &tempfile::TempDir) -> RepositoryRoot {
        RepositoryRoot::try_new(directory.path().join("repo")).unwrap()
    }

    #[test]
    fn saved_filters_round_trip_per_repository_and_inactive_filters_are_forgotten() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteAppState::open(directory.path()).unwrap();
        let repository = repository(&directory);
        let other = RepositoryRoot::try_new(directory.path().join("other")).unwrap();
        let only_rust = ExtensionFilter::new(
            ExtensionFilterMode::Only,
            FileExtensions::new(["rs", "toml"]),
        );

        assert_eq!(
            database.extension_filter(&repository).unwrap(),
            ExtensionFilter::default()
        );
        database
            .save_extension_filter(&repository, &only_rust)
            .unwrap();
        assert_eq!(database.extension_filter(&repository).unwrap(), only_rust);
        assert_eq!(
            database.extension_filter(&other).unwrap(),
            ExtensionFilter::default()
        );

        let hide_locks =
            ExtensionFilter::new(ExtensionFilterMode::Hide, FileExtensions::new(["lock"]));
        database
            .save_extension_filter(&repository, &hide_locks)
            .unwrap();
        assert_eq!(database.extension_filter(&repository).unwrap(), hide_locks);

        database
            .save_extension_filter(
                &repository,
                &ExtensionFilter::new(ExtensionFilterMode::Only, FileExtensions::default()),
            )
            .unwrap();
        let rows: i64 = database
            .connection_lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM repository_extension_filters",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(rows, 0);
    }

    #[test]
    fn corrupt_saved_filters_are_errors() {
        let directory = tempfile::tempdir().unwrap();
        let database = SqliteAppState::open(directory.path()).unwrap();
        let repository = repository(&directory);
        database
            .connection_lock()
            .unwrap()
            .execute(
                "INSERT INTO repository_extension_filters (repository_root, mode, extensions_json) VALUES (?1, 'hide', '[1]')",
                [repository.to_str().unwrap()],
            )
            .unwrap();

        assert!(database.extension_filter(&repository).is_err());
    }
}
