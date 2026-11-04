mod commands;
mod secrets;

use tauri::{plugin, Manager, RunEvent, Runtime, Url};

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
            let db_path = dir.join("rostery.db");
            commands::purge_old_youtube_data(&db_path);
            app.manage(commands::AppState {
                db_path,
                roast_running: Default::default(),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::totals,
            commands::load_demo,
            commands::clear_demo,
            commands::has_demo_data,
            commands::import_paths,
            commands::get_universe,
            commands::generate_roast,
            commands::latest_roast,
            commands::digest_preview,
            commands::creators_to_tag,
            commands::provider_statuses,
            commands::provider_disclosure,
            commands::get_settings,
            commands::save_settings,
            commands::default_system_prompt,
            commands::export_site,
            commands::open_site_preview,
            commands::reveal_site,
            commands::deploy_status,
            commands::save_deploy_token,
            commands::forget_deploy_token,
            commands::open_token_page,
            commands::deploy_github,
            commands::deploy_vercel,
            commands::connection_status,
            commands::connect_spotify,
            commands::sync_spotify,
            commands::disconnect_spotify,
            commands::connect_youtube,
            commands::sync_youtube,
            commands::disconnect_youtube,
            commands::open_policy_page
        ])
        .build(tauri::generate_context!())
        .expect("Rostery could not start")
        .run(|_, event| {
            // Otherwise a CLI runs on for up to 5 minutes with nobody to read its reply.
            // ponytail: a crash or SIGKILL skips this, and then the CLI finishes by itself.
            if let RunEvent::Exit = event {
                rostery_core::llm::stop_running();
            }
        });
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
