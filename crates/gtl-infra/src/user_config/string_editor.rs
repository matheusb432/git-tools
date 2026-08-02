use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    io::Write as _,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::Context as _;
use gtl_application::ports::UserSettingsEditError;
use toml_edit::{DocumentMut, Item, Value};

const USER_SETTINGS_LOCK_WAIT_MAX: Duration = Duration::from_millis(500);
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

fn lock_path(settings_path: &Path) -> PathBuf {
    let mut value = OsString::from(settings_path.as_os_str());
    value.push(".lock");
    PathBuf::from(value)
}

fn acquire_lock(settings_path: &Path) -> anyhow::Result<File> {
    let path = lock_path(settings_path);
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("open user-settings lock {}", path.display()))?;
    let deadline = Instant::now() + USER_SETTINGS_LOCK_WAIT_MAX;
    let mut acquisition_attempt_initial = true;

    loop {
        if !acquisition_attempt_initial && Instant::now() >= deadline {
            anyhow::bail!(
                "user-settings lock {} was not acquired within 500 ms",
                path.display()
            );
        }
        acquisition_attempt_initial = false;

        match lock.try_lock() {
            Ok(()) => return Ok(lock),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(error)) => {
                return Err(error)
                    .with_context(|| format!("lock user settings {}", path.display()));
            }
        }

        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() {
            std::thread::sleep(remaining.min(USER_SETTINGS_LOCK_POLL_INTERVAL));
        }
    }
}

fn read_document(path: &Path) -> anyhow::Result<DocumentMut> {
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(error).with_context(|| format!("read user settings {}", path.display()));
        }
    };
    raw.parse::<DocumentMut>()
        .with_context(|| format!("parse user settings {}", path.display()))
}

