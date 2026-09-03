use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use gtl_application::ports::{UserSettingsEditError, UserSettingsEditOutcome};
use toml_edit::{DocumentMut, Item, Value};

const USER_SETTINGS_LOCK_WAIT_MAX: Duration = Duration::from_secs(5);
const USER_SETTINGS_LOCK_POLL_INTERVAL: Duration = Duration::from_millis(10);
const USER_SETTINGS_SYMBOLIC_LINK_DEPTH_MAX: usize = 40;

#[derive(Debug, Clone, Copy)]
pub(super) enum StringEdit<'value> {
    Set(&'value str),
    Remove,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct StringEditOutcome {
    pub(super) value_old: Option<String>,
    pub(super) document_changed: bool,
}

impl From<StringEditOutcome> for UserSettingsEditOutcome {
    fn from(outcome: StringEditOutcome) -> Self {
        if outcome.document_changed {
            Self::Changed
        } else {
            Self::Unchanged
        }
    }
}

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
        return Err(UserSettingsEditError::LockTimeout {
            path: path.to_path_buf(),
            wait_seconds: USER_SETTINGS_LOCK_WAIT_MAX.as_secs(),
        });
    }
    Ok(remaining)
}

fn try_lock(lock: &File, path: &Path) -> Result<bool, UserSettingsEditError> {
    match lock.try_lock() {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => Err(UserSettingsEditError::Unexpected(
            anyhow::Error::new(error).context(format!("lock user settings {}", path.display())),
        )),
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

fn read_document(path: &Path) -> Result<(Vec<u8>, String, DocumentMut), UserSettingsEditError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(UserSettingsEditError::Unexpected(
                anyhow::Error::new(error).context(format!("read user settings {}", path.display())),
            ));
        }
    };
    let raw = String::from_utf8(bytes.clone()).map_err(|error| {
        UserSettingsEditError::InvalidConfiguration {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    })?;
    let document = raw.parse::<DocumentMut>().map_err(|error| {
        UserSettingsEditError::InvalidConfiguration {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    })?;
    Ok((bytes, raw, document))
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

fn value_old(document: &DocumentMut, key: &str) -> Result<Option<String>, UserSettingsEditError> {
    match document.get(key) {
        None | Some(Item::None) => Ok(None),
        Some(Item::Value(value)) => value
            .as_str()
            .map(str::to_owned)
            .map(Some)
            .ok_or(UserSettingsEditError::InvalidValueShape),
        Some(_) => Err(UserSettingsEditError::InvalidValueShape),
    }
}

fn set_string(document: &mut DocumentMut, key: &str, value_new: &str) {
    if let Some(Item::Value(value_old)) = document.get_mut(key) {
        let decor = value_old.decor().clone();
        let mut replacement = Value::from(value_new);
        *replacement.decor_mut() = decor;
        *value_old = replacement;
    } else {
        document[key] = toml_edit::value(value_new);
    }
}

pub(super) fn edit(
    path: &Path,
    key: &str,
    edit: StringEdit<'_>,
) -> Result<StringEditOutcome, UserSettingsEditError> {
    let lock_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(lock_parent)
        .with_context(|| format!("create user-settings directory {}", lock_parent.display()))?;

    let replacement_path = replacement_path(path)?;
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
    let (baseline_bytes, raw, mut document) = read_document(&replacement_path)?;
    let value_old = value_old(&document, key)?;
    super::validate_raw_for_edit(&replacement_path, &raw)?;

    let document_changed = match edit {
        StringEdit::Set(value_new) if value_old.as_deref() == Some(value_new) => false,
        StringEdit::Set(value_new) => {
            set_string(&mut document, key, value_new);
            true
        }
        StringEdit::Remove if value_old.is_none() => false,
        StringEdit::Remove => {
            document.remove(key);
            true
        }
    };

    if !document_changed {
        return Ok(StringEditOutcome {
            value_old,
            document_changed,
        });
    }

    #[cfg(test)]
    wait_before_persist_for_test(&replacement_path);

    let current_bytes = read_document_bytes(&replacement_path)?;
    if current_bytes != baseline_bytes {
        return Err(UserSettingsEditError::ConcurrentModification {
            path: replacement_path,
        });
    }

    let raw_new = document.to_string();
    super::validate_raw_for_edit(&replacement_path, &raw_new)?;
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

    Ok(StringEditOutcome {
        value_old,
        document_changed,
    })
}

pub(super) fn edit_document(
    path: &Path,
    apply: impl FnOnce(&mut DocumentMut) -> Result<(), UserSettingsEditError>,
) -> Result<UserSettingsEditOutcome, UserSettingsEditError> {
    let lock_parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(lock_parent)
        .with_context(|| format!("create user-settings directory {}", lock_parent.display()))?;
    let replacement_path = replacement_path(path)?;
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
    let (baseline_bytes, raw, mut document) = read_document(&replacement_path)?;
    super::validate_raw_for_edit(&replacement_path, &raw)?;
    apply(&mut document)?;
    let raw_new = document.to_string();
    super::validate_raw_for_edit(&replacement_path, &raw_new)?;
    if raw_new == raw {
        return Ok(UserSettingsEditOutcome::Unchanged);
    }
    let current_bytes = read_document_bytes(&replacement_path)?;
    if current_bytes != baseline_bytes {
        return Err(UserSettingsEditError::ConcurrentModification {
            path: replacement_path,
        });
    }
    let mut temporary = tempfile::NamedTempFile::new_in(replacement_parent).with_context(|| {
        format!(
            "create temporary settings file in {}",
            replacement_parent.display()
        )
    })?;
    temporary
        .write_all(raw_new.as_bytes())
        .context("write edited user settings")?;
    temporary.flush().context("flush edited user settings")?;
    temporary
        .as_file()
        .sync_all()
        .context("synchronize edited user settings")?;
    temporary
        .persist(&replacement_path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace user settings {}", replacement_path.display()))?;
    Ok(UserSettingsEditOutcome::Changed)
}

fn read_document_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
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

#[cfg(test)]
mod tests {
    use std::{
        fs::{File, OpenOptions},
        path::Path,
        sync::mpsc::{self, RecvTimeoutError},
        time::{Duration, Instant},
    };

    use gtl_application::ports::UserSettingsEditError;

    use super::{
        BEFORE_PERSIST_HOOK, BeforePersistHook, StringEdit, USER_SETTINGS_LOCK_WAIT_MAX, edit,
        lock_identity_path, lock_path, replacement_path,
    };

    const LOCK_ATTEMPT_WAIT_TEST_MAX: Duration = Duration::from_millis(100);
    const LOCK_HELD_OBSERVATION_WAIT: Duration = Duration::from_millis(50);
    const RESULT_WAIT_TEST_MAX: Duration = Duration::from_secs(8);

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
    fn set_returns_the_previous_string_and_preserves_unrelated_content() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "# viewer\ntheme = \"dark\"\n[push]\nconfirm = false\n",
        )
        .unwrap();

        let outcome = edit(&path, "theme", StringEdit::Set("light")).unwrap();
        let raw = std::fs::read_to_string(path).unwrap();

        assert_eq!(outcome.value_old.as_deref(), Some("dark"));
        assert!(outcome.document_changed);
        assert!(raw.contains("# viewer"));
        assert!(raw.contains("theme = \"light\""));
        assert!(raw.contains("[push]\nconfirm = false"));
    }

    #[test]
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

        let outcome = edit(&path, "theme", StringEdit::Set("light")).unwrap();

        assert_eq!(outcome.value_old.as_deref(), Some("dark"));
        assert!(outcome.document_changed);
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "theme = \"light\"\n"
        );
    }

    #[test]
    fn set_through_dangling_relative_symlink_creates_target() {
        let directory = tempfile::tempdir().unwrap();
        let config_directory = directory.path().join("config");
        let managed_directory = directory.path().join("managed");
        std::fs::create_dir_all(&config_directory).unwrap();
        std::fs::create_dir_all(&managed_directory).unwrap();
        let target = managed_directory.join("settings.toml");
        let path = config_directory.join("config.toml");
        create_file_symbolic_link(Path::new("../managed/settings.toml"), &path).unwrap();

        let outcome = edit(&path, "theme", StringEdit::Set("light")).unwrap();

        assert_eq!(outcome.value_old, None);
        assert!(outcome.document_changed);
        assert!(
            std::fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "theme = \"light\"\n"
        );
    }

    #[test]
    fn symbolic_link_cycle_is_bounded_and_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let path_other = directory.path().join("config-other.toml");
        create_file_symbolic_link(Path::new("config-other.toml"), &path).unwrap();
        create_file_symbolic_link(Path::new("config.toml"), &path_other).unwrap();

        let error = edit(&path, "theme", StringEdit::Set("light")).unwrap_err();

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

        let equal = edit(&path, "theme", StringEdit::Set("dark")).unwrap();
        let absent = edit(&path, "layout", StringEdit::Remove).unwrap();

        assert_eq!(equal.value_old.as_deref(), Some("dark"));
        assert!(!equal.document_changed);
        assert_eq!(absent.value_old, None);
        assert!(!absent.document_changed);
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn non_string_document_returns_the_typed_shape_error_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = "theme = 7\n";
        std::fs::write(&path, raw).unwrap();

        let error = edit(&path, "theme", StringEdit::Set("light")).unwrap_err();

        assert!(matches!(error, UserSettingsEditError::InvalidValueShape));
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn malformed_document_returns_a_typed_configuration_error_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let raw = "theme = {{{\n";
        std::fs::write(&path, raw).unwrap();

        let error = edit(&path, "theme", StringEdit::Set("light")).unwrap_err();

        assert!(matches!(
            error,
            UserSettingsEditError::InvalidConfiguration { path: error_path, .. }
                if error_path == path
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
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
            let result = edit(&path_worker, "theme", StringEdit::Set("light"));
            result_sender.send(result).unwrap();
        });
        let result = result_receiver.recv_timeout(RESULT_WAIT_TEST_MAX).unwrap();
        let elapsed = started_at.elapsed();
        drop(worker);
        let error = result.unwrap_err();

        assert!(matches!(
            error,
            UserSettingsEditError::LockTimeout {
                wait_seconds: 5,
                ..
            }
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
        let changed_raw = "theme = \"hearth\"\nlayout = \"split\"\n";
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
        let worker =
            std::thread::spawn(move || edit(&path_worker, "theme", StringEdit::Set("light")));

        ready_receiver.recv_timeout(RESULT_WAIT_TEST_MAX).unwrap();
        std::fs::write(&path, changed_raw).unwrap();
        continue_sender.send(()).unwrap();
        let error = worker.join().unwrap().unwrap_err();
        assert!(matches!(
            error,
            UserSettingsEditError::ConcurrentModification { path: error_path }
                if error_path == path
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), changed_raw);
    }

    #[test]
    fn concurrent_changes_to_different_keys_both_survive() {
        use gtl_application::ports::UserSettingsStore as _;

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
            let result = store_theme.set_value(gtl_models::settings::SettingKeyValue::Theme(
                gtl_models::viewer::Theme::Light,
            ));
            result_sender_theme.send(("theme", result)).unwrap();
        });

        let mut store_layout = store.clone();
        let attempt_sender_layout = attempt_sender.clone();
        let result_sender_layout = result_sender.clone();
        let layout_worker = std::thread::spawn(move || {
            attempt_sender_layout.send("layout").unwrap();
            let result = store_layout.set_value(gtl_models::settings::SettingKeyValue::Layout(
                gtl_models::viewer::DiffLayout::Split,
            ));
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
        assert_eq!(document["theme"].as_str(), Some("light"));
        assert_eq!(document["layout"].as_str(), Some("split"));
    }

    #[test]
    fn remove_returns_the_previous_string() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "density = \"full\"\n").unwrap();

        let outcome = edit(&path, "density", StringEdit::Remove).unwrap();

        assert_eq!(outcome.value_old.as_deref(), Some("full"));
        assert!(outcome.document_changed);
        assert!(!std::fs::read_to_string(path).unwrap().contains("density"));
    }

    #[test]
    fn absent_remove_does_not_create_the_config_target() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested").join("config.toml");

        let outcome = edit(&path, "density", StringEdit::Remove).unwrap();

        assert_eq!(outcome.value_old, None);
        assert!(!outcome.document_changed);
        assert!(!path.exists());
    }
}
