use std::{fs, path::Path};

use gtl_application::{
    ports::{
        UserSettingsEditor as _, UserSettingsLoadError, UserSettingsReader as _,
        UserSettingsRecovery as _,
    },
    settings::{UserSettingsFieldUpdate, UserSettingsPatch},
};
use gtl_infra::user_config::TomlSettingsStore;
use gtl_models::{timestamps::MachineTimestamp, viewer::Theme};

#[test]
fn viewer_push_confirmation_persists_independently_of_cli_and_clear_restores_default()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    for cli_confirmation in [true, false] {
        fs::write(
            &path,
            format!(
                "# keep this\nviewer_push_no_confirmation_projects = ['review-project']\n[push]\nconfirm = {cli_confirmation}\n"
            ),
        )?;
        let mut store = TomlSettingsStore::new(Some(path.clone()));
        let cached = store.clone();
        assert!(cached.load()?.viewer_push_confirmation_required());
        for viewer_confirmation in [true, false] {
            store.edit(UserSettingsPatch {
                viewer_push_confirmation_required: UserSettingsFieldUpdate::Update(
                    viewer_confirmation,
                ),
                ..Default::default()
            })?;
            for loaded in [
                cached.load()?,
                TomlSettingsStore::new(Some(path.clone())).load()?,
            ] {
                assert_eq!(
                    loaded.viewer_push_confirmation_required(),
                    viewer_confirmation
                );
                assert_eq!(loaded.push_confirmation_required(), cli_confirmation);
                assert_eq!(loaded.viewer_push_no_confirmation_projects().len(), 1);
            }
            assert!(fs::read_to_string(&path)?.contains("# keep this"));
        }
        store.edit(UserSettingsPatch {
            viewer_push_confirmation_required: UserSettingsFieldUpdate::Clear,
            ..Default::default()
        })?;
        let loaded = cached.load()?;
        assert!(loaded.viewer_push_confirmation_required());
        assert_eq!(loaded.push_confirmation_required(), cli_confirmation);
    }
    fs::write(&path, "[viewer_push]\nconfirm = 'false'\n")?;
    assert!(matches!(
        TomlSettingsStore::new(Some(path)).load(),
        Err(UserSettingsLoadError::InvalidConfiguration(_))
    ));
    Ok(())
}

#[test]
fn viewer_push_confirmation_is_opted_out_per_project_and_preserves_cli_preferences()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(&path, "# keep this\n[push]\nconfirm = false\n")?;
    let mut store = TomlSettingsStore::new(Some(path.clone()));
    assert!(
        store
            .load()?
            .viewer_push_no_confirmation_projects()
            .is_empty()
    );
    let project = gtl_models::paths::ProjectName::try_new("review-project")?;
    store.edit(UserSettingsPatch {
        viewer_push_no_confirmation_projects: UserSettingsFieldUpdate::Update(
            [project.clone()].into(),
        ),
        ..Default::default()
    })?;
    let loaded = TomlSettingsStore::new(Some(path.clone())).load()?;
    assert!(
        loaded
            .viewer_push_no_confirmation_projects()
            .contains(&project)
    );
    assert!(
        !loaded
            .viewer_push_no_confirmation_projects()
            .contains(&gtl_models::paths::ProjectName::try_new("another-project")?)
    );
    assert!(!loaded.push_confirmation_required());
    assert!(fs::read_to_string(&path)?.contains("# keep this"));
    store.edit(UserSettingsPatch {
        viewer_push_no_confirmation_projects: UserSettingsFieldUpdate::Clear,
        ..Default::default()
    })?;
    assert!(
        store
            .load()?
            .viewer_push_no_confirmation_projects()
            .is_empty()
    );
    assert!(!store.load()?.push_confirmation_required());
    fs::write(path, "viewer_push_no_confirmation_projects = [42]\n")?;
    assert!(matches!(
        store.load(),
        Err(UserSettingsLoadError::InvalidConfiguration(_))
    ));
    Ok(())
}

