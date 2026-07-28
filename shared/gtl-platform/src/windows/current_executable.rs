use std::{env, fs, io, path::Path};

pub fn copy_current_executable(destination: &Path) -> io::Result<u64> {
    fs::copy(env::current_exe()?, destination)
}
