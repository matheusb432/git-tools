use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use gtl_application::{
    ports::{UserSettingsEditConflict, UserSettingsEditError, UserSettingsEditOutcome},
    settings::UserSettingsPatch,
};
use gtl_models::settings::UserSettingsRevision;

use super::{document::UserSettingsDocumentEdit, settings_document};

const USER_SETTINGS_LOCK_WAIT_MAX: Duration = Duration::from_secs(5);
const USER_SETTINGS_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(10);
const USER_SETTINGS_SYMBOLIC_LINK_DEPTH_MAX: usize = 40;

fn lock_path(settings_path: &Path) -> PathBuf {
    let mut value = OsString::from(settings_path.as_os_str());
    value.push(".lock");
    PathBuf::from(value)
}

#[derive(Debug)]
#[must_use = "dropping the lease releases exclusive access to the user-settings file"]
struct SettingsEditLease {
    _lock_file: File,
}

impl SettingsEditLease {
    fn acquire(settings_path: &Path) -> Result<Self, UserSettingsEditError> {
        let path = lock_path(settings_path);
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("open user-settings lock {}", path.display()))?;
        let deadline = Instant::now() + USER_SETTINGS_LOCK_WAIT_MAX;

        let mut remaining = lock_wait_remaining(deadline, &path)?;
        while !try_lock(&lock, &path)? {
            std::thread::sleep(remaining.min(USER_SETTINGS_LOCK_POLL_INTERVAL));
            remaining = lock_wait_remaining(deadline, &path)?;
        }
        Ok(Self { _lock_file: lock })
    }
}

fn lock_wait_remaining(deadline: Instant, path: &Path) -> Result<Duration, UserSettingsEditError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(UserSettingsEditConflict::LockTimeout {
            path: path.to_path_buf(),
            wait_seconds: USER_SETTINGS_LOCK_WAIT_MAX.as_secs(),
        }
        .into());
    }
    Ok(remaining)
}

fn try_lock(lock: &File, path: &Path) -> anyhow::Result<bool> {
    match lock.try_lock() {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => {
            Err(error).with_context(|| format!("lock user settings {}", path.display()))
        }
    }
}

fn lock_identity_path(settings_path: &Path) -> anyhow::Result<PathBuf> {
    match std::fs::canonicalize(settings_path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = settings_path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let file_name = settings_path
                .file_name()
                .context("user-settings path has no file name")?;
            let parent = std::fs::canonicalize(parent).with_context(|| {
                format!("resolve user-settings lock parent {}", parent.display())
            })?;
            Ok(parent.join(file_name))
        }
        Err(error) => Err(error).with_context(|| {
            format!(
                "resolve user-settings lock identity {}",
                settings_path.display()
            )
        }),
    }
}

fn acquire_lock(settings_path: &Path) -> Result<SettingsEditLease, UserSettingsEditError> {
    SettingsEditLease::acquire(&lock_identity_path(settings_path)?)
}

fn replacement_path(path: &Path) -> anyhow::Result<PathBuf> {
    let mut replacement = path.to_path_buf();
    for _ in 0..USER_SETTINGS_SYMBOLIC_LINK_DEPTH_MAX {
        let metadata = match std::fs::symlink_metadata(&replacement) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(replacement),
            Err(error) => return Err(replacement_metadata_error(&replacement, error)),
        };
        if !metadata.file_type().is_symlink() {
            return Ok(replacement);
        }

        let target = std::fs::read_link(&replacement).with_context(|| {
            format!("read user-settings symbolic link {}", replacement.display())
        })?;
        replacement = if target.is_absolute() {
            target
        } else {
            replacement
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or(Path::new("."))
                .join(target)
        };
    }

    anyhow::bail!(
        "user-settings path {} exceeds {USER_SETTINGS_SYMBOLIC_LINK_DEPTH_MAX} symbolic links",
        path.display()
    )
}

fn replacement_metadata_error(path: &Path, error: std::io::Error) -> anyhow::Error {
    anyhow::Error::new(error).context(format!(
        "inspect user-settings replacement target {}",
        path.display()
    ))
}

