mod commands;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            app.manage(commands::AppState {
                db_path: dir.join("rostery.db"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::totals,
            commands::load_demo,
            commands::clear_demo,
            commands::has_demo_data,
            commands::import_paths,
            commands::get_universe
        ])
        .run(tauri::generate_context!())
        .expect("Rostery could not start");
}
