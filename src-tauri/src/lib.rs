// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod epub;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Descomprime (si hace falta) un .epub bajo la carpeta de librería de la
/// app y devuelve su metadata básica (Fase 2: todavía no hay UI de lectura,
/// solo extracción + parseo seguros).
#[tauri::command]
async fn import_epub(app: tauri::AppHandle, path: String) -> Result<epub::EpubMeta, String> {
    let library_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("library");
    std::fs::create_dir_all(&library_dir).map_err(|e| e.to_string())?;
    epub::import(std::path::Path::new(&path), &library_dir)
}

/// Muestra la ventana principal si estaba oculta, o la oculta si estaba visible.
/// Es el mismo toggle que usan tanto el click izquierdo del tray como el item de menu.
fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let is_visible = window.is_visible().unwrap_or(false);
        if is_visible {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .invoke_handler(tauri::generate_handler![greet, import_epub])
        .setup(|app| {
            let toggle_item =
                MenuItem::with_id(app, "toggle", "Mostrar/Ocultar", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle" => toggle_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Cerrar la ventana (Alt+F4, etc.) la oculta en vez de matar la app:
            // el widget sigue vivo en el tray y se sale solo desde "Salir".
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