pub(super) fn edit(
    settings_path: &Path,
    settings_patch: UserSettingsPatch,
) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
    let lock_parent = settings_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(lock_parent)
        .with_context(|| format!("create user-settings directory {}", lock_parent.display()))?;

    let replacement_path = replacement_path(settings_path)?;
    let replacement_parent = replacement_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(replacement_parent).with_context(|| {
        format!(
            "create user-settings replacement directory {}",
            replacement_parent.display()
        )
    })?;
    let _lease = acquire_lock(&replacement_path)?;
    let baseline_bytes = read_document_bytes(&replacement_path)?;
    if settings_patch
        .expected_revision
        .is_some_and(|expected| expected != revision(&baseline_bytes))
    {
        return Err(UserSettingsEditConflict::StaleRevision {
            path: replacement_path,
        }
        .into());
    }
    let document = settings_document(&replacement_path, baseline_bytes.clone())?;
    let UserSettingsDocumentEdit::Changed(raw_new) = document.apply(settings_patch) else {
        return Ok(UserSettingsEditOutcome::Unchanged);
    };

    #[cfg(test)]
    wait_before_persist_for_test(&replacement_path);

    let current_bytes = read_document_bytes(&replacement_path)?;
    if current_bytes != baseline_bytes {
        return Err(UserSettingsEditConflict::ConcurrentModification {
            path: replacement_path,
        }
        .into());
    }

    let mut temporary = tempfile::NamedTempFile::new_in(replacement_parent).with_context(|| {
        format!(
            "create temporary settings file in {}",
            replacement_parent.display()
        )
    })?;
    temporary.write_all(raw_new.as_bytes()).with_context(|| {
        format!(
            "write temporary user settings {}",
            temporary.path().display()
        )
    })?;
    temporary.flush().with_context(|| {
        format!(
            "flush temporary user settings {}",
            temporary.path().display()
        )
    })?;
    temporary.as_file().sync_all().with_context(|| {
        format!(
            "synchronize temporary user settings {}",
            temporary.path().display()
        )
    })?;
    temporary
        .persist(&replacement_path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace user settings {}", replacement_path.display()))?;

    Ok(UserSettingsEditOutcome::Changed)
}

pub(super) fn read_document_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => {
            Err(error).with_context(|| format!("reread user settings {}", path.display()))
        }
    }
}

#[cfg(test)]
fn wait_before_persist_for_test(path: &Path) {
    let hook = {
        let mut slot = BEFORE_PERSIST_HOOK
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap();
        if slot.as_ref().is_some_and(|hook| hook.path == path) {
            slot.take()
        } else {
            None
        }
    };
    if let Some(hook) = hook {
        hook.ready_sender.send(()).unwrap();
        hook.continue_receiver.recv().unwrap();
    }
}

#[cfg(test)]
struct BeforePersistHook {
    path: PathBuf,
    ready_sender: std::sync::mpsc::SyncSender<()>,
    continue_receiver: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
static BEFORE_PERSIST_HOOK: std::sync::OnceLock<std::sync::Mutex<Option<BeforePersistHook>>> =
    std::sync::OnceLock::new();

pub(super) fn revision(bytes: &[u8]) -> UserSettingsRevision {
    use sha2::{Digest as _, Sha256};

    UserSettingsRevision::from_digest(Sha256::digest(bytes).into())
}

pub(super) fn reset_invalid(
    settings_path: &Path,
    expected_revision: &str,
    timestamp: &gtl_models::timestamps::MachineTimestamp,
) -> Result<PathBuf, UserSettingsEditError> {
    let target = replacement_path(settings_path)?;
    let _lease = acquire_lock(&target)?;
    let bytes = read_document_bytes(&target)?;
    if revision(&bytes).to_string() != expected_revision
        || settings_document(settings_path, bytes.clone()).is_ok()
    {
        return Err(UserSettingsEditConflict::ConcurrentModification {
            path: settings_path.to_path_buf(),
        }
        .into());
    }
    let timestamp = timestamp
        .as_ref()
        .chars()
        .take(19)
        .filter(|c| *c != '-' && *c != ':')
        .collect::<String>()
        .replace('T', "-");
    let stem = settings_path
        .file_stem()
        .context("user-settings path has no file stem")?
        .to_string_lossy();
    let backup_path = settings_path.with_file_name(format!("{stem}.{timestamp}-backup.toml"));
    let backup_parent = backup_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut backup =
        tempfile::NamedTempFile::new_in(backup_parent).context("create settings backup")?;
    backup.write_all(&bytes).context("write settings backup")?;
    backup
        .as_file()
        .sync_all()
        .context("synchronize settings backup")?;
    backup
        .persist_noclobber(&backup_path)
        .map_err(|error| error.error)
        .with_context(|| format!("save settings backup {}", backup_path.display()))?;
    let parent = target
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let defaults = tempfile::NamedTempFile::new_in(parent).context("create default settings")?;
    defaults
        .as_file()
        .sync_all()
        .context("synchronize default settings")?;
    #[cfg(test)]
    wait_before_persist_for_test(&target);
    if read_document_bytes(&target)? != bytes {
        return Err(UserSettingsEditConflict::ConcurrentModification {
            path: settings_path.to_path_buf(),
        }
        .into());
    }
    defaults
        .persist(&target)
        .map_err(|error| error.error)
        .context("restore default settings")?;
    Ok(backup_path)
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{File, OpenOptions},
        path::Path,
        sync::mpsc::{self, RecvTimeoutError},
        time::{Duration, Instant},
    };

