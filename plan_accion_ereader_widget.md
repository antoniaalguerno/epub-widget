# Plan de Acción — Ereader Widget de Escritorio (Windows)

**Stack:** Tauri 2.0 + Svelte 5 + epub.js
**Filosofía:** cada fase cierra con algo funcional y probado antes de pasar a la siguiente. No se avanza con deuda técnica sin resolver.

---

## Fase 0 — Fundamentos y entorno

**Objetivo:** tener el entorno listo y entender lo mínimo necesario de las herramientas nuevas antes de escribir lógica de producto.

**Tareas:**

- Instalar Rust, Node, Tauri CLI y prerequisitos de Windows (WebView2 ya viene en Win10/11).
- Crear proyecto Tauri + Svelte 5 desde el scaffold oficial (`create-tauri-app`).
- Hacer correr el "hello world" como ventana normal (sin trucos de widget todavía).
- Aprender lo básico de Svelte 5 (runes: `$state`, `$derived`, `$effect`) con 2-3 componentes de prueba.
- Familiarizarte con la estructura de `src-tauri/` (comandos Rust, `tauri.conf.json`, `capabilities/`).

**Cierre de fase:** app Tauri+Svelte corriendo localmente, mostrando un componente Svelte reactivo simple. Sin código de producto todavía.

---

## Fase 1 — El "widget shell" (mecánica de ventana)

**Objetivo:** resolver todo lo que hace que esto se sienta un widget y no una app normal, sin meter todavía el epub.

**Tareas:**

- [ ] Ventana sin bordes (`decorations: false`)
- [ ] Always-on-top.
- [ ] Fondo transparente.
- [ ] Arrastrable desde cualquier punto del contenido (no solo una barra de título).
- [ ] Persistencia de posición y tamaño entre sesiones (guardar en un JSON local).
- [ ] Comportamiento correcto en multi-monitor (que no aparezca fuera de pantalla si cambiaste de setup).
- [ ] Un ícono en la bandeja del sistema (tray) para mostrar/ocultar el widget y salir de la app.

**Cierre de fase:** un rectángulo flotante vacío (puede tener un color de fondo de prueba) que se puede mover a cualquier parte de la pantalla, queda siempre encima, y recuerda dónde lo dejaste al reabrir la app.

---

## Fase 2 — Núcleo del EPUB (parseo y seguridad básica)

**Objetivo:** poder cargar un .epub de forma segura y extraer lo esencial, sin preocuparte todavía de cómo se ve.

**Tareas:**

- Función en Rust (o plugin) para recibir la ruta de un .epub y descomprimirlo.
- Validación de rutas al descomprimir (evitar zip slip) y límite de tamaño (evitar zip bombs).
- Extraer metadata básica: título, autor, portada.
- Definir la carpeta local donde se guarda la librería (`$APPDATA` de la app, no cualquier lado).
- Configurar la primera **capability** de Tauri: la ventana del reader solo tiene permiso de leer esa carpeta específica, nada más.

**Cierre de fase:** dado un .epub, la app extrae y muestra (aunque sea en consola/log) título, autor y ruta de la portada, de forma segura y con permisos acotados.

---

## Fase 3 — Renderizado y lectura

**Objetivo:** que el contenido del epub se pueda leer de verdad dentro del widget.

**Tareas:**

- Integrar epub.js en el frontend Svelte.
- Cargar el epub descomprimido en el visor.
- Paginación (avanzar/retroceder), no scroll infinito.
- Ajuste de tamaño de fuente y tema (claro/oscuro/sepia) básico.
- Verificar que el contenido embebido del epub (JS/HTML si lo trae) no tenga permisos de más — este es el punto donde revisás que el contenido no confiable esté bien aislado.

**Cierre de fase:** podés abrir un epub real, pasar páginas, cambiar tema/fuente, y cerrar la app sin que rompa nada raro. Esta es la primera versión "usable" del reader.

