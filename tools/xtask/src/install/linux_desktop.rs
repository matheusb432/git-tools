use std::{
    env, fs, io,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

const APP_ID: &str = "dev.gittools.viewer";
const APP_NAME: &str = "git-tools diff viewer";
const WM_CLASS: &str = "Gtl-viewer";
const ICON_SIZE: u32 = 512;

pub(super) fn install(repo: &Path, viewer: &Path) -> Result<Option<PathBuf>> {
    let Some(data_home) = xdg_data_home()? else {
        return Ok(None);
    };
    let icon = repo
        .join("crates")
        .join("desktop")
        .join("icons")
        .join("icon.png");
    install_files(&data_home, viewer, &icon)
        .with_context(|| format!("installing Linux desktop files for {}", viewer.display()))?;
    Ok(Some(desktop_entry_path(&data_home)))
}

pub(super) fn uninstall() -> Result<Vec<PathBuf>> {
    let Some(data_home) = xdg_data_home()? else {
        return Ok(Vec::new());
    };
    remove_files(&data_home).context("removing Linux desktop files for gtl-viewer")
}

fn desktop_entry(viewer: &Path) -> String {
    let exec = desktop_exec_arg(viewer);
    format!(
        "\
[Desktop Entry]
Type=Application
Name={APP_NAME}
GenericName=Diff Viewer
Comment=View git-tools diff previews
Exec={exec}
TryExec={exec}
Icon={APP_ID}
Terminal=false
Categories=Development;RevisionControl;
StartupNotify=false
StartupWMClass={WM_CLASS}
SingleMainWindow=true
"
    )
}

fn xdg_data_home() -> Result<Option<PathBuf>> {
    if env::consts::OS != "linux" {
        return Ok(None);
    }
    if let Some(data_home) = env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return Ok(Some(PathBuf::from(data_home)));
    }
    let home =
        env::var_os("HOME").context("HOME is not set; cannot install Linux desktop entry")?;
    Ok(Some(PathBuf::from(home).join(".local").join("share")))
}

fn install_files(data_home: &Path, viewer: &Path, icon_src: &Path) -> Result<()> {
    let desktop_path = desktop_entry_path(data_home);
    let icon_path = icon_path(data_home);

    if let Some(dir) = desktop_path.parent() {
        fs::create_dir_all(dir)?;
    }
    if let Some(dir) = icon_path.parent() {
        fs::create_dir_all(dir)?;
    }

    write_if_changed(&desktop_path, desktop_entry(viewer).as_bytes())?;
    write_resized_icon(icon_src, &icon_path)?;
    bust_icon_theme_mtime(data_home)?;
    Ok(())
}

fn remove_files(data_home: &Path) -> io::Result<Vec<PathBuf>> {
    let mut removed = Vec::new();
    for path in [desktop_entry_path(data_home), icon_path(data_home)] {
        if path.exists() {
            fs::remove_file(&path)?;
            removed.push(path);
        }
    }
    if !removed.is_empty() {
        bust_icon_theme_mtime(data_home)?;
    }
    Ok(removed)
}

fn desktop_entry_path(data_home: &Path) -> PathBuf {
    data_home
        .join("applications")
        .join(format!("{APP_ID}.desktop"))
}

fn icon_path(data_home: &Path) -> PathBuf {
    data_home
        .join("icons")
        .join("hicolor")
        .join("512x512")
        .join("apps")
        .join(format!("{APP_ID}.png"))
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if path.exists() && fs::read(path)? == bytes {
        return Ok(());
    }
    fs::write(path, bytes)
}

fn write_resized_icon(src: &Path, dst: &Path) -> Result<()> {
    let img = image::open(src).with_context(|| format!("reading app icon {}", src.display()))?;
    let resized = image::imageops::resize(
        &img,
        ICON_SIZE,
        ICON_SIZE,
        image::imageops::FilterType::Lanczos3,
    );
    resized
        .save(dst)
        .with_context(|| format!("writing app icon {}", dst.display()))?;
    Ok(())
}

