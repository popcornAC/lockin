//! Lockin: a lean League of Legends companion (tray app).

mod assets;
mod import;
mod lcu;
mod opgg;
mod players;
mod premades;
mod service;
mod settings;

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, State, WindowEvent};

use import::ImportReport;
use service::{Service, Status};
use settings::Settings;

type Svc<'a> = State<'a, Arc<Service>>;

const AUTOSTART_ARG: &str = "--autostarted";

#[tauri::command]
fn get_status(svc: Svc) -> Status {
    svc.status()
}

#[tauri::command]
fn get_settings(svc: Svc) -> Settings {
    svc.settings.lock().unwrap().clone()
}

#[tauri::command]
fn set_settings(svc: Svc, settings: Settings) -> Result<Settings, String> {
    svc.set_settings(settings)
}

#[tauri::command]
async fn apply_now(svc: Svc<'_>) -> Result<ImportReport, String> {
    let svc = svc.inner().clone();
    svc.apply_now().await
}

#[tauri::command]
fn refresh_builds(svc: Svc) {
    svc.refresh_builds();
}

#[tauri::command]
fn get_assets(svc: Svc) -> Option<assets::GameAssets> {
    svc.assets()
}

#[tauri::command]
fn tiers() -> Vec<(&'static str, &'static str)> {
    opgg::builds::TIERS.to_vec()
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch just focuses the running instance.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main(app)
        }))
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_settings,
            set_settings,
            apply_now,
            refresh_builds,
            get_assets,
            tiers
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let svc = Service::new(handle.clone(), data_dir);
            // The window starts hidden; show it unless we were launched at login.
            if !std::env::args().any(|a| a == AUTOSTART_ARG) {
                show_main(&handle);
            }
            app.manage(svc.clone());
            tauri::async_runtime::spawn(svc.run());

            let show = MenuItem::with_id(app, "show", "Show Lockin", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Lockin")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, ev| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = ev
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it; the app keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