fn replacement_path(path: &Path) -> anyhow::Result<PathBuf> {
    let mut replacement = path.to_path_buf();
    for _ in 0..USER_SETTINGS_SYMBOLIC_LINK_DEPTH_MAX {
        let metadata = match std::fs::symlink_metadata(&replacement) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(replacement),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "inspect user-settings replacement target {}",
                        replacement.display()
                    )
                });
            }
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

    let _lock = acquire_lock(path)?;
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
    let mut document = read_document(&replacement_path)?;
    let value_old = value_old(&document, key)?;

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

    let mut temporary = tempfile::NamedTempFile::new_in(replacement_parent).with_context(|| {
        format!(
            "create temporary settings file in {}",
            replacement_parent.display()
        )
    })?;
    temporary
        .write_all(document.to_string().as_bytes())
        .with_context(|| {
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

#[cfg(test)]
mod tests {
    use std::{
        fs::{File, OpenOptions},
        path::Path,
        sync::mpsc::{self, RecvTimeoutError},
        time::{Duration, Instant},
    };

    use super::{StringEdit, USER_SETTINGS_LOCK_WAIT_MAX, edit, lock_path};

    const LOCK_ATTEMPT_WAIT_TEST_MAX: Duration = Duration::from_millis(100);
    const LOCK_HELD_OBSERVATION_WAIT: Duration = Duration::from_millis(50);
    const RESULT_WAIT_TEST_MAX: Duration = Duration::from_secs(2);

    #[cfg(unix)]
    fn create_file_symbolic_link(original: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(original, link)
    }

    #[cfg(windows)]
    fn create_file_symbolic_link(original: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_file(original, link)
    }

    fn hold_settings_lock(path: &Path) -> File {
        let lock = OpenOptions::new()
            .create(true)
            .read(true)
            .truncate(false)
            .write(true)
            .open(lock_path(path))
            .expect("open lock");
        lock.lock().expect("hold lock");
        lock
    }

    #[test]
    fn set_returns_the_previous_string_and_preserves_unrelated_content() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        std::fs::write(
            &path,
            "# viewer\ntheme = \"dark\"\n[push]\nconfirm = false\n",
        )
        .expect("seed config");

        let outcome = edit(&path, "theme", StringEdit::Set("light")).expect("set string");
        let raw = std::fs::read_to_string(path).expect("updated config");

        assert_eq!(outcome.value_old.as_deref(), Some("dark"));
        assert!(outcome.document_changed);
        assert!(raw.contains("# viewer"));
        assert!(raw.contains("theme = \"light\""));
        assert!(raw.contains("[push]\nconfirm = false"));
    }

    #[test]
    fn set_through_relative_symlink_preserves_link_and_updates_target() {
        let directory = tempfile::tempdir().expect("temp directory");
        let config_directory = directory.path().join("config");
        let managed_directory = directory.path().join("managed");
        std::fs::create_dir_all(&config_directory).expect("create config directory");
        std::fs::create_dir_all(&managed_directory).expect("create managed directory");
        let target = managed_directory.join("settings.toml");
        let path = config_directory.join("config.toml");
        std::fs::write(&target, "theme = \"dark\"\n").expect("seed managed config");
        create_file_symbolic_link(Path::new("../managed/settings.toml"), &path)
            .expect("link managed config");

        let outcome = edit(&path, "theme", StringEdit::Set("light")).expect("set theme");

        assert_eq!(outcome.value_old.as_deref(), Some("dark"));
        assert!(outcome.document_changed);
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("config link metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("managed config"),
            "theme = \"light\"\n"
        );
    }

    #[test]
    fn set_through_dangling_relative_symlink_creates_target() {
        let directory = tempfile::tempdir().expect("temp directory");
        let config_directory = directory.path().join("config");
        let managed_directory = directory.path().join("managed");
        std::fs::create_dir_all(&config_directory).expect("create config directory");
        std::fs::create_dir_all(&managed_directory).expect("create managed directory");
        let target = managed_directory.join("settings.toml");
        let path = config_directory.join("config.toml");
        create_file_symbolic_link(Path::new("../managed/settings.toml"), &path)
            .expect("link managed config");

        let outcome = edit(&path, "theme", StringEdit::Set("light")).expect("set theme");

        assert_eq!(outcome.value_old, None);
        assert!(outcome.document_changed);
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("config link metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target).expect("managed config"),
            "theme = \"light\"\n"
        );
    }

    #[test]
    fn symbolic_link_cycle_is_bounded_and_untouched() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let path_other = directory.path().join("config-other.toml");
        create_file_symbolic_link(Path::new("config-other.toml"), &path).expect("link config");
        create_file_symbolic_link(Path::new("config.toml"), &path_other)
            .expect("link other config");

        let error = edit(&path, "theme", StringEdit::Set("light"))
            .expect_err("symbolic-link cycle must fail");

        assert!(format!("{error:#}").contains("exceeds 40 symbolic links"));
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("config link metadata")
                .file_type()
                .is_symlink()
        );
        assert!(
            std::fs::symlink_metadata(&path_other)
                .expect("other config link metadata")
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn equal_set_and_absent_remove_do_not_rewrite_config() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let raw = "theme = \"dark\"\n";
        std::fs::write(&path, raw).expect("seed config");

        let equal = edit(&path, "theme", StringEdit::Set("dark")).expect("equal set");
        let absent = edit(&path, "layout", StringEdit::Remove).expect("absent remove");

        assert_eq!(equal.value_old.as_deref(), Some("dark"));
        assert!(!equal.document_changed);
        assert_eq!(absent.value_old, None);
        assert!(!absent.document_changed);
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn non_string_and_malformed_documents_remain_unchanged() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");

        for raw in ["theme = 7\n", "theme = {{{\n"] {
            std::fs::write(&path, raw).expect("seed config");
            assert!(edit(&path, "theme", StringEdit::Set("light")).is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
        }
    }

    #[test]
    fn edit_stops_when_the_lock_budget_expires() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let raw = "theme = \"dark\"\n";
        std::fs::write(&path, raw).expect("seed config");
        let _lock = hold_settings_lock(&path);
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        let path_worker = path.clone();

        let started_at = Instant::now();
        let worker = std::thread::spawn(move || {
            let result = edit(&path_worker, "theme", StringEdit::Set("light"));
            result_sender.send(result).expect("send edit result");
        });
        let result = result_receiver
            .recv_timeout(RESULT_WAIT_TEST_MAX)
            .expect("edit returns within the test timeout");
        let elapsed = started_at.elapsed();
        drop(worker);
        let error = result.expect_err("held lock times out");

        assert!(format!("{error:#}").contains("within 500 ms"));
        assert!(
            elapsed >= USER_SETTINGS_LOCK_WAIT_MAX,
            "elapsed: {elapsed:?}"
        );
        assert!(elapsed < RESULT_WAIT_TEST_MAX, "elapsed: {elapsed:?}");
        assert_eq!(std::fs::read_to_string(path).unwrap(), raw);
    }

    #[test]
    fn concurrent_changes_to_different_keys_both_survive() {
        use gtl_application::ports::UserSettingsEditor as _;

        use crate::user_config::TomlSettingsStore;

        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        let store = TomlSettingsStore::new(Some(path.clone()));
        let lock = hold_settings_lock(&path);
        let (attempt_sender, attempt_receiver) = mpsc::sync_channel(2);
        let (result_sender, result_receiver) = mpsc::sync_channel(2);

        let store_theme = store.clone();
        let attempt_sender_theme = attempt_sender.clone();
        let result_sender_theme = result_sender.clone();
        let theme_worker = std::thread::spawn(move || {
            attempt_sender_theme
                .send("theme")
                .expect("send theme attempt");
            let result = store_theme.set_string("theme", "light");
            result_sender_theme
                .send(("theme", result))
                .expect("send theme result");
        });

        let store_layout = store.clone();
        let attempt_sender_layout = attempt_sender.clone();
        let result_sender_layout = result_sender.clone();
        let layout_worker = std::thread::spawn(move || {
            attempt_sender_layout
                .send("layout")
                .expect("send layout attempt");
            let result = store_layout.set_string("layout", "split");
            result_sender_layout
                .send(("layout", result))
                .expect("send layout result");
        });
        drop(attempt_sender);
        drop(result_sender);

        let attempt_first = attempt_receiver
            .recv_timeout(LOCK_ATTEMPT_WAIT_TEST_MAX)
            .expect("first editor begins within the test timeout");
        let attempt_second = attempt_receiver
            .recv_timeout(LOCK_ATTEMPT_WAIT_TEST_MAX)
            .expect("second editor begins within the test timeout");
        assert_ne!(attempt_first, attempt_second);

        let result_while_locked = result_receiver.recv_timeout(LOCK_HELD_OBSERVATION_WAIT);
        assert!(
            matches!(result_while_locked, Err(RecvTimeoutError::Timeout)),
            "an editor completed while the sibling lock was held: {result_while_locked:?}"
        );
        lock.unlock().expect("release lock");

        let result_first = result_receiver
            .recv_timeout(RESULT_WAIT_TEST_MAX)
            .expect("first editor finishes within the test timeout");
        let result_second = result_receiver
            .recv_timeout(RESULT_WAIT_TEST_MAX)
            .expect("second editor finishes within the test timeout");
        drop(theme_worker);
        drop(layout_worker);

        let mut tags = Vec::with_capacity(2);
        for (tag, result) in [result_first, result_second] {
            assert_eq!(result.expect("set string"), None, "{tag} previous value");
            tags.push(tag);
        }
        tags.sort_unstable();
        assert_eq!(tags, ["layout", "theme"]);

        let raw = std::fs::read_to_string(path).expect("updated config");
        let document = toml::from_str::<toml::Value>(&raw).expect("valid TOML");
        assert_eq!(document["theme"].as_str(), Some("light"));
        assert_eq!(document["layout"].as_str(), Some("split"));
    }

    #[test]
    fn remove_returns_the_previous_string() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("config.toml");
        std::fs::write(&path, "density = \"full\"\n").expect("seed config");

        let outcome = edit(&path, "density", StringEdit::Remove).expect("remove density");

        assert_eq!(outcome.value_old.as_deref(), Some("full"));
        assert!(outcome.document_changed);
        assert!(!std::fs::read_to_string(path).unwrap().contains("density"));
    }

    #[test]
    fn absent_remove_does_not_create_the_config_target() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("nested").join("config.toml");

        let outcome = edit(&path, "density", StringEdit::Remove).expect("remove absent key");

        assert_eq!(outcome.value_old, None);
        assert!(!outcome.document_changed);
        assert!(!path.exists());
    }
}
