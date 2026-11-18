// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;
mod elevated;
mod preview;
mod window;

fn main() {
    // The elevated copy (Windows fast scan) does its one job and exits
    // before any window or web view exists.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(elevated::FLAG) {
        std::process::exit(elevated::run_elevated_copy(&args));
    }

    tauri::Builder::default()
        .setup(|app| {
            let state = bridge::AppState::open(app)?;
            tauri::Manager::manage(app, state);
            let material = window::create(app)?;
            tauri::Manager::manage(app, material);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bridge::list_volumes,
            bridge::start_scan,
            bridge::cancel_scan,
            bridge::get_dir,
            bridge::latest_scan,
            bridge::get_ring,
            bridge::get_apps,
            bridge::get_app_dirs,
            bridge::largest_files,
            bridge::reveal,
            bridge::get_suggestions,
            bridge::clean_up,
            bridge::open_trash,
            preview::preview_file,
            bridge::list_exclusions,
            bridge::exclude_folder,
            bridge::include_folder,
            bridge::list_scans,
            bridge::compare_scans,
            bridge::list_cleanups,
            bridge::app_info,
            window::window_material,
        ])
        .run(tauri::generate_context!())
        .expect("Strata could not start");
}
