mod commands;

use tauri::{plugin, Manager, Runtime, Url};

/// The app's own pages. Everything else opens in the system browser, so nothing can take the window
/// somewhere there is no way back from, such as "Open Link" on a link's context menu.
fn stays_in_app(url: &Url) -> bool {
    url.scheme() == "tauri" || matches!(url.host_str(), Some("localhost" | "tauri.localhost"))
}

fn stay_in_app<R: Runtime>() -> plugin::TauriPlugin<R> {
    plugin::Builder::new("stay-in-app")
        .on_navigation(|_, url| stays_in_app(url))
        .build()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Links are opened by the page on purpose, so it needs no script that catches them too.
        .plugin(
            tauri_plugin_opener::Builder::new()
                .open_js_links_on_click(false)
                .build(),
        )
        .plugin(stay_in_app())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_own_pages_load_in_its_window() {
        let allowed = |url: &str| stays_in_app(&Url::parse(url).unwrap());
        // The packaged app on macOS and Linux, on Windows, and pnpm tauri dev.
        assert!(allowed("tauri://localhost/index.html"));
        assert!(allowed("http://tauri.localhost/index.html"));
        assert!(allowed("http://localhost:5173/"));

        assert!(!allowed("https://open.spotify.com/artist/abc"));
        assert!(!allowed("https://www.youtube.com/watch?v=abc"));
        assert!(!allowed("https://localhost.evil.example/"));
        assert!(!allowed("https://evil.example/localhost"));
        assert!(!allowed("file:///etc/passwd"));
    }
}