    use gtl_application::{
        ports::{
            UserSettingsEditConflict, UserSettingsEditError, UserSettingsEditOutcome,
            UserSettingsEditor,
        },
        settings::{UserSettingsFieldUpdate, UserSettingsPatch},
    };
    use gtl_models::{
        settings::{SettingKey, SettingKeyValue},
        viewer::{DiffLayout, Theme},
    };

    use super::{
        BEFORE_PERSIST_HOOK, BeforePersistHook, USER_SETTINGS_LOCK_WAIT_MAX, edit,
        lock_identity_path, lock_path, replacement_path,
    };

    const LOCK_ATTEMPT_WAIT_TEST_MAX: Duration = Duration::from_millis(100);
    const LOCK_HELD_OBSERVATION_WAIT: Duration = Duration::from_millis(50);
    const RESULT_WAIT_TEST_MAX: Duration = Duration::from_secs(8);

    #[test]
    fn reset_preserves_invalid_bytes_and_restores_defaults() {
        for raw in [
            b"theme = {{{".as_slice(),
            b"theme = 7",
            b"\xff\xfe",
            b"[[projects]]\nname = \"rust-snake\"\nexclude_from_push_all = true",
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("config.toml");
            std::fs::write(&path, raw).unwrap();
            let timestamp =
                gtl_models::timestamps::MachineTimestamp::try_from("2026-09-08T03:10:09Z").unwrap();
            let backup =
                super::reset_invalid(&path, &super::revision(raw).to_string(), &timestamp).unwrap();
            assert_eq!(
                backup.file_name().unwrap(),
                "config.20260908-031009-backup.toml"
            );
            assert_eq!(std::fs::read(&backup).unwrap(), raw);
            assert_eq!(std::fs::read(&path).unwrap(), b"");
            let store = crate::user_config::TomlSettingsStore::new(Some(path));
            assert!(gtl_application::ports::UserSettingsReader::load(&store).is_ok());
        }
    }

