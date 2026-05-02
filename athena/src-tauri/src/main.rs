// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = athena_lib::init_portable_app_data_override() {
        eprintln!("[athena] FATAL: portable / AthenaData initialization failed: {e}");
        std::process::exit(1);
    }
    athena_lib::run()
}
