//! Detaches FUSE mounts that session services leave inside the sandbox.
//!
//! Each private D-Bus session activates `xdg-document-portal` and `gvfsd-fuse`,
//! which mount `doc` and `gvfs` under the sandbox `XDG_RUNTIME_DIR`. Stopping a
//! session kills its process group before those services unmount, and the
//! sandbox directory cannot be removed while the stale mounts remain.

use std::{
    cmp::Reverse,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MOUNT_TABLE_PATH: &str = "/proc/self/mountinfo";
const UNMOUNT_PROGRAMS: [&str; 2] = ["fusermount3", "fusermount"];

/// Lazily unmounts every FUSE mount under `root`, deepest first.
pub(super) fn unmount_fuse_mounts_under(root: &Path) {
    if env::consts::OS != "linux" {
        return;
    }
    let Ok(table) = fs::read_to_string(MOUNT_TABLE_PATH) else {
        eprintln!("desktop-e2e: failed to read {MOUNT_TABLE_PATH}");
        return;
    };
    let root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    for mount_point in fuse_mount_points_under(&table, &root) {
        if !unmount(&mount_point) {
            eprintln!("desktop-e2e: failed to unmount {}", mount_point.display());
        }
    }
}

fn unmount(mount_point: &Path) -> bool {
    UNMOUNT_PROGRAMS.into_iter().any(|program| {
        Command::new(program)
            .args(["-u", "-z"])
            .arg(mount_point)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

/// Lists the FUSE mount points under `root` in a `/proc/self/mountinfo` table, deepest first.
///
/// A path mounted more than once appears once per mount, because each unmount detaches only the top
/// one.
fn fuse_mount_points_under(table: &str, root: &Path) -> Vec<PathBuf> {
    let mut mount_points = table
        .lines()
        .filter_map(|line| {
            let fields = line.split(' ').collect::<Vec<_>>();
            let separator = fields.iter().position(|field| *field == "-")?;
            let file_system_type = fields.get(separator + 1)?;
            let mount_point = PathBuf::from(unescape_mount_field(fields.get(4)?)?);
            (file_system_type.starts_with("fuse") && mount_point.starts_with(root))
                .then_some(mount_point)
        })
        .collect::<Vec<_>>();
    mount_points.sort_by_key(|mount_point| Reverse(mount_point.components().count()));
    mount_points
}

/// Decodes the `\NNN` octal escapes that the mount table uses for spaces, tabs, newlines, and
/// backslashes.
fn unescape_mount_field(field: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(field.len());
    let mut remaining = field.as_bytes();
    while let Some((&byte, rest)) = remaining.split_first() {
        match (byte, rest) {
            (b'\\', [first, second, third, after @ ..])
                if [first, second, third]
                    .iter()
                    .all(|digit| (b'0'..=b'7').contains(*digit)) =>
            {
                let value = [first, second, third]
                    .iter()
                    .fold(0_u16, |value, digit| value * 8 + u16::from(**digit - b'0'));
                bytes.push(u8::try_from(value).ok()?);
                remaining = after;
            }
            _ => {
                bytes.push(byte);
                remaining = rest;
            }
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
25 1 259:2 / / rw,relatime shared:1 - ext4 /dev/nvme0n1p2 rw
61 25 0:52 / /run/user/1000/doc rw,nosuid,nodev,relatime shared:600 - fuse.portal portal rw,user_id=1000
70 25 0:60 / /tmp/gtl-viewer-e2e-a1/xdg/runtime/doc rw,nosuid,nodev - fuse.portal portal rw
71 25 0:61 / /tmp/gtl-viewer-e2e-a1/xdg/runtime/gvfs rw,nosuid,nodev - fuse.gvfsd-fuse gvfsd-fuse rw
72 71 0:62 / /tmp/gtl-viewer-e2e-a1/xdg/runtime/gvfs rw,nosuid,nodev - fuse.gvfsd-fuse gvfsd-fuse rw
73 25 0:63 / /tmp/gtl-viewer-e2e-a1/xdg/runtime/doc/by-app rw shared:9 master:3 - fuse.portal portal rw
74 25 0:64 / /tmp/gtl-viewer-e2e-a1/scratch rw - tmpfs tmpfs rw
75 25 0:65 / /tmp/gtl-viewer-e2e-a10/xdg/runtime/doc rw - fuse.portal portal rw
76 25 0:66 / /tmp/gtl-viewer-e2e-a1/with\\040space rw - fuse.sshfs host:/ rw";

    #[test]
    fn lists_only_fuse_mounts_inside_the_root_deepest_first() {
        let mount_points = fuse_mount_points_under(TABLE, Path::new("/tmp/gtl-viewer-e2e-a1"));

        assert_eq!(
            mount_points,
            [
                "/tmp/gtl-viewer-e2e-a1/xdg/runtime/doc/by-app",
                "/tmp/gtl-viewer-e2e-a1/xdg/runtime/doc",
                "/tmp/gtl-viewer-e2e-a1/xdg/runtime/gvfs",
                "/tmp/gtl-viewer-e2e-a1/xdg/runtime/gvfs",
                "/tmp/gtl-viewer-e2e-a1/with space",
            ]
            .map(PathBuf::from)
        );
    }

    #[test]
    fn unescape_decodes_octal_escapes_and_keeps_other_backslashes() {
        assert_eq!(
            unescape_mount_field(r"a\040b\011c\134d\e").as_deref(),
            Some("a b\tc\\d\\e")
        );
        assert_eq!(unescape_mount_field(r"\777"), None);
    }
}
