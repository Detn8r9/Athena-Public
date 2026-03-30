// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    athena_lib::init_portable_app_data_override();
    athena_lib::run()
}
