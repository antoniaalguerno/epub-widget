//! Núcleo del EPUB: descompresión segura + extracción de metadata básica.
//!
//! Un .epub es un .zip. Como puede venir de cualquier lado (el usuario lo
//! arrastra al widget), lo tratamos como contenido no confiable: nada de lo
//! que hay adentro decide dónde se escribe en disco ni cuánto espacio ocupa.

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// Tope de tamaño (descomprimido) por cada archivo dentro del epub.
const MAX_ENTRY_SIZE: u64 = 100 * 1024 * 1024; // 100 MB
/// Tope de tamaño total descomprimido de todo el epub.
const MAX_TOTAL_SIZE: u64 = 500 * 1024 * 1024; // 500 MB
/// Tope de cantidad de entradas en el zip.
const MAX_ENTRIES: usize = 5_000;

#[derive(Debug, Clone, Serialize)]
pub struct EpubMeta {
    pub id: String,
    pub title: String,
    pub author: String,
    pub cover_path: Option<String>,
    pub book_dir: String,
}

/// Importa un .epub: lo descomprime (si no estaba ya) bajo `library_dir` y
/// devuelve su metadata. `library_dir` debe ser una carpeta que la app
/// controla por completo (no una elegida por el usuario).
pub fn import(epub_path: &Path, library_dir: &Path) -> Result<EpubMeta, String> {
    if epub_path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase())
        != Some("epub".to_string())
    {
        return Err("el archivo no tiene extensión .epub".into());
    }
    if !epub_path.is_file() {
        return Err(format!("no existe el archivo: {}", epub_path.display()));
    }

    let bytes = fs::read(epub_path).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    let id = content_id(&bytes);
    let book_dir = library_dir.join(&id);

    if !book_dir.exists() {
        extract_safely(&bytes, &book_dir)?;
    }

    let (title, author, cover_rel) = read_metadata(&book_dir)?;
    let cover_path = cover_rel.map(|rel| book_dir.join(rel).to_string_lossy().into_owned());

    Ok(EpubMeta {
        id,
        title,
        author,
        cover_path,
        book_dir: book_dir.to_string_lossy().into_owned(),
    })
}

/// Id estable derivado del contenido (no del nombre de archivo, que el
/// usuario puede cambiar): así el mismo libro no se duplica en la librería.
fn content_id(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// Descomprime `bytes` (un .epub ya leído en memoria) dentro de `dest_dir`.
///
/// Defensas:
/// - **Zip slip**: usamos `enclosed_name()` de la crate `zip`, que descarta
///   entradas con rutas absolutas o `..`; además volvemos a chequear que la
///   ruta resuelta quede efectivamente dentro del directorio destino.
/// - **Zip bomb**: limitamos cantidad de entradas, tamaño declarado por
///   entrada, tamaño total acumulado, y además cortamos la lectura real con
///   `Read::take` para no confiar ciegamente en el tamaño que el propio zip
///   dice tener en su header.
fn extract_safely(bytes: &[u8], dest_dir: &Path) -> Result<(), String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format!("el archivo no es un .epub válido (zip corrupto): {e}"))?;

    if archive.len() > MAX_ENTRIES {
        return Err(format!(
            "el epub tiene demasiadas entradas ({}), se rechaza como sospechoso",
            archive.len()
        ));
    }

    // Extraemos a una carpeta temporal y recién al final la renombramos al
    // nombre final: si algo falla a mitad de camino, no queda una carpeta
    // "book_dir" a medio escribir que luego se confunda con una ya válida.
    let parent = dest_dir.parent().ok_or("ruta de destino inválida")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp_dir = dest_dir.with_extension("part");
    if tmp_dir.exists() {
        fs::remove_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;

    let result = extract_entries(&mut archive, &tmp_dir, MAX_ENTRY_SIZE, MAX_TOTAL_SIZE);
    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp_dir);
        return result;
    }

    fs::rename(&tmp_dir, dest_dir).map_err(|e| e.to_string())
}

