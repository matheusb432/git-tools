//! Replaces the launcher icon that `just install` placed in the user's hicolor icon theme.
//!
//! GNOME Shell draws the dock and app grid icon from these files, not from the window, so the
//! viewer rewrites them to follow its theme. Paths match
//! `xtask/src/verbs/install/linux_desktop.rs`.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use super::ThemeIcons;

/// Rewrites the installed icons of `app_id` and reports whether any file changed.
///
/// An absent icon means this build is not installed, so nothing is created.
pub(super) fn replace_installed(app_id: &str, icons: &ThemeIcons) -> io::Result<bool> {
    let Some(hicolor) = hicolor_directory() else {
        return Ok(false);
    };
    let mut changed = false;
    for (path, bytes) in [
        (
            hicolor.join("scalable/apps").join(format!("{app_id}.svg")),
            icons.launcher_svg,
        ),
        (
            hicolor.join("512x512/apps").join(format!("{app_id}.png")),
            icons.launcher_png,
        ),
    ] {
        changed |= replace_file(&path, bytes)?;
    }
    if changed {
        // GNOME reloads icons after the theme directory's modification time changes.
        let marker = hicolor.join(".gtl-viewer-icon-cache-bust");
        fs::write(&marker, b"")?;
        fs::remove_file(marker)?;
    }
    Ok(changed)
}

fn hicolor_directory() -> Option<PathBuf> {
    let data_home = env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))?;
    Some(data_home.join("icons/hicolor"))
}

fn replace_file(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    match fs::read(path) {
        Ok(current) if current == bytes => Ok(false),
        Ok(_) => {
            let name = path
                .file_name()
                .map_or_else(Default::default, |name| name.to_string_lossy().into_owned());
            let replacement = path.with_file_name(format!(".{name}.theme"));
            fs::write(&replacement, bytes)?;
            fs::rename(&replacement, path)?;
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::replace_file;

    #[test]
    fn only_an_installed_icon_with_other_bytes_is_replaced() {
        let directory = tempfile::tempdir().unwrap();
        let icon = directory.path().join("dev.gittools.viewer.svg");

        assert!(!replace_file(&icon, b"themed").unwrap());
        assert!(!icon.exists(), "an uninstalled icon must stay absent");

        std::fs::write(&icon, b"installed").unwrap();
        assert!(replace_file(&icon, b"themed").unwrap());
        assert_eq!(std::fs::read(&icon).unwrap(), b"themed");
        assert!(!replace_file(&icon, b"themed").unwrap());
        assert_eq!(
            std::fs::read_dir(directory.path()).unwrap().count(),
            1,
            "the replacement file must not remain"
        );
    }
}
