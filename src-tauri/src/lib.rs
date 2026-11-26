mod commands;
mod render;
mod windows;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::json;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_tray_base::{
    apply_window_settings, install_state, setup_tray, show_main, sync_autostart,
    was_launched_minimised, TrayBaseOptions, TrayBaseState, TraySetupOptions,
};

use commands::CaptureState;

const APP_NAME: &str = "WindowShot";
const STARTUP_SHOW_FALLBACK_MS: u64 = 3000;

static STARTUP_REVEALED: AtomicBool = AtomicBool::new(false);

fn startup_should_show(app: &AppHandle) -> bool {
    !was_launched_minimised()
        && !app
            .try_state::<TrayBaseState>()
            .map(|state| state.settings.lock().start_minimised)
            .unwrap_or(false)
}

fn reveal_main_on_startup(app: &AppHandle) {
    if STARTUP_REVEALED.swap(true, Ordering::SeqCst) {
        return;
    }
    show_main(app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    windows::enable_dpi_awareness();

    let builder = tauri_tray_base::with_common_plugins(tauri::Builder::default())
        // Replace the tray base's page-load hook: we reveal the main window here
        // only after its document has finished loading, so no blank frame flashes.
        .on_page_load(|webview, payload| {
            if payload.event() != PageLoadEvent::Finished
                || webview.label() != tauri_tray_base::MAIN_WINDOW_LABEL
            {
                return;
            }

            let app = webview.app_handle();
            if startup_should_show(app) {
                reveal_main_on_startup(app);
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            tauri_tray_base::settings_get,
            tauri_tray_base::settings_set,
            commands::list_windows,
            commands::overlay_bounds,
            commands::capture_window,
            commands::import_image,
            commands::restyle_image,
            commands::start_picker,
            commands::pick_window_at,
            commands::cancel_picker,
            commands::save_last,
        ])
        .setup(move |app| {
            let mut defaults = HashMap::new();
            defaults.insert("opacity".into(), json!(1.0));
            defaults.insert("alwaysOnTop".into(), json!(false));
            defaults.insert("startMinimised".into(), json!(false));

            install_state(
                app.handle(),
                TrayBaseOptions {
                    app_name: APP_NAME.into(),
                    settings_file_name: "window-shot-settings.json".into(),
                    defaults,
                    auto_show_main_on_ready: false,
                    ..Default::default()
                },
            )?;
            app.manage(CaptureState::default());
            register_shortcuts(app.handle());

            setup_tray(app.handle(), TraySetupOptions::default())?;
            apply_window_settings(app.handle());
            sync_autostart(app.handle());

            if let Some(main) = app.get_webview_window(tauri_tray_base::MAIN_WINDOW_LABEL) {
                #[cfg(windows)]
                {
                    let _ = main.set_decorations(false);
                    let _ = main.set_shadow(true);
                }
            }

            // WebView2 can occasionally skip the Finished event while the window is
            // hidden. Keep a bounded fallback so the app is never left invisible.
            let app_handle = app.handle().clone();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(STARTUP_SHOW_FALLBACK_MS));
                if startup_should_show(&app_handle) {
                    reveal_main_on_startup(&app_handle);
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            tauri_tray_base::on_window_event(window, event);
        });

    builder
        .run(tauri::generate_context!())
        .expect("error while running window-shot");
}

fn register_shortcuts(app: &AppHandle) {
    let shortcuts = [
        ("Ctrl+Shift+S", Modifiers::CONTROL | Modifiers::SHIFT),
        ("Alt+Shift+S", Modifiers::ALT | Modifiers::SHIFT),
    ];

    for (_label, modifiers) in shortcuts {
        let shortcut = Shortcut::new(Some(modifiers), Code::KeyS);
        let handle = app.clone();
        let _ = app
            .global_shortcut()
            .on_shortcut(shortcut, move |_, _, event| {
                if event.state == ShortcutState::Pressed {
                    commands::start_picker_impl(&handle, None, None);
                }
            });
    }
}
