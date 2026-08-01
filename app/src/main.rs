// A release build must not open a console window on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(code) = quotadeck_app::statusline_helper::dispatch() {
        std::process::exit(code);
    }
    if let Err(error) = quotadeck_app::run() {
        eprintln!("quotadeck: failed to start: {error}");
        quotadeck_app::report_startup_error(&error.to_string());
        std::process::exit(1);
    }
}
