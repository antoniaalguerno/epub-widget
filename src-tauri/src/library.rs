//! Índice global de la biblioteca: qué libros se importaron y cuándo se
//! abrieron por última vez. Vive aparte de `state.rs` (que guarda
//! marcadores/progreso *de un libro puntual*) porque esto es un listado de
//! toda la biblioteca, no el estado interno de un libro — se persiste en
//! `$APPDATA/state/library.json`, separado también de
//! `$APPDATA/library/<id>/`, que es la copia descomprimida del epub tal
//! cual (ver comentario de cabecera de `state.rs`).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LibraryEntry {
    pub id: String,
    pub title: String,
    pub author: String,
    pub cover_path: Option<String>,
    pub book_dir: String,
    /// Unix timestamp (segundos) de cuándo se importó por primera vez.
    pub imported_at: i64,
    /// Unix timestamp (segundos) de la última vez que se abrió para leer;
    /// `None` si se importó pero nunca se abrió (no debería pasar en la
    /// práctica — importar abre de una — pero se modela igual por las
    /// dudas).
    #[serde(default)]
    pub last_opened_at: Option<i64>,
    /// Tiempo de lectura acumulado, en segundos (Fase 4b). `#[serde(default)]`
    /// para que los `library.json` de la Fase 4a (sin este campo) sigan
    /// cargando bien.
    #[serde(default)]
    pub total_seconds_read: u64,
}