---

## Fase 4 — Estado y librería

**Objetivo:** que la experiencia persista entre sesiones y se pueda manejar más de un libro.

**Tareas:**

- Guardar progreso de lectura por libro (última página/posición).
- Vista simple de librería (lista de libros agregados, con portada).
- Agregar/quitar libros (drag & drop de un .epub al widget, o selector de archivo).
- Guardar preferencias (tema, tamaño de fuente) de forma persistente.

**Cierre de fase:** cerrás la app en medio de un libro, la reabrís, y te deja exactamente donde ibas. Podés tener varios libros y cambiar entre ellos.

---

## Fase 5 — Hardening de seguridad

**Objetivo:** revisar con cabeza fría todo lo que quedó "suelto" en fases anteriores, antes de pensar en publicar.

**Tareas:**

- Auditar las capabilities: confirmar que cada ventana tiene el mínimo de permisos posible (nada de `fs:default` genérico si no lo necesitás).
- Revisar que la extracción de zip no tenga huecos.
- Si vas a tener auto-actualización, definir el mecanismo (firmado, verificación de firma antes de instalar).
- Revisar dependencias de npm/cargo por vulnerabilidades conocidas (`npm audit`, `cargo audit`).

**Cierre de fase:** checklist de seguridad revisado y documentado (aunque sea informal, un archivo `SECURITY_NOTES.md` con qué se revisó y qué falta).

---

## Fase 6 — Empaquetado y publicación

**Objetivo:** que otra persona lo pueda instalar sin fricción.

**Tareas:**

- Generar el instalador con `tauri build` (MSI/NSIS).
- Ícono de la app, nombre, metadata.
- Firma de código (para evitar warnings de SmartScreen) — evaluar costo/beneficio en esta etapa.
- Subir a GitHub Releases con changelog básico.
- Evaluar si vale la pena Microsoft Store más adelante (requiere cuenta de developer).

**Cierre de fase:** un instalador descargable que alguien externo a vos pudo instalar y usar sin tu ayuda directa.

---

## Fase 7 — Generalizar la arquitectura (widget host)

**Objetivo:** ahora que el ereader funciona de punta a punta, extraer el patrón para que agregar un widget nuevo sea barato.

**Tareas:**

- Separar conceptualmente "core/shell" (gestión de ventanas, tray, persistencia general) del "widget epub" (que pasa a ser el primer módulo).
- Definir una interfaz mínima de "qué necesita un widget" (su propia ventana, su propia capability, su propio estado).
- Documentar el patrón para vos misma (aunque sea informal) para no tener que redescubrirlo con el próximo widget.

**Cierre de fase:** el código del ereader vive claramente separado de la lógica genérica de ventanas/tray/persistencia. Se podría, en teoría, agregar un segundo widget tocando poco código del core.

---

## Fase 8 — Segundo widget (prueba de que escala)

**Objetivo:** validar que la arquitectura de la Fase 7 realmente sirve, con algo simple.

**Tareas:**

- Elegir un widget chico (algo simple: notas rápidas, clima, lo que prefieras) como prueba de concepto.
- Implementarlo usando el patrón definido en la Fase 7.
- Medir cuánto código nuevo hizo falta vs. cuánto se reusó del core.

**Cierre de fase:** dos widgets corriendo desde la misma app, cada uno con su propia ventana/capability, compartiendo la infraestructura común.

---

## Notas generales

- No pasar a la fase siguiente con la anterior "casi lista" — cada cierre de fase es un punto donde probás la app como si fueras usuaria nueva.
- Es buena idea hacer commits de git por fase (o por tarea dentro de la fase), así queda un historial claro de qué se cerró cuándo.
- Las Fases 0-6 son el camino directo al ereader publicable. Las Fases 7-8 son las que te dan la plataforma multi-widget que tenías en mente desde el principio — no hace falta apurarlas antes de tener el ereader sólido.