/// Los límites se reciben por parámetro (en vez de leer las constantes del
/// módulo directamente) para poder ejercitar el rechazo por tamaño en los
/// tests sin tener que generar archivos de cientos de MB.
fn extract_entries(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    tmp_dir: &Path,
    max_entry_size: u64,
    max_total_size: u64,
) -> Result<(), String> {
    let mut total_size: u64 = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;

        // enclosed_name() ya es la defensa de la propia crate `zip` contra
        // zip slip (rutas absolutas o con "..' devuelven None). Si una
        // entrada no tiene una ruta segura, la descartamos en vez de
        // abortar todo el import: un epub real no debería traer eso, pero
        // no es motivo para tirar el libro entero.
        let Some(entry_name) = entry.enclosed_name() else {
            continue;
        };

        let out_path = tmp_dir.join(&entry_name);
        if !out_path.starts_with(tmp_dir) {
            return Err(format!("ruta insegura dentro del epub: {}", entry_name.display()));
        }

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
            continue;
        }

        if let Some(dir) = out_path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }

        let declared_size = entry.size();
        if declared_size > max_entry_size {
            return Err(format!(
                "archivo demasiado grande dentro del epub: {} ({declared_size} bytes)",
                entry_name.display()
            ));
        }
        total_size = total_size.saturating_add(declared_size);
        if total_size > max_total_size {
            return Err("el epub descomprimido excede el límite de tamaño total permitido".into());
        }

        let mut out_file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
        // No confiamos solo en `declared_size`: un zip armado a mano puede
        // mentir en el header. Cortamos la copia real un byte por encima
        // del límite para poder detectar el exceso.
        let mut limited = (&mut entry).take(max_entry_size + 1);
        let copied = std::io::copy(&mut limited, &mut out_file).map_err(|e| e.to_string())?;
        if copied > max_entry_size {
            return Err(format!(
                "archivo demasiado grande dentro del epub: {}",
                entry_name.display()
            ));
        }
    }

    Ok(())
}

/// Lee `META-INF/container.xml` y el OPF que señala, y saca título, autor
/// y la ruta (relativa a `book_dir`) de la imagen de portada si hay una.
fn read_metadata(book_dir: &Path) -> Result<(String, String, Option<PathBuf>), String> {
    let container_path = book_dir.join("META-INF").join("container.xml");
    let container_xml = fs::read_to_string(&container_path)
        .map_err(|_| "no se encontró META-INF/container.xml (¿es realmente un epub?)".to_string())?;
    let opf_rel = find_opf_path(&container_xml)?;
    let opf_path = book_dir.join(&opf_rel);
    let opf_xml = fs::read_to_string(&opf_path).map_err(|e| format!("no se pudo leer el OPF: {e}"))?;
    let opf_dir = opf_path.parent().unwrap_or(book_dir).to_path_buf();

    parse_opf(&opf_xml, &opf_dir, book_dir)
}

fn find_opf_path(xml: &str) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if local_name(e.local_name()) == "rootfile" => {
                if let Some(path) = get_attr(&e, "full-path") {
                    return Ok(path);
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("container.xml inválido: {e}")),
            _ => {}
        }
        buf.clear();
    }
    Err("container.xml no declara un rootfile".into())
}

