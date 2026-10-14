// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(result) = retoucan_lib::cli::run(&args) {
        if let Err(e) = result {
            eprintln!("retoucan: {e:#}");
            std::process::exit(1);
        }
        return;
    }
    retoucan_lib::run()
}
