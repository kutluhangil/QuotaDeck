// A release build must not open a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = quotadeck_app::statusline_helper::dispatch() {
        std::process::exit(code);
    }
    quotadeck_app::run();
}