fn bust_icon_theme_mtime(data_home: &Path) -> io::Result<()> {
    let theme_dir = data_home.join("icons").join("hicolor");
    fs::create_dir_all(&theme_dir)?;
    let marker = theme_dir.join(".gtl-viewer-icon-cache-bust");
    fs::write(&marker, b"")?;
    fs::remove_file(marker)
}

fn desktop_exec_arg(path: &Path) -> String {
    let raw = path.display().to_string();
    if raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
    {
        return raw;
    }
    let escaped = raw
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    #[test]
    fn desktop_entry_matches_gtl_viewer_identity() {
        let entry = super::desktop_entry(Path::new("/home/dev/.local/bin/gtl-viewer"));

        assert!(entry.contains("[Desktop Entry]\n"));
        assert!(entry.contains("Type=Application\n"));
        assert!(entry.contains("Name=git-tools diff viewer\n"));
        assert!(entry.contains("Exec=/home/dev/.local/bin/gtl-viewer\n"));
        assert!(entry.contains("TryExec=/home/dev/.local/bin/gtl-viewer\n"));
        assert!(entry.contains("Icon=dev.gittools.viewer\n"));
        assert!(entry.contains("StartupWMClass=Gtl-viewer\n"));
        assert!(entry.contains("SingleMainWindow=true\n"));
    }

    #[test]
    fn install_files_places_desktop_entry_and_icon_in_xdg_data_home() {
        let temp = tempfile::tempdir().unwrap();
        let data_home = temp.path().join("data");
        let viewer = temp.path().join("bin").join("gtl-viewer");
        let icon = temp.path().join("icon.png");
        fs::create_dir_all(viewer.parent().unwrap()).unwrap();
        fs::write(&viewer, b"viewer").unwrap();
        image::RgbaImage::from_pixel(1024, 1024, image::Rgba([1, 2, 3, 255]))
            .save(&icon)
            .unwrap();

        super::install_files(&data_home, &viewer, &icon).unwrap();

        let desktop = data_home
            .join("applications")
            .join("dev.gittools.viewer.desktop");
        let installed_icon = data_home
            .join("icons")
            .join("hicolor")
            .join("512x512")
            .join("apps")
            .join("dev.gittools.viewer.png");

        assert!(desktop.exists());
        assert_eq!(
            fs::read_to_string(desktop).unwrap(),
            super::desktop_entry(&viewer)
        );
        assert!(installed_icon.exists());
    }

    #[test]
    fn install_files_writes_a_512_square_png_icon() {
        let temp = tempfile::tempdir().unwrap();
        let data_home = temp.path().join("data");
        let viewer = temp.path().join("bin").join("gtl-viewer");
        let icon = temp.path().join("icon.png");
        fs::create_dir_all(viewer.parent().unwrap()).unwrap();
        fs::write(&viewer, b"viewer").unwrap();
        image::RgbaImage::from_pixel(1024, 1024, image::Rgba([1, 2, 3, 255]))
            .save(&icon)
            .unwrap();

        super::install_files(&data_home, &viewer, &icon).unwrap();

        let installed = image::open(super::icon_path(&data_home)).unwrap();
        assert_eq!(installed.width(), 512);
        assert_eq!(installed.height(), 512);
    }

    #[test]
    fn remove_files_deletes_desktop_entry_and_icon() {
        let temp = tempfile::tempdir().unwrap();
        let data_home = temp.path().join("data");
        let viewer = temp.path().join("bin").join("gtl-viewer");
        let icon = temp.path().join("icon.png");
        fs::create_dir_all(viewer.parent().unwrap()).unwrap();
        fs::write(&viewer, b"viewer").unwrap();
        image::RgbaImage::from_pixel(1024, 1024, image::Rgba([1, 2, 3, 255]))
            .save(&icon)
            .unwrap();
        super::install_files(&data_home, &viewer, &icon).unwrap();

        super::remove_files(&data_home).unwrap();

        assert!(!super::desktop_entry_path(&data_home).exists());
        assert!(!super::icon_path(&data_home).exists());
    }
}