fn parse_opf(
    xml: &str,
    opf_dir: &Path,
    book_dir: &Path,
) -> Result<(String, String, Option<PathBuf>), String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut title = String::new();
    let mut author = String::new();
    let mut cover_meta_id: Option<String> = None;
    // (id, href, marcado explícitamente como "cover-image" en EPUB3)
    let mut manifest: Vec<(String, String, bool)> = Vec::new();
    let mut capturing: Option<&'static str> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = local_name(e.local_name());
                // Un OPF puede traer varios <dc:title> (título + subtítulo)
                // o varios <dc:creator> (autor + traductor, etc.). Nos
                // quedamos con el primero de cada uno en vez de concatenar
                // todo sin separador.
                if name == "title" && title.is_empty() {
                    capturing = Some("title");
                } else if name == "creator" && author.is_empty() {
                    capturing = Some("creator");
                } else {
                    handle_opf_tag(&name, &e, &mut cover_meta_id, &mut manifest);
                }
            }
            Ok(Event::Empty(e)) => {
                handle_opf_tag(&local_name(e.local_name()), &e, &mut cover_meta_id, &mut manifest);
            }
            Ok(Event::Text(t)) => {
                if let Some(field) = capturing {
                    let text = t.decode().map_err(|e| e.to_string())?;
                    match field {
                        "title" => title.push_str(&text),
                        "creator" => author.push_str(&text),
                        _ => {}
                    }
                }
            }
            // Referencias de entidad (`&amp;`, `&#39;`, etc.) le llegan al
            // reader como eventos separados, no como parte del Text: hay que
            // resolverlas a mano si estamos capturando texto en ese momento.
            Ok(Event::GeneralRef(r)) => {
                if let Some(field) = capturing {
                    let resolved = if r.is_char_ref() {
                        r.resolve_char_ref().map_err(|e| e.to_string())?.map(|c| c.to_string())
                    } else {
                        let name = r.decode().map_err(|e| e.to_string())?;
                        quick_xml::escape::resolve_predefined_entity(&name).map(str::to_string)
                    };
                    if let Some(text) = resolved {
                        match field {
                            "title" => title.push_str(&text),
                            "creator" => author.push_str(&text),
                            _ => {}
                        }
                    }
                }
            }
            Ok(Event::End(e)) => {
                if matches!(local_name(e.local_name()).as_str(), "title" | "creator") {
                    capturing = None;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("OPF inválido: {e}")),
            _ => {}
        }
        buf.clear();
    }

    let title = non_empty(title, "(sin título)");
    let author = non_empty(author, "(autor desconocido)");

    let cover_href = cover_meta_id
        .and_then(|id| manifest.iter().find(|(iid, _, _)| *iid == id).map(|(_, h, _)| h.clone()))
        .or_else(|| manifest.iter().find(|(_, _, is_cover)| *is_cover).map(|(_, h, _)| h.clone()));

    let cover_rel = cover_href.map(|href| {
        let abs = opf_dir.join(&href);
        abs.strip_prefix(book_dir).map(Path::to_path_buf).unwrap_or(abs)
    });

    Ok((title, author, cover_rel))
}

fn handle_opf_tag(
    name: &str,
    e: &BytesStart,
    cover_meta_id: &mut Option<String>,
    manifest: &mut Vec<(String, String, bool)>,
) {
    if name == "meta" {
        if let (Some(n), Some(c)) = (get_attr(e, "name"), get_attr(e, "content")) {
            if n == "cover" {
                *cover_meta_id = Some(c);
            }
        }
    } else if name == "item" {
        if let (Some(id), Some(href)) = (get_attr(e, "id"), get_attr(e, "href")) {
            let is_cover = get_attr(e, "properties")
                .map(|p| p.split_whitespace().any(|t| t == "cover-image"))
                .unwrap_or(false);
            manifest.push((id, href, is_cover));
        }
    }
}

fn non_empty(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn local_name<'a>(name: quick_xml::name::LocalName<'a>) -> String {
    String::from_utf8_lossy(name.as_ref()).into_owned()
}