#[test]
fn project_sort_persists_through_cached_reads_edits_and_clear() -> anyhow::Result<()> {
    use gtl_models::settings::ProjectsSort;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("config.toml");
    fs::write(&path, "# keep this\ntheme = \"dark\"\n")?;
    let mut store = TomlSettingsStore::new(Some(path.clone()));
    let cached = store.clone();
    assert_eq!(cached.load_viewer_settings()?.1.sort, ProjectsSort::Changes);
    for sort in [
        ProjectsSort::Name,
        ProjectsSort::Branch,
        ProjectsSort::Changes,
        ProjectsSort::ChangesAscending,
        ProjectsSort::NameDescending,
        ProjectsSort::BranchDescending,
    ] {
        store.edit(UserSettingsPatch {
            projects_sort: UserSettingsFieldUpdate::Update(sort),
            ..Default::default()
        })?;
        assert_eq!(cached.load_viewer_settings()?.1.sort, sort);
        assert_eq!(
            TomlSettingsStore::new(Some(path.clone()))
                .load_viewer_settings()?
                .1
                .sort,
            sort
        );
        let document = fs::read_to_string(&path)?;
        assert!(document.contains(&format!("projects_sort = \"{sort}\"")));
        assert!(document.contains("# keep this"));
        assert_eq!(cached.load()?.theme(), Some(Theme::Dark));
    }
    store.edit(UserSettingsPatch {
        projects_sort: UserSettingsFieldUpdate::Clear,
        ..Default::default()
    })?;
    assert_eq!(cached.load_viewer_settings()?.1.sort, ProjectsSort::Changes);
    assert!(!fs::read_to_string(&path)?.contains("projects_sort"));
    fs::write(&path, "projects_sort = \"size\"\n")?;
    assert!(matches!(
        cached.load(),
        Err(UserSettingsLoadError::InvalidConfiguration(_))
    ));
    Ok(())
}

fn write_settings(path: &Path, theme: &str, page_size: u32) -> std::io::Result<()> {
    // Preserve byte length when the cache test changes a theme without changing mtime.
    let theme = format!("\"{theme}\"");
    fs::write(
        path,
        format!("theme = {theme:10}\nprojects_page_size = {page_size}\n"),
    )
}

fn assert_settings(
    store: &TomlSettingsStore,
    theme: Theme,
    page_size: u32,
) -> Result<(), UserSettingsLoadError> {
    let settings = store.load()?;
    let (viewer_settings, projects) = store.clone().load_viewer_settings()?;
    assert_eq!(settings.theme(), Some(theme));
    assert_eq!(viewer_settings, settings);
    assert_eq!(projects.page_size.into_inner(), page_size);
    Ok(())
}

#[test]
fn clones_reload_equal_length_edits_with_unchanged_modification_time() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_settings(&path, "dark", 30).unwrap();
    let store = TomlSettingsStore::new(Some(path.clone()));
    let reader = store.clone();
    assert_settings(&store, Theme::Dark, 30).unwrap();
    let metadata = fs::metadata(&path).unwrap();

    write_settings(&path, "graphite", 10).unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
        .unwrap();
    let changed_metadata = fs::metadata(&path).unwrap();
    assert_eq!(changed_metadata.len(), metadata.len());
    assert_eq!(
        changed_metadata.modified().unwrap(),
        metadata.modified().unwrap()
    );
    assert_settings(&reader, Theme::Graphite, 10).unwrap();
    assert_settings(&store, Theme::Graphite, 10).unwrap();
}

#[test]
fn reads_follow_file_creation_deletion_and_recreation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let store = TomlSettingsStore::new(Some(path.clone()));
    let reader = store.clone();
    let defaults = TomlSettingsStore::new(None).load_viewer_settings().unwrap();
    assert_eq!(store.load_viewer_settings().unwrap(), defaults);

    write_settings(&path, "glacier", 30).unwrap();
    assert_settings(&reader, Theme::Glacier, 30).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(reader.load().unwrap(), defaults.0);
    assert_eq!(store.load_viewer_settings().unwrap(), defaults);
    write_settings(&path, "graphite", 10).unwrap();
    assert_settings(&store, Theme::Graphite, 10).unwrap();
}