/// Datos que trae un import recién hecho — sin timestamps, esos los decide
/// este módulo.
pub struct NewBook {
    pub id: String,
    pub title: String,
    pub author: String,
    pub cover_path: Option<String>,
    pub book_dir: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Library {
    #[serde(default)]
    books: Vec<LibraryEntry>,
}

fn index_path(state_dir: &Path) -> PathBuf {
    state_dir.join("library.json")
}

fn load(state_dir: &Path) -> Result<Library, String> {
    let path = index_path(state_dir);
    if !path.is_file() {
        return Ok(Library::default());
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

fn save(state_dir: &Path, library: &Library) -> Result<(), String> {
    fs::create_dir_all(state_dir).map_err(|e| e.to_string())?;
    let raw = serde_json::to_string_pretty(library).map_err(|e| e.to_string())?;
    fs::write(index_path(state_dir), raw).map_err(|e| e.to_string())
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Agrega un libro nuevo a la biblioteca, o actualiza su metadata si ya
/// estaba (mismo `id` — un epub ya importado no se duplica, ver
/// `epub::content_id`). `imported_at` se conserva del primer import; solo
/// se pisa título/autor/portada/carpeta por si el parseo cambió.
pub fn upsert(state_dir: &Path, book: NewBook) -> Result<(), String> {
    let mut library = load(state_dir)?;
    match library.books.iter_mut().find(|b| b.id == book.id) {
        Some(existing) => {
            existing.title = book.title;
            existing.author = book.author;
            existing.cover_path = book.cover_path;
            existing.book_dir = book.book_dir;
        }
        None => library.books.push(LibraryEntry {
            id: book.id,
            title: book.title,
            author: book.author,
            cover_path: book.cover_path,
            book_dir: book.book_dir,
            imported_at: now(),
            last_opened_at: None,
            total_seconds_read: 0,
        }),
    }
    save(state_dir, &library)
}

/// Marca un libro como recién abierto. Si el id no está en la biblioteca
/// (no debería pasar — se llama siempre después de un `upsert`) no hace
/// nada en vez de fallar: no hay ningún dato que perder.
pub fn mark_opened(state_dir: &Path, book_id: &str) -> Result<(), String> {
    let mut library = load(state_dir)?;
    if let Some(entry) = library.books.iter_mut().find(|b| b.id == book_id) {
        entry.last_opened_at = Some(now());
        save(state_dir, &library)?;
    }
    Ok(())
}

/// Suma segundos de lectura al acumulado de un libro (Fase 4b). El
/// frontend llama esto cada tanto mientras el libro está abierto y la
/// ventana tiene foco — ver el efecto de montaje del lector en
/// `+page.svelte`. Igual que `mark_opened`, si el id no está no falla: no
/// hay ningún dato que perder.
pub fn add_reading_time(state_dir: &Path, book_id: &str, seconds: u64) -> Result<(), String> {
    let mut library = load(state_dir)?;
    if let Some(entry) = library.books.iter_mut().find(|b| b.id == book_id) {
        entry.total_seconds_read = entry.total_seconds_read.saturating_add(seconds);
        save(state_dir, &library)?;
    }
    Ok(())
}

/// Lista la biblioteca para el dashboard: los libros abiertos más
/// recientemente primero; los nunca abiertos van al final, ordenados por
/// fecha de importación descendente. `Option<i64>` ya ordena `None` antes
/// que cualquier `Some`, así que comparar en reversa (`b` contra `a`) deja
/// los `Some` (abiertos) adelante y, entre ellos, el más reciente primero.
pub fn list(state_dir: &Path) -> Result<Vec<LibraryEntry>, String> {
    let mut library = load(state_dir)?;
    library.books.sort_by(|a, b| {
        b.last_opened_at
            .cmp(&a.last_opened_at)
            .then_with(|| b.imported_at.cmp(&a.imported_at))
    });
    Ok(library.books)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("epub-widget-library-test-{label}-{nanos}"))
    }

    fn book(id: &str, title: &str) -> NewBook {
        NewBook {
            id: id.into(),
            title: title.into(),
            author: "Autora".into(),
            cover_path: None,
            book_dir: format!("dir-{id}"),
        }
    }

    #[test]
    fn biblioteca_vacia_no_tiene_libros() {
        let dir = unique_temp_dir("empty");
        assert!(list(&dir).unwrap().is_empty());
    }

    #[test]
    fn importar_un_libro_lo_deja_en_la_lista_sin_abrir() {
        let dir = unique_temp_dir("upsert");
        upsert(&dir, book("a", "Libro A")).unwrap();

        let entries = list(&dir).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Libro A");
        assert!(entries[0].last_opened_at.is_none());
        assert!(entries[0].imported_at > 0);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn reimportar_el_mismo_id_actualiza_metadata_y_conserva_fecha_de_importacion() {
        let dir = unique_temp_dir("reimport");
        upsert(&dir, book("a", "Título viejo")).unwrap();
        let primera_fecha = list(&dir).unwrap()[0].imported_at;

        upsert(&dir, book("a", "Título nuevo")).unwrap();
        let entries = list(&dir).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "Título nuevo");
        assert_eq!(entries[0].imported_at, primera_fecha);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn marcar_abierto_actualiza_last_opened_at() {
        let dir = unique_temp_dir("mark-opened");
        upsert(&dir, book("a", "Libro A")).unwrap();
        mark_opened(&dir, "a").unwrap();

        let entries = list(&dir).unwrap();
        assert!(entries[0].last_opened_at.is_some());

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn marcar_abierto_un_id_inexistente_no_falla() {
        let dir = unique_temp_dir("mark-opened-missing");
        assert!(mark_opened(&dir, "no-existe").is_ok());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn libro_abierto_aparece_antes_que_uno_nunca_abierto() {
        let dir = unique_temp_dir("order");
        upsert(&dir, book("a", "Libro A")).unwrap();
        upsert(&dir, book("b", "Libro B")).unwrap();
        mark_opened(&dir, "b").unwrap();

        let entries = list(&dir).unwrap();
        assert_eq!(entries[0].id, "b");
        assert_eq!(entries[1].id, "a");

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn libro_recien_importado_no_tiene_tiempo_de_lectura() {
        let dir = unique_temp_dir("time-empty");
        upsert(&dir, book("a", "Libro A")).unwrap();

        let entries = list(&dir).unwrap();
        assert_eq!(entries[0].total_seconds_read, 0);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sumar_tiempo_de_lectura_lo_acumula() {
        let dir = unique_temp_dir("time-add");
        upsert(&dir, book("a", "Libro A")).unwrap();
        add_reading_time(&dir, "a", 60).unwrap();
        add_reading_time(&dir, "a", 30).unwrap();

        let entries = list(&dir).unwrap();
        assert_eq!(entries[0].total_seconds_read, 90);

        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sumar_tiempo_de_lectura_a_un_id_inexistente_no_falla() {
        let dir = unique_temp_dir("time-missing");
        assert!(add_reading_time(&dir, "no-existe", 60).is_ok());
        fs::remove_dir_all(&dir).ok();
    }
}
