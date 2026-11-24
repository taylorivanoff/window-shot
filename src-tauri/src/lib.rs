mod commands;
mod render;
mod windows;

use std::collections::HashMap;

use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_tray_base::{
    apply_window_settings, install_state, setup_tray, sync_autostart, TrayBaseOptions,
    TraySetupOptions,
};

use commands::CaptureState;

const APP_NAME: &str = "WindowShot";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    windows::enable_dpi_awareness();

    let builder = tauri_tray_base::with_common_plugins(tauri::Builder::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            tauri_tray_base::settings_get,
            tauri_tray_base::settings_set,
            commands::list_windows,
            commands::overlay_bounds,
            commands::capture_window,
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
