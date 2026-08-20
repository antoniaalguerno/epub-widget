//! Estado de lectura por libro (marcadores por ahora; progreso y
//! preferencias son candidatos naturales para sumar acá en la Fase 4).
//!
//! Se guarda como un .json por libro en `$APPDATA/state/<id>.json` —
//! deliberadamente separado de `$APPDATA/library/<id>/`, que es la copia
//! descomprimida del epub tal cual y no debería mezclarse con datos propios
//! de la app.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    /// EpubCFI: puntero preciso de epub.js a un punto exacto del texto.
    pub cfi: String,
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BookState {
    #[serde(default)]
    pub bookmarks: Vec<Bookmark>,
    /// Última posición leída (EpubCFI) — `None` si todavía no se guardó
    /// ninguna. `#[serde(default)]` para que los `state/<id>.json` ya
    /// guardados (de antes de esta fase) sigan cargando bien sin este
    /// campo.
    #[serde(default)]
    pub progress_cfi: Option<String>,
}

fn state_path(state_dir: &Path, book_id: &str) -> PathBuf {
    state_dir.join(format!("{book_id}.json"))
}

pub fn load(state_dir: &Path, book_id: &str) -> Result<BookState, String> {
    let path = state_path(state_dir, book_id);
    if !path.is_file() {
        return Ok(BookState::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn save(state_dir: &Path, book_id: &str, state: &BookState) -> Result<(), String> {
    fs::create_dir_all(state_dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    fs::write(state_path(state_dir, book_id), raw).map_err(|e| e.to_string())
}

pub fn add_bookmark(
    state_dir: &Path,
    book_id: &str,
    cfi: String,
    label: String,
) -> Result<Vec<Bookmark>, String> {
    let mut state = load(state_dir, book_id)?;
    if !state.bookmarks.iter().any(|b| b.cfi == cfi) {
        state.bookmarks.push(Bookmark { cfi, label });
    }
    save(state_dir, book_id, &state)?;
    Ok(state.bookmarks)
}

pub fn remove_bookmark(state_dir: &Path, book_id: &str, cfi: &str) -> Result<Vec<Bookmark>, String> {
    let mut state = load(state_dir, book_id)?;
    state.bookmarks.retain(|b| b.cfi != cfi);
    save(state_dir, book_id, &state)?;
    Ok(state.bookmarks)
}

/// Guarda la posición actual de lectura, pisando la anterior si había.
pub fn save_progress(state_dir: &Path, book_id: &str, cfi: String) -> Result<(), String> {
    let mut state = load(state_dir, book_id)?;
    state.progress_cfi = Some(cfi);
    save(state_dir, book_id, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("epub-widget-state-test-{label}-{nanos}"))
    }

    #[test]
    fn libro_sin_estado_previo_no_tiene_marcadores() {
        let dir = unique_temp_dir("empty");
        let state = load(&dir, "abc123").unwrap();
        assert!(state.bookmarks.is_empty());
    }

    #[test]
    fn agregar_y_leer_un_marcador_persiste_entre_llamadas() {
        let dir = unique_temp_dir("add");
        add_bookmark(&dir, "libro1", "epubcfi(/6/4)".into(), "Capítulo 1".into()).unwrap();

        let state = load(&dir, "libro1").unwrap();
        assert_eq!(state.bookmarks.len(), 1);
        assert_eq!(state.bookmarks[0].cfi, "epubcfi(/6/4)");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn agregar_el_mismo_cfi_dos_veces_no_duplica() {
        let dir = unique_temp_dir("dup");
        add_bookmark(&dir, "libro1", "epubcfi(/6/4)".into(), "Cap 1".into()).unwrap();
        let bookmarks =
            add_bookmark(&dir, "libro1", "epubcfi(/6/4)".into(), "Cap 1 otra vez".into()).unwrap();

        assert_eq!(bookmarks.len(), 1);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn remover_un_marcador_lo_saca_de_la_lista() {
        let dir = unique_temp_dir("remove");
        add_bookmark(&dir, "libro1", "epubcfi(/6/4)".into(), "Cap 1".into()).unwrap();
        add_bookmark(&dir, "libro1", "epubcfi(/6/8)".into(), "Cap 2".into()).unwrap();

        let remaining = remove_bookmark(&dir, "libro1", "epubcfi(/6/4)").unwrap();

        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].cfi, "epubcfi(/6/8)");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn los_marcadores_son_independientes_por_libro() {
        let dir = unique_temp_dir("perbook");
        add_bookmark(&dir, "libro1", "epubcfi(/6/4)".into(), "Cap 1".into()).unwrap();

        let otro = load(&dir, "libro2").unwrap();
        assert!(otro.bookmarks.is_empty());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn libro_sin_progreso_previo_devuelve_none() {
        let dir = unique_temp_dir("progress-empty");
        let state = load(&dir, "abc123").unwrap();
        assert_eq!(state.progress_cfi, None);
    }

    #[test]
    fn guardar_progreso_persiste_entre_llamadas() {
        let dir = unique_temp_dir("progress-save");
        save_progress(&dir, "libro1", "epubcfi(/6/10)".into()).unwrap();

        let state = load(&dir, "libro1").unwrap();
        assert_eq!(state.progress_cfi.as_deref(), Some("epubcfi(/6/10)"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn guardar_progreso_nuevo_pisa_el_anterior() {
        let dir = unique_temp_dir("progress-overwrite");
        save_progress(&dir, "libro1", "epubcfi(/6/10)".into()).unwrap();
        save_progress(&dir, "libro1", "epubcfi(/6/20)".into()).unwrap();

        let state = load(&dir, "libro1").unwrap();
        assert_eq!(state.progress_cfi.as_deref(), Some("epubcfi(/6/20)"));

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn guardar_progreso_no_borra_los_marcadores_existentes() {
        let dir = unique_temp_dir("progress-keeps-bookmarks");
        add_bookmark(&dir, "libro1", "epubcfi(/6/4)".into(), "Cap 1".into()).unwrap();
        save_progress(&dir, "libro1", "epubcfi(/6/10)".into()).unwrap();

        let state = load(&dir, "libro1").unwrap();
        assert_eq!(state.bookmarks.len(), 1);
        assert_eq!(state.progress_cfi.as_deref(), Some("epubcfi(/6/10)"));

        fs::remove_dir_all(&dir).ok();
    }
}
