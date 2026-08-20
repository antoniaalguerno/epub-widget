// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
mod epub;
mod library;
mod state;

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

/// Puente de depuración: los errores de JS (excepciones no atrapadas,
/// promesas rechazadas, errores de epub.js) no aparecen en ninguna consola
/// que yo pueda ver desde acá, así que el frontend los reenvía por acá para
/// que salgan en la terminal de `tauri dev` junto con el resto de los logs.
#[tauri::command]
fn log_frontend_error(message: String) {
    eprintln!("[frontend error] {message}");
}

/// Igual que `log_frontend_error` pero para diagnóstico normal (no
/// necesariamente un error) — usado mientras se depura el theming del
/// lector, para ver qué está pasando sin pedirle a la usuaria que abra
/// devtools cada vez.
#[tauri::command]
fn log_frontend_info(message: String) {
    eprintln!("[frontend info] {message}");
}

/// Descomprime (si hace falta) un .epub bajo la carpeta de librería de la
/// app, devuelve su metadata básica y lo registra en el índice de
/// biblioteca (Fase 4a) para que aparezca en el dashboard de "recientes".
#[tauri::command]
async fn import_epub(app: tauri::AppHandle, path: String) -> Result<epub::EpubMeta, String> {
    let library_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("library");
    std::fs::create_dir_all(&library_dir).map_err(|e| e.to_string())?;
    let meta = epub::import(std::path::Path::new(&path), &library_dir)?;

    library::upsert(
        &state_dir(&app)?,
        library::NewBook {
            id: meta.id.clone(),
            title: meta.title.clone(),
            author: meta.author.clone(),
            cover_path: meta.cover_path.clone(),
            book_dir: meta.book_dir.clone(),
        },
    )?;

    Ok(meta)
}

/// Lista la biblioteca completa (recientes primero) para el dashboard.
#[tauri::command]
fn list_library(app: tauri::AppHandle) -> Result<Vec<library::LibraryEntry>, String> {
    library::list(&state_dir(&app)?)
}

/// Se llama apenas se abre un libro (recién importado o reabierto desde la
/// biblioteca) para que el dashboard sepa qué mostrar primero en
/// "recientes".
#[tauri::command]
fn open_book(app: tauri::AppHandle, book_id: String) -> Result<(), String> {
    library::mark_opened(&state_dir(&app)?, &book_id)
}

/// El frontend llama esto cada tanto (y al perder foco / cerrar el libro)
/// con los segundos leídos desde el último llamado — ver el efecto de
/// montaje del lector en `+page.svelte`. Se acumula por libro y el
/// dashboard suma todos los libros para mostrar "horas leídas".
#[tauri::command]
fn add_reading_time(app: tauri::AppHandle, book_id: String, seconds: u64) -> Result<(), String> {
    library::add_reading_time(&state_dir(&app)?, &book_id, seconds)
}

#[tauri::command]
fn get_progress(app: tauri::AppHandle, book_id: String) -> Result<Option<String>, String> {
    Ok(state::load(&state_dir(&app)?, &book_id)?.progress_cfi)
}

#[tauri::command]
fn save_progress(app: tauri::AppHandle, book_id: String, cfi: String) -> Result<(), String> {
    state::save_progress(&state_dir(&app)?, &book_id, cfi)
}

fn state_dir(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("state");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

#[tauri::command]
fn get_bookmarks(app: tauri::AppHandle, book_id: String) -> Result<Vec<state::Bookmark>, String> {
    Ok(state::load(&state_dir(&app)?, &book_id)?.bookmarks)
}

#[tauri::command]
fn add_bookmark(
    app: tauri::AppHandle,
    book_id: String,
    cfi: String,
    label: String,
) -> Result<Vec<state::Bookmark>, String> {
    state::add_bookmark(&state_dir(&app)?, &book_id, cfi, label)
}

#[tauri::command]
fn remove_bookmark(
    app: tauri::AppHandle,
    book_id: String,
    cfi: String,
) -> Result<Vec<state::Bookmark>, String> {
    state::remove_bookmark(&state_dir(&app)?, &book_id, &cfi)
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
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            import_epub,
            log_frontend_error,
            log_frontend_info,
            get_bookmarks,
            add_bookmark,
            remove_bookmark,
            list_library,
            open_book,
            get_progress,
            save_progress,
            add_reading_time
        ])
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
