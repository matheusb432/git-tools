use std::{fs, io, path::Path};

pub fn copy_current_executable(destination: &Path) -> io::Result<u64> {
    fs::copy("/proc/self/exe", destination)
}
