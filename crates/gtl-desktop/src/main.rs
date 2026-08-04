// Hide the console window on Windows release builds (no effect on Linux).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(error) = gtl_desktop::run() {
        eprintln!("gtl-viewer: {error:#}");
        std::process::exit(1);
    }
}
