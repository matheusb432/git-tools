// Hide the console window on Windows release builds (no effect on Linux).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gtl_desktop::run();
}