    #[test]
    fn reset_rejects_changed_or_repaired_settings_and_backup_collisions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let timestamp =
            gtl_models::timestamps::MachineTimestamp::try_from("2026-09-08T03:10:09Z").unwrap();
        let raw = b"theme = 7";
        std::fs::write(&path, raw).unwrap();
        assert!(matches!(
            super::reset_invalid(
                &path,
                &super::revision(b"theme = 8").to_string(),
                &timestamp,
            ),
            Err(UserSettingsEditError::Conflict(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        let backup = path.with_file_name("config.20260908-031009-backup.toml");
        std::fs::write(&backup, "keep this backup").unwrap();
        assert!(
            super::reset_invalid(&path, &super::revision(raw).to_string(), &timestamp).is_err()
        );
        assert_eq!(
            std::fs::read_to_string(&backup).unwrap(),
            "keep this backup"
        );
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        std::fs::write(&path, b"").unwrap();
        assert!(matches!(
            super::reset_invalid(&path, &super::revision(b"").to_string(), &timestamp),
            Err(UserSettingsEditError::Conflict(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn reset_preserves_symbolic_links_and_private_backup_permissions() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let target = directory.path().join("managed.toml");
        let raw = b"theme = 7";
        std::fs::write(&target, raw).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let timestamp =
            gtl_models::timestamps::MachineTimestamp::try_from("2026-09-08T03:10:09Z").unwrap();
        let backup =
            super::reset_invalid(&path, &super::revision(raw).to_string(), &timestamp).unwrap();
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(std::fs::read(&target).unwrap().is_empty());
        assert_eq!(std::fs::read(&backup).unwrap(), raw);
        assert_eq!(
            std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    fn set(mutation: SettingKeyValue) -> UserSettingsPatch {
        mutation.into()
    }

    fn clear(key: SettingKey) -> UserSettingsPatch {
        UserSettingsPatch::clear(key)
    }

    #[cfg(unix)]
    fn create_file_symbolic_link(original: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(original, link)
    }

    #[cfg(windows)]
    fn create_file_symbolic_link(original: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(original, link)
    }

    fn hold_settings_lock(path: &Path) -> File {
        let replacement_path = replacement_path(path).unwrap();
        let identity_path = lock_identity_path(&replacement_path).unwrap();
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .truncate(false)
            .write(true)
            .open(lock_path(&identity_path))
            .unwrap();
        lock.lock().unwrap();
        lock
    }

    #[test]
    fn set_preserves_unrelated_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "# viewer\ntheme = \"dark\"\n[push]\nconfirm = false\n",
        )
        .unwrap();

        let outcome = edit(&path, set(SettingKeyValue::Theme(Theme::Glacier))).unwrap();
        let raw = std::fs::read_to_string(path).unwrap();

        assert_eq!(outcome, UserSettingsEditOutcome::Changed);
        assert!(raw.contains("# viewer"));
        assert!(raw.contains("theme = \"glacier\""));
        assert!(raw.contains("[push]\nconfirm = false"));
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "requires Windows Developer Mode or the symbolic-link privilege"
    )]
    fn set_through_relative_symlink_preserves_link_and_updates_target() {
        let directory = tempfile::tempdir().unwrap();
        let config_directory = directory.path().join("config");
        let managed_directory = directory.path().join("managed");
        std::fs::create_dir_all(&config_directory).unwrap();
        std::fs::create_dir_all(&managed_directory).unwrap();
        let target = managed_directory.join("settings.toml");
        let path = config_directory.join("config.toml");
        std::fs::write(&target, "theme = \"dark\"\n").unwrap();
        create_file_symbolic_link(Path::new("../managed/settings.toml"), &path).unwrap();

        let outcome = edit(&path, set(SettingKeyValue::Theme(Theme::Glacier))).unwrap();

        assert_eq!(outcome, UserSettingsEditOutcome::Changed);
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "theme = \"glacier\"\n"
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "requires Windows Developer Mode or the symbolic-link privilege"
    )]
    fn set_through_dangling_relative_symlink_creates_target() {
        let directory = tempfile::tempdir().unwrap();
        let config_directory = directory.path().join("config");
        let managed_directory = directory.path().join("managed");
        std::fs::create_dir_all(&config_directory).unwrap();
        std::fs::create_dir_all(&managed_directory).unwrap();
        let target = managed_directory.join("settings.toml");
        let path = config_directory.join("config.toml");
        create_file_symbolic_link(Path::new("../managed/settings.toml"), &path).unwrap();

        let outcome = edit(&path, set(SettingKeyValue::Theme(Theme::Glacier))).unwrap();

        assert_eq!(outcome, UserSettingsEditOutcome::Changed);
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "theme = \"glacier\"\n"
        );
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "requires Windows Developer Mode or the symbolic-link privilege"
    )]
    fn symbolic_link_cycle_is_bounded_and_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let path_other = directory.path().join("config-other.toml");
        create_file_symbolic_link(Path::new("config-other.toml"), &path).unwrap();
        create_file_symbolic_link(Path::new("config.toml"), &path_other).unwrap();

        let error = edit(&path, set(SettingKeyValue::Theme(Theme::Glacier))).unwrap_err();

        assert!(format!("{error:#}").contains("exceeds 40 symbolic links"));
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            std::fs::symlink_metadata(&path_other)
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn equal_set_and_absent_remove_do_not_rewrite_config() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = "theme = \"dark\"\n";
        std::fs::write(&path, raw).unwrap();

        let equal = edit(&path, set(SettingKeyValue::Theme(Theme::Dark))).unwrap();
        let absent = edit(&path, clear(SettingKey::Layout)).unwrap();

        assert_eq!(equal, UserSettingsEditOutcome::Unchanged);
        assert_eq!(absent, UserSettingsEditOutcome::Unchanged);
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn non_string_document_returns_a_configuration_error_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = "theme = 7\n";
        std::fs::write(&path, raw).unwrap();

        let error = edit(&path, set(SettingKeyValue::Theme(Theme::Glacier))).unwrap_err();

        assert!(matches!(
            error,
            UserSettingsEditError::InvalidConfiguration(configuration)
                if configuration.path() == path
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn malformed_document_returns_a_typed_configuration_error_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = "theme = {{{\n";
        std::fs::write(&path, raw).unwrap();

        let error = edit(&path, set(SettingKeyValue::Theme(Theme::Glacier))).unwrap_err();

        assert!(matches!(
            error,
            UserSettingsEditError::InvalidConfiguration(configuration)
                if configuration.path() == path
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    #[cfg_attr(
        windows,
        ignore = "requires Windows Developer Mode or the symbolic-link privilege"
    )]
    fn symlink_aliases_share_the_lock_and_timeout_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("managed").join("config.toml");
        let path = directory.path().join("config.toml");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let raw = "theme = \"dark\"\n";
        std::fs::write(&target, raw).unwrap();
        create_file_symbolic_link(Path::new("managed/config.toml"), &path).unwrap();
        let _lock = hold_settings_lock(&target);
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let path_worker = path.clone();

        let started_at = Instant::now();
        let worker = std::thread::spawn(move || {
            let result = edit(&path_worker, set(SettingKeyValue::Theme(Theme::Glacier)));
            result_sender.send(result).unwrap();
        });
        let result = result_receiver.recv_timeout(RESULT_WAIT_TEST_MAX).unwrap();
        let elapsed = started_at.elapsed();
        drop(worker);
        let error = result.unwrap_err();

        assert!(matches!(
            error,
            UserSettingsEditError::Conflict(UserSettingsEditConflict::LockTimeout {
                wait_seconds: 5,
                ..
            })
        ));
        assert!(
            elapsed >= USER_SETTINGS_LOCK_WAIT_MAX,
            "elapsed: {elapsed:?}"
        );
        assert!(elapsed < RESULT_WAIT_TEST_MAX, "elapsed: {elapsed:?}");
        assert_eq!(std::fs::read_to_string(target).unwrap(), raw);
    }

    #[test]
    fn concurrent_external_change_aborts_and_preserves_the_change() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = "theme = \"dark\"\nlayout = \"split\"\n";
        let changed_raw = "theme = \"mirage\"\nlayout = \"split\"\n";
        std::fs::write(&path, raw).unwrap();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let (continue_sender, continue_receiver) = mpsc::sync_channel(1);
        BEFORE_PERSIST_HOOK
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .unwrap()
            .replace(BeforePersistHook {
                path: path.clone(),
                ready_sender,
                continue_receiver,
            });
        let path_worker = path.clone();
        let worker = std::thread::spawn(move || {
            edit(&path_worker, set(SettingKeyValue::Theme(Theme::Glacier)))
        });

        ready_receiver.recv_timeout(RESULT_WAIT_TEST_MAX).unwrap();
        std::fs::write(&path, changed_raw).unwrap();
        continue_sender.send(()).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert!(matches!(
            error,
            UserSettingsEditError::Conflict(
                UserSettingsEditConflict::ConcurrentModification { path: error_path }
            ) if error_path == path
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), changed_raw);
    }

    #[test]
    fn stale_revision_rejects_the_edit_before_parsing_or_replacing_the_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let loaded_raw = b"theme = \"dark\"\n";
        let current_raw = "theme = \"mirage\"\n";
        let expected_revision = super::revision(loaded_raw);
        std::fs::write(&path, current_raw).unwrap();

        let error = edit(
            &path,
            UserSettingsPatch {
                expected_revision: Some(expected_revision),
                theme: UserSettingsFieldUpdate::Update(Theme::Glacier),
                ..UserSettingsPatch::default()
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            UserSettingsEditError::Conflict(UserSettingsEditConflict::StaleRevision {
                path: error_path
            }) if error_path == path
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), current_raw);
    }

    #[test]
    fn concurrent_changes_to_different_keys_both_survive() {
        use crate::user_config::TomlSettingsStore;

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let store = TomlSettingsStore::new(Some(path.clone()));
        let lock = hold_settings_lock(&path);
        let (attempt_sender, attempt_receiver) = mpsc::sync_channel(2);
        let (result_sender, result_receiver) = mpsc::sync_channel(2);

        let mut store_theme = store.clone();
        let attempt_sender_theme = attempt_sender.clone();
        let result_sender_theme = result_sender.clone();
        let theme_worker = std::thread::spawn(move || {
            attempt_sender_theme.send("theme").unwrap();
            let result = store_theme.edit(SettingKeyValue::Theme(Theme::Glacier).into());
            result_sender_theme.send(("theme", result)).unwrap();
        });

        let mut store_layout = store.clone();
        let attempt_sender_layout = attempt_sender.clone();
        let result_sender_layout = result_sender.clone();
        let layout_worker = std::thread::spawn(move || {
            attempt_sender_layout.send("layout").unwrap();
            let result = store_layout.edit(SettingKeyValue::Layout(DiffLayout::Split).into());
            result_sender_layout.send(("layout", result)).unwrap();
        });
        drop(attempt_sender);
        drop(result_sender);

        let attempt_first = attempt_receiver
            .recv_timeout(LOCK_ATTEMPT_WAIT_TEST_MAX)
            .unwrap();
        let attempt_second = attempt_receiver
            .recv_timeout(LOCK_ATTEMPT_WAIT_TEST_MAX)
            .unwrap();
        assert_ne!(attempt_first, attempt_second);

        let result_while_locked = result_receiver.recv_timeout(LOCK_HELD_OBSERVATION_WAIT);
        assert!(
            matches!(result_while_locked, Err(RecvTimeoutError::Timeout)),
            "an editor completed while the sibling lock was held: {result_while_locked:?}"
        );
        lock.unlock().unwrap();

        let result_first = result_receiver.recv_timeout(RESULT_WAIT_TEST_MAX).unwrap();
        let result_second = result_receiver.recv_timeout(RESULT_WAIT_TEST_MAX).unwrap();
        drop(theme_worker);
        drop(layout_worker);

        let mut tags = Vec::with_capacity(2);
        for (tag, result) in [result_first, result_second] {
            assert_eq!(
                result.unwrap(),
                gtl_application::ports::UserSettingsEditOutcome::Changed,
                "{tag} document change"
            );
            tags.push(tag);
        }
        tags.sort_unstable();
        assert_eq!(tags, ["layout", "theme"]);

        let raw = std::fs::read_to_string(path).unwrap();
        let document = toml::from_str::<toml::Value>(&raw).unwrap();
        assert_eq!(document["theme"].as_str(), Some("glacier"));
        assert_eq!(document["layout"].as_str(), Some("split"));
    }

    #[test]
    fn remove_deletes_the_selected_setting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "density = \"full\"\n").unwrap();

        let outcome = edit(&path, clear(SettingKey::Density)).unwrap();

        assert_eq!(outcome, UserSettingsEditOutcome::Changed);
        assert!(!std::fs::read_to_string(path).unwrap().contains("density"));
    }

    #[test]
    fn absent_remove_does_not_create_the_config_target() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("config.toml");

        let outcome = edit(&path, clear(SettingKey::Density)).unwrap();

        assert_eq!(outcome, UserSettingsEditOutcome::Unchanged);
        assert!(!path.exists());
    }
}