#[test]
fn cached_settings_do_not_hide_current_errors_and_reads_recover_after_repair() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_settings(&path, "dark", 30).unwrap();
    let store = TomlSettingsStore::new(Some(path.clone()));
    let reader = store.clone();
    assert_settings(&store, Theme::Dark, 30).unwrap();

    for invalid in [b"theme = {{{".as_slice(), b"theme = 7", b"\xff\xfe"] {
        fs::write(&path, invalid).unwrap();
        assert!(matches!(
            reader.load(),
            Err(UserSettingsLoadError::InvalidConfiguration(error)) if error.path() == path
        ));
        assert!(matches!(
            store.load_viewer_settings(),
            Err(UserSettingsLoadError::InvalidConfiguration(error)) if error.path() == path
        ));
        write_settings(&path, "dark", 30).unwrap();
        assert_settings(&store, Theme::Dark, 30).unwrap();
    }

    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(matches!(
        reader.load(),
        Err(UserSettingsLoadError::Adapter(_))
    ));
    assert!(matches!(
        store.load_viewer_settings(),
        Err(UserSettingsLoadError::Adapter(_))
    ));
    fs::remove_dir(&path).unwrap();
    write_settings(&path, "glacier", 10).unwrap();
    assert_settings(&reader, Theme::Glacier, 10).unwrap();
}

#[test]
fn editing_a_clone_preserves_external_changes_and_refreshes_both_read_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_settings(&path, "dark", 30).unwrap();
    let store = TomlSettingsStore::new(Some(path.clone()));
    let mut editor = store.clone();
    assert_settings(&store, Theme::Dark, 30).unwrap();

    write_settings(&path, "glacier", 10).unwrap();
    editor
        .edit(UserSettingsPatch {
            theme: UserSettingsFieldUpdate::Update(Theme::Graphite),
            ..Default::default()
        })
        .unwrap();
    assert_settings(&store, Theme::Graphite, 10).unwrap();
    assert_settings(&editor, Theme::Graphite, 10).unwrap();
}

#[test]
fn recovery_inspects_current_bytes_and_reset_refreshes_cached_settings() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    write_settings(&path, "dark", 30).unwrap();
    let store = TomlSettingsStore::new(Some(path.clone()));
    let mut recovery = store.clone();
    assert_settings(&store, Theme::Dark, 30).unwrap();

    let invalid = b"theme = 7";
    fs::write(&path, invalid).unwrap();
    let inspection = recovery.inspect().unwrap();
    assert!(inspection.diagnostic.is_some());
    let timestamp = MachineTimestamp::try_from("2026-09-13T12:00:00Z").unwrap();
    let backup = recovery
        .reset_invalid(&inspection.revision, &timestamp)
        .unwrap();
    assert_eq!(fs::read(backup).unwrap(), invalid);
    let defaults = TomlSettingsStore::new(None).load_viewer_settings().unwrap();
    assert_eq!(store.load().unwrap(), defaults.0);
    assert_eq!(recovery.load_viewer_settings().unwrap(), defaults);
}

#[cfg(unix)]
#[test]
fn reads_follow_atomic_replacement_and_symbolic_link_retargeting() {
    use std::{io::Write as _, os::unix::fs::symlink};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.toml");
    let target = directory.path().join("managed.toml");
    let target_other = directory.path().join("other.toml");
    write_settings(&target, "dark", 30).unwrap();
    write_settings(&target_other, "glacier", 10).unwrap();
    symlink(&target, &path).unwrap();
    let store = TomlSettingsStore::new(Some(path.clone()));
    assert_settings(&store, Theme::Dark, 30).unwrap();

    let mut replacement = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
    replacement
        .write_all(b"theme = \"graphite\"\nprojects_page_size = 15\n")
        .unwrap();
    replacement.persist(&target).unwrap();
    assert_settings(&store, Theme::Graphite, 15).unwrap();
    symlink(&target_other, directory.path().join("config-new.toml")).unwrap();
    fs::rename(directory.path().join("config-new.toml"), &path).unwrap();
    assert_settings(&store, Theme::Glacier, 10).unwrap();
    assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
}