fn get_attr(e: &BytesStart, key: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == key.as_bytes() {
            a.unescape_value().ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const SAMPLE_CONTAINER: &[u8] = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    const SAMPLE_OPF: &[u8] = br#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Cuento de prueba &amp; algo mas</dc:title>
    <dc:creator>Autora de prueba</dc:creator>
    <meta name="cover" content="cover-img"/>
  </metadata>
  <manifest>
    <item id="cover-img" href="cover.jpg" media-type="image/jpeg"/>
    <item id="chap1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap1"/>
  </spine>
</package>"#;

    /// Arma un .epub en memoria a partir de pares (nombre de entrada, contenido).
    fn build_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let options = SimpleFileOptions::default();
            for (name, content) in entries {
                writer.start_file(*name, options).unwrap();
                writer.write_all(content).unwrap();
            }
            writer.finish().unwrap();
        }
        buf
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("epub-widget-test-{label}-{nanos}"))
    }

    #[test]
    fn usa_solo_el_primer_titulo_cuando_hay_titulo_y_subtitulo() {
        // Caso real: epubs con <dc:title> principal + subtítulo por separado
        // (título/subtítulo como dos elementos distintos, sin refines).
        const OPF_CON_SUBTITULO: &[u8] = br#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>The Common Lisp Cookbook</dc:title>
    <dc:title>Diving into the programmable programming language</dc:title>
    <dc:creator>The Common Lisp Cookbook contributors</dc:creator>
  </metadata>
  <manifest>
    <item id="chap1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="chap1"/></spine>
</package>"#;

        let epub_bytes = build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", SAMPLE_CONTAINER),
            ("OEBPS/content.opf", OPF_CON_SUBTITULO),
            ("OEBPS/chapter1.xhtml", b"<html><body>hola</body></html>"),
        ]);

        let tmp = unique_temp_dir("subtitulo");
        let library_dir = tmp.join("library");
        fs::create_dir_all(&library_dir).unwrap();
        let epub_path = tmp.join("libro.epub");
        fs::write(&epub_path, &epub_bytes).unwrap();

        let meta = import(&epub_path, &library_dir).expect("debería importar sin error");

        assert_eq!(meta.title, "The Common Lisp Cookbook");
        assert_eq!(meta.author, "The Common Lisp Cookbook contributors");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn importa_titulo_autor_y_portada_de_un_epub_valido() {
        let epub_bytes = build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", SAMPLE_CONTAINER),
            ("OEBPS/content.opf", SAMPLE_OPF),
            ("OEBPS/cover.jpg", b"contenido-falso-de-portada"),
            ("OEBPS/chapter1.xhtml", b"<html><body>hola</body></html>"),
        ]);

        let tmp = unique_temp_dir("ok");
        let library_dir = tmp.join("library");
        fs::create_dir_all(&library_dir).unwrap();
        let epub_path = tmp.join("libro.epub");
        fs::write(&epub_path, &epub_bytes).unwrap();

        let meta = import(&epub_path, &library_dir).expect("debería importar sin error");

        assert_eq!(meta.title, "Cuento de prueba & algo mas");
        assert_eq!(meta.author, "Autora de prueba");
        let expected_cover = library_dir.join(&meta.id).join("OEBPS").join("cover.jpg");
        assert_eq!(meta.cover_path.as_deref(), Some(expected_cover.to_string_lossy().as_ref()));
        assert!(Path::new(&meta.cover_path.unwrap()).is_file());

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn rechaza_zip_slip_sin_escaparse_de_la_carpeta_destino() {
        // Ademas de los archivos legitimos del epub, una entrada maliciosa
        // que intenta escribir fuera de la carpeta de destino.
        let epub_bytes = build_zip(&[
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", SAMPLE_CONTAINER),
            ("OEBPS/content.opf", SAMPLE_OPF),
            ("OEBPS/cover.jpg", b"portada"),
            ("OEBPS/chapter1.xhtml", b"<html><body>hola</body></html>"),
            ("../../evil.txt", b"deberia quedar afuera"),
        ]);

        let tmp = unique_temp_dir("slip");
        let library_dir = tmp.join("library");
        fs::create_dir_all(&library_dir).unwrap();
        let epub_path = tmp.join("libro.epub");
        fs::write(&epub_path, &epub_bytes).unwrap();

        let meta = import(&epub_path, &library_dir).expect("el resto del epub sigue siendo valido");

        // El archivo malicioso no debe existir en ningun lado por fuera del
        // book_dir: ni al lado de la libreria, ni en el temp raiz.
        assert!(!tmp.join("evil.txt").exists());
        assert!(!library_dir.join("evil.txt").exists());
        assert!(!tmp.parent().unwrap().join("evil.txt").exists());
        // El libro en si se importo bien a pesar de la entrada maliciosa.
        assert_eq!(meta.title, "Cuento de prueba & algo mas");

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn rechaza_entradas_que_exceden_el_limite_de_tamano() {
        let epub_bytes = build_zip(&[("big.bin", &[0u8; 1024])]);
        let mut archive = zip::ZipArchive::new(Cursor::new(epub_bytes.as_slice())).unwrap();

        let tmp = unique_temp_dir("bomb");
        fs::create_dir_all(&tmp).unwrap();

        // Limite de 10 bytes: la entrada de 1024 bytes tiene que ser rechazada.
        let result = extract_entries(&mut archive, &tmp, 10, 10);
        assert!(result.is_err());

        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn rechaza_epub_con_extension_incorrecta() {
        let err = import(Path::new("C:/no/existe/libro.txt"), Path::new("C:/no/existe")).unwrap_err();
        assert!(err.contains(".epub"));
    }
}
