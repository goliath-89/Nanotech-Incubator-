// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bridge;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let state = bridge::AppState::open(app)?;
            tauri::Manager::manage(app, state);
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
        ])
        .run(tauri::generate_context!())
        .expect("Strata could not start");
}
