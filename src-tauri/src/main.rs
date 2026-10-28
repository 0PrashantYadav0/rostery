mod commands;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
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
            commands::clear_demo
        ])
        .run(tauri::generate_context!())
        .expect("Rostery could not start");
}
