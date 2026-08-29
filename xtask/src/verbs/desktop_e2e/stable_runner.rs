use std::{fs, io, path::Path};

#[cfg(target_os = "linux")]
pub(super) fn copy_current_executable(destination: &Path) -> io::Result<u64> {
    fs::copy("/proc/self/exe", destination)
}

#[cfg(not(target_os = "linux"))]
pub(super) fn copy_current_executable(destination: &Path) -> io::Result<u64> {
    fs::copy(std::env::current_exe()?, destination)
}

#[cfg(test)]
mod tests {
    use super::copy_current_executable;

    #[test]
    fn copies_the_running_xtask_to_a_stable_path() {
        let temporary_directory = tempfile::tempdir().unwrap();
        let destination = temporary_directory.path().join("xtask-copy");

        let copied = copy_current_executable(&destination).unwrap();

        assert!(copied > 0);
        assert!(destination.is_file());
    }
}
