<script lang="ts">
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";

  // ---------------------------------------------------------------------
  // Puente de errores del frontend -> consola de Rust. Los errores de JS
  // (excepciones sueltas, promesas rechazadas, fallos de epub.js) no
  // aparecen en ninguna terminal que se pueda inspeccionar desde acá, así
  // que se reenvían a un comando de Tauri que los imprime del lado Rust.
  // ---------------------------------------------------------------------
  function logToRust(message: string) {
    console.error(message);
    invoke("log_frontend_error", { message }).catch(() => {});
  }
  function infoToRust(message: string) {
    console.log(message);
    invoke("log_frontend_info", { message }).catch(() => {});
  }

  onMount(() => {
    const onError = (event: ErrorEvent) => {
      logToRust(
        `window.onerror: ${event.message} @ ${event.filename}:${event.lineno}`,
      );
    };
    const onRejection = (event: PromiseRejectionEvent) => {
      logToRust(`unhandledrejection: ${String(event.reason)}`);
    };
    window.addEventListener("error", onError);
    window.addEventListener("unhandledrejection", onRejection);
    return () => {
      window.removeEventListener("error", onError);
      window.removeEventListener("unhandledrejection", onRejection);
    };
  });

  // ---------------------------------------------------------------------
  // Reloj + import de prueba (Fase 1/2)
  // ---------------------------------------------------------------------
  let hora = $state(new Date().toLocaleTimeString());
  $effect(() => {
    const id = setInterval(() => {
      hora = new Date().toLocaleTimeString();
    }, 1000);
    return () => clearInterval(id);
  });

  type EpubMeta = {
    id: string;
    title: string;
    author: string;
    cover_path: string | null;
    book_dir: string;
  };

  type LibraryEntry = EpubMeta & {
    imported_at: number;
    last_opened_at: number | null;
    total_seconds_read: number;
  };

  let importing = $state(false);
  let importError = $state("");
  let currentBook = $state<EpubMeta | null>(null);
  let library = $state<LibraryEntry[]>([]);

  async function refreshLibrary() {
    try {
      library = await invoke<LibraryEntry[]>("list_library");
    } catch (e) {
      logToRust(`list_library: ${String(e)}`);
    }
  }

  onMount(() => {
    refreshLibrary();
  });

  // ---------------------------------------------------------------------
  // Fase 4b: estadísticas del dashboard — se derivan de la biblioteca ya
  // cargada, sin pedir nada nuevo al backend.
  // ---------------------------------------------------------------------
  let totalSecondsRead = $derived(
    library.reduce((sum, entry) => sum + entry.total_seconds_read, 0),
  );
  let booksThisMonth = $derived.by(() => {
    const now = new Date();
    return library.filter((entry) => {
      const imported = new Date(entry.imported_at * 1000);
      return (
        imported.getFullYear() === now.getFullYear() &&
        imported.getMonth() === now.getMonth()
      );
    }).length;
  });

  function formatReadingTime(totalSeconds: number): string {
    const totalMinutes = Math.floor(totalSeconds / 60);
    if (totalMinutes < 60) return `${totalMinutes} min`;
    const hours = Math.floor(totalMinutes / 60);
    const minutes = totalMinutes % 60;
    return minutes === 0 ? `${hours} h` : `${hours} h ${minutes} min`;
  }

  // Marca el libro como recién abierto (para "recientes") y lo muestra en
  // el lector. No se espera a open_book: no hay nada útil que mostrar
  // mientras tanto, y si falla el registro de "abierto" no tiene sentido
  // bloquear la lectura por eso.
  function openBook(meta: EpubMeta) {
    currentBook = meta;
    invoke("open_book", { bookId: meta.id }).catch((e) =>
      logToRust(`open_book: ${String(e)}`),
    );
  }

  function openFromLibrary(entry: LibraryEntry) {
    openBook({
      id: entry.id,
      title: entry.title,
      author: entry.author,
      cover_path: entry.cover_path,
      book_dir: entry.book_dir,
    });
  }

  // Abre el explorador de archivos de Windows filtrado a .epub — reemplaza
  // el viejo input de texto con la ruta a mano.
  async function pickAndImport() {
    importError = "";
    let selected: string | null;
    try {
      selected = await open({
        multiple: false,
        filters: [{ name: "EPUB", extensions: ["epub"] }],
      });
    } catch (e) {
      importError = String(e);
      logToRust(`dialog open: ${String(e)}`);
      return;
    }
    if (typeof selected !== "string") return;

    importing = true;
    try {
      const meta = await invoke<EpubMeta>("import_epub", { path: selected });
      console.log("[import_epub] ok:", meta);
      openBook(meta);
    } catch (e) {
      importError = String(e);
      logToRust(`import_epub: ${String(e)}`);
    } finally {
      importing = false;
    }
  }

  function closeBook() {
    currentBook = null;
    refreshLibrary();
  }

  // ---------------------------------------------------------------------
  // Fase 3: renderizado con epub.js
  //
  // El epub ya está descomprimido en disco por import_epub (Fase 2).
  // epub.js soporta abrir un epub "directorio" (sin volver a zippear/
  // unzippear nada): le pasamos la carpeta como URL base y sigue los
  // fetch/XHR relativos (META-INF/container.xml -> OPF -> capítulos).
  // Esas requests las resuelve el protocolo de assets de Tauri, que solo
  // tiene permiso de leer $APPDATA/library/** (configurado en tauri.conf.json).
  //
  // Seguridad: epub.js renderiza cada sección dentro de un <iframe> con
  // sandbox="allow-same-origin" y, mientras no le pasemos
  // allowScriptedContent/allowPopups, NO agrega allow-scripts ni
  // allow-popups — el <script> que traiga el epub queda inerte. No seteamos
  // esas opciones a propósito.
  // ---------------------------------------------------------------------
  type Theme = "light" | "dark" | "sepia" | "ghost";

  const THEME_RULES: Record<Theme, Record<string, Record<string, string>>> = {
    light: {
      "html, body": {
        background: "#faf7f2 !important",
        color: "#1a1a1a !important",
      },
    },
    dark: {
      "html, body": {
        background: "#1c1b22 !important",
        color: "#e8e6e3 !important",
      },
    },
    sepia: {
      "html, body": {
        background: "#f4ecd8 !important",
        color: "#5b4636 !important",
      },
    },
    ghost: {
      "html, body": {
        background: "#00000020 !important",
        color: "#e8e6e380 !important",
      },
    },
  };

  type TocItem = { label: string; href: string; subitems?: TocItem[] };
  type Bookmark = { cfi: string; label: string };
  type SearchResult = { cfi: string; excerpt: string; href: string };
  type NavTab = "toc" | "bookmarks" | "search" | null;
  type ReadingMode = "paginated" | "scrolled";

  let readerEl: HTMLDivElement | undefined = $state();
  let readerReady = $state(false);
  let readerError = $state("");
  let theme = $state<Theme>("light");
  let fontSize = $state(100);
  let readingMode = $state<ReadingMode>("paginated");
  let toc = $state<TocItem[]>([]);
  let showSettings = $state(false);
  let navTab = $state<NavTab>(null);

  // Posición actual del lector (se actualiza con el evento "relocated" de
  // epub.js), para saber qué marcar/desmarcar con el botón de marcador.
  let currentCfi = $state("");
  let currentHref = $state("");
  let bookmarks = $state<Bookmark[]>([]);
  let isBookmarked = $derived(bookmarks.some((b) => b.cfi === currentCfi));

  let searchQuery = $state("");
  let searchResults = $state<SearchResult[]>([]);
  let searching = $state(false);
  let searchError = $state("");

  // No son $state: epub.js maneja su propio estado interno y no queremos
  // que Svelte intente hacerlos reactivos (son objetos grandes con ciclos).
  let book: any = null;
  let rendition: any = null;

  $effect(() => {
    const target = currentBook;
    const el = readerEl;
    if (!target || !el) return;

    let cancelled = false;
    readerReady = false;
    readerError = "";

    // -----------------------------------------------------------------
    // Fase 4b: tiempo de lectura. Se cuenta solo mientras el libro está
    // abierto Y la ventana tiene foco — el widget se oculta con
    // window.hide() (tray) en vez de destruirse, así que este efecto
    // sigue "montado" en segundo plano; sin este chequeo de foco, el
    // tiempo se acumularía también con el widget escondido.
    // Se manda al backend cada minuto y al perder el foco/cerrar el
    // libro, en vez de en cada segundo, para no saturar de escrituras a
    // disco por algo que no necesita esa precisión.
    // -----------------------------------------------------------------
    let sessionStartMs: number | null = document.hasFocus() ? Date.now() : null;

    const flushReadingTime = () => {
      if (sessionStartMs === null) return;
      const elapsedSeconds = Math.round((Date.now() - sessionStartMs) / 1000);
      sessionStartMs = Date.now();
      if (elapsedSeconds < 1) return;
      invoke("add_reading_time", {
        bookId: target.id,
        seconds: elapsedSeconds,
      }).catch((e: unknown) => logToRust(`add_reading_time: ${String(e)}`));
    };
    const pauseReadingTime = () => {
      flushReadingTime();
      sessionStartMs = null;
    };
    const resumeReadingTime = () => {
      if (sessionStartMs === null) sessionStartMs = Date.now();
    };
    window.addEventListener("blur", pauseReadingTime);
    window.addEventListener("focus", resumeReadingTime);
    const readingTimeIntervalId = window.setInterval(flushReadingTime, 60_000);

    (async () => {
      const epubjs = await import("epubjs");
      const ePub = epubjs.default;
      if (cancelled || !readerEl) return;

      const baseUrl = convertFileSrc(target.book_dir) + "/";
      book = ePub(baseUrl);
      rendition = book.renderTo(el, {
        width: "100%",
        height: "100%",
        flow: readingMode,
        allowScriptedContent: false,
        allowPopups: false,
      });
      rendition.themes.register(THEME_RULES);
      rendition.themes.select(theme);
      rendition.themes.fontSize(`${fontSize}%`);

      rendition.on(
        "relocated",
        (location: { start: { cfi: string; href: string } }) => {
          currentCfi = location.start.cfi;
          currentHref = location.start.href;
          invoke("save_progress", {
            bookId: target.id,
            cfi: location.start.cfi,
          }).catch((e: unknown) => logToRust(`save_progress: ${String(e)}`));
        },
      );

      // Reaplicar tema/tamaño de letra en CADA render de sección, no solo al
      // cambiar el valor: epub.js a veces recicla/recrea la vista interna
      // (por ejemplo al pasar de página) y el estilo inyectado en la vista
      // anterior no viaja solo a la nueva. "rendered" dispara una vez por
      // cada sección que efectivamente se pinta, así que es el punto más
      // confiable para reforzar el estilo.
      rendition.on("rendered", () => applyReaderStyles("rendered event"));

      // Zonas de toque para "hojear" tocando la página, además de los
      // botones Anterior/Siguiente. Solo activo en modo paginado — en
      // scroll no tendría sentido (el gesto natural ahí es el scroll).
      rendition.hooks.content.register((contents: any) => {
        const doc: Document | undefined = contents?.document;
        if (!doc) return;
        doc.addEventListener("click", (event: MouseEvent) => {
          if (readingMode !== "paginated") return;
          const selection = doc.getSelection?.();
          if (selection && selection.toString().length > 0) return;
          if ((event.target as HTMLElement | null)?.closest?.("a")) return;
          const width = doc.defaultView?.innerWidth;
          if (!width) return;
          if (event.clientX < width * 0.3) prevPage();
          else if (event.clientX > width * 0.7) nextPage();
        });
      });

      // Retomar en la posición guardada si existe — se pide antes del
      // display() inicial para no pasar primero por la portada/página 1 y
      // "saltar" recién después: rendition.display() acepta el CFI
      // directamente como punto de entrada.
      let resumeCfi: string | null = null;
      try {
        resumeCfi = await invoke<string | null>("get_progress", {
          bookId: target.id,
        });
      } catch (e) {
        logToRust(`get_progress: ${String(e)}`);
      }
      if (cancelled) return;

      await rendition.display(resumeCfi ?? undefined);
      if (cancelled) return;
      readerReady = true;
      applyReaderStyles("display() inicial");
      console.log("[epubjs] libro renderizado:", target.title);

      // El índice sale del nav/ncx del epub; si por lo que sea no lo trae
      // o falla el parseo, no tiene que tirar abajo la lectura — solo
      // queda sin botón de índice.
      book.loaded.navigation
        .then((nav: { toc: TocItem[] }) => {
          if (!cancelled) toc = nav.toc ?? [];
        })
        .catch((e: unknown) => logToRust(`epubjs toc: ${String(e)}`));

      invoke<Bookmark[]>("get_bookmarks", { bookId: target.id })
        .then((list) => {
          if (!cancelled) bookmarks = list;
        })
        .catch((e: unknown) => logToRust(`get_bookmarks: ${String(e)}`));
    })().catch((e: unknown) => {
      if (cancelled) return;
      readerError = String(e);
      logToRust(`epubjs: ${String(e)}`);
    });

    return () => {
      cancelled = true;
      readerReady = false;
      showSettings = false;
      navTab = null;
      toc = [];
      bookmarks = [];
      currentCfi = "";
      currentHref = "";
      searchQuery = "";
      searchResults = [];
      if (rendition) {
        rendition.destroy();
      }
      rendition = null;
      book = null;

      flushReadingTime();
      window.removeEventListener("blur", pauseReadingTime);
      window.removeEventListener("focus", resumeReadingTime);
      clearInterval(readingTimeIntervalId);
    };
  });

  // Aplica tema + tamaño de letra a lo que esté renderizado ahora mismo.
  // Se llama tanto reactivamente (cuando cambian theme/fontSize) como desde
  // el evento "rendered" de epub.js (cada vez que se pinta una sección
  // nueva), porque confiar en un solo disparador no alcanzaba — ver nota
  // más arriba.
  function applyReaderStyles(reason: string) {
    if (!rendition) return;
    try {
      rendition.themes.select(theme);
      rendition.themes.fontSize(`${fontSize}%`);
      const count = rendition.getContents?.()?.length ?? "?";
      infoToRust(
        `applyReaderStyles (${reason}) theme=${theme} fontSize=${fontSize}% contents=${count}`,
      );
    } catch (e) {
      logToRust(`applyReaderStyles (${reason}): ${String(e)}`);
    }
  }

  $effect(() => {
    // Se leen theme/fontSize acá (no solo dentro de applyReaderStyles) para
    // que este efecto quede explícitamente suscripto a los dos.
    void theme;
    void fontSize;
    if (rendition && readerReady) {
      applyReaderStyles("cambio de theme/fontSize");
    }
  });

  function setReadingMode(mode: ReadingMode) {
    readingMode = mode;
    // Imperativo a propósito: llamar rendition.flow() dispara un re-layout
    // interno de epub.js, y no queremos que eso pase solo (redundante) cada
    // vez que el componente se re-evalúa — solo cuando la usuaria realmente
    // lo cambia.
    rendition?.flow(mode);
  }

  function nextPage() {
    rendition?.next();
  }
  function prevPage() {
    rendition?.prev();
  }
  function biggerFont() {
    fontSize = Math.min(fontSize + 10, 200);
  }
  function smallerFont() {
    fontSize = Math.max(fontSize - 10, 60);
  }

  function toggleSettings() {
    showSettings = !showSettings;
    navTab = null;
  }
  function openNavTab(tab: NavTab) {
    navTab = navTab === tab ? null : tab;
    showSettings = false;
  }
  function goToChapter(item: TocItem) {
    rendition?.display(item.href);
    navTab = null;
  }
  function goToBookmark(bookmark: Bookmark) {
    rendition?.display(bookmark.cfi);
    navTab = null;
  }

  // Título del capítulo que contiene un href, buscado en el índice — sirve
  // de etiqueta legible para un marcador ("Capítulo 3" en vez del CFI crudo).
  function flattenToc(items: TocItem[]): TocItem[] {
    return items.flatMap((item) => [item, ...flattenToc(item.subitems ?? [])]);
  }
  function chapterTitleFor(href: string): string {
    const withoutFragment = (h: string) => h.split("#")[0];
    const items = flattenToc(toc);
    const target = withoutFragment(href);

    const exact = items.find((item) => withoutFragment(item.href) === target);
    if (exact) return exact.label.trim();

    // El href del índice a veces está resuelto contra una base distinta a
    // la del spine (nav.xhtml vs el OPF) y no matchea exacto — de última,
    // comparamos solo el nombre de archivo.
    const targetFile = target.split("/").pop();
    const byFile = items.find(
      (item) => withoutFragment(item.href).split("/").pop() === targetFile,
    );
    return byFile?.label.trim() || "Marcador";
  }

  async function toggleBookmark() {
    if (!currentBook || !currentCfi) return;
    try {
      if (isBookmarked) {
        bookmarks = await invoke<Bookmark[]>("remove_bookmark", {
          bookId: currentBook.id,
          cfi: currentCfi,
        });
      } else {
        bookmarks = await invoke<Bookmark[]>("add_bookmark", {
          bookId: currentBook.id,
          cfi: currentCfi,
          label: chapterTitleFor(currentHref),
        });
      }
    } catch (e) {
      logToRust(`bookmark: ${String(e)}`);
    }
  }

  // Recorre cada sección del epub, la carga temporalmente, busca el texto
  // y la descarga — es el patrón recomendado por la propia epub.js para no
  // tener todo el libro en memoria a la vez.
  async function runSearch(event: Event) {
    event.preventDefault();
    const query = searchQuery.trim();
    if (!book || !query) {
      searchResults = [];
      return;
    }
    searching = true;
    searchError = "";
    try {
      const perSection = await Promise.all(
        book.spine.spineItems.map(async (section: any) => {
          await section.load(book.load.bind(book));
          const matches = section.find(query) as {
            cfi: string;
            excerpt: string;
          }[];
          section.unload();
          return matches.map((m) => ({ ...m, href: section.href as string }));
        }),
      );
      searchResults = perSection.flat();
    } catch (e) {
      searchError = String(e);
      logToRust(`search: ${String(e)}`);
    } finally {
      searching = false;
    }
  }
  function goToSearchResult(result: SearchResult) {
    rendition?.display(result.cfi);
    navTab = null;
  }

  function onKeydown(e: KeyboardEvent) {
    if (!currentBook) return;
    if (navTab || showSettings) return;
    if (e.key === "ArrowRight") nextPage();
    if (e.key === "ArrowLeft") prevPage();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<!--
  data-tauri-drag-region en el contenedor raíz hace que el widget se
  arrastre desde cualquier punto libre. El área de lectura y los controles
  quedan explícitamente fuera de esa zona para poder clickear normalmente.
-->
<main class="widget" data-tauri-drag-region>
  {#if !currentBook}
    <div class="dashboard">
      <div class="dashboard-header" data-tauri-drag-region>
        <span class="dashboard-title" data-tauri-drag-region>Mi biblioteca</span
        >
        <span class="clock" data-tauri-drag-region>{hora}</span>
      </div>

      <div class="stats-row">
        <div class="stat-tile">
          <span class="stat-value">{formatReadingTime(totalSecondsRead)}</span>
          <span class="stat-label">Leídas</span>
        </div>
        <div class="stat-tile">
          <span class="stat-value">{booksThisMonth}</span>
          <span class="stat-label">Este mes</span>
        </div>
        <div class="stat-tile">
          <span class="stat-value">{library.length}</span>
          <span class="stat-label">Biblioteca</span>
        </div>
      </div>

      <button class="load-btn" onclick={pickAndImport} disabled={importing}>
        {importing ? "Importando…" : "📂 Cargar libro"}
      </button>

      {#if importError}
        <p class="error">{importError}</p>
      {/if}

      <div class="dashboard-list">
        {#if library.length === 0}
          <p class="hint" data-tauri-drag-region>
            Todavía no cargaste ningún libro. Usá "Cargar libro" para elegir un
            .epub del explorador de archivos.
          </p>
        {:else}
          <ul class="book-list">
            {#each library as entry (entry.id)}
              <li>
                <button
                  class="book-item"
                  onclick={() => openFromLibrary(entry)}
                >
                  {#if entry.cover_path}
                    <img
                      class="book-cover"
                      src={convertFileSrc(entry.cover_path)}
                      alt=""
                    />
                  {:else}
                    <span class="book-cover book-cover-placeholder">📖</span>
                  {/if}
                  <span class="book-info">
                    <span class="book-title">{entry.title}</span>
                    <span class="book-author">{entry.author}</span>
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </div>
    </div>
  {:else}
    <div class="reader-bar" data-tauri-drag-region>
      <button class="icon-btn" onclick={closeBook} title="Volver">←</button>
      <span class="reader-title" data-tauri-drag-region
        >{currentBook.title}</span
      >
      <button
        class="icon-btn"
        onclick={() => openNavTab("toc")}
        title="Índice y marcadores"
      >
        ☰
      </button>
      <button
        class="icon-btn"
        class:selected={isBookmarked}
        onclick={toggleBookmark}
        disabled={!readerReady}
        title={isBookmarked
          ? "Sacar marcador de esta página"
          : "Marcar esta página"}
      >
        <svg
          class="bookmark-icon"
          viewBox="0 0 24 24"
          aria-hidden="true"
          focusable="false"
        >
          <path
            d="M6 3.75A1.75 1.75 0 0 1 7.75 2h8.5A1.75 1.75 0 0 1 18 3.75V22l-6-3.75L6 22V3.75Z"
          />
        </svg>
      </button>
      <button
        class="icon-btn"
        onclick={toggleSettings}
        title="Tipografía y color"
      >
        ⚙
      </button>
    </div>

    <div class="reader-viewport theme-{theme}">
      <div class="reader-target" bind:this={readerEl}></div>
      {#if !readerReady && !readerError}
        <p class="loading">Cargando…</p>
      {/if}
      {#if readerError}
        <p class="error">{readerError}</p>
      {/if}

      {#if navTab}
        <div class="panel nav-panel">
          <div class="tab-row">
            <button
              class:selected={navTab === "toc"}
              onclick={() => (navTab = "toc")}
            >
              Índice
            </button>
            <button
              class:selected={navTab === "bookmarks"}
              onclick={() => (navTab = "bookmarks")}
            >
              Marcadores
            </button>
            <button
              class:selected={navTab === "search"}
              onclick={() => (navTab = "search")}
            >
              Buscar
            </button>
          </div>

          {#if navTab === "toc"}
            {#if toc.length === 0}
              <p class="empty">Este epub no trae índice.</p>
            {:else}
              <ul>
                {#each toc as item}
                  <li>
                    <button class="list-item" onclick={() => goToChapter(item)}>
                      {item.label.trim()}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {:else if navTab === "bookmarks"}
            {#if bookmarks.length === 0}
              <p class="empty">
                Todavía no marcaste ninguna página — usá el 🔖 de arriba.
              </p>
            {:else}
              <ul>
                {#each bookmarks as bookmark}
                  <li>
                    <button
                      class="list-item"
                      onclick={() => goToBookmark(bookmark)}
                    >
                      {bookmark.label}
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
          {:else if navTab === "search"}
            <form class="search-form" onsubmit={runSearch}>
              <input
                type="text"
                placeholder="Buscar en el libro…"
                bind:value={searchQuery}
                disabled={searching}
              />
              <button type="submit" disabled={searching || !searchQuery.trim()}>
                {searching ? "…" : "Ir"}
              </button>
            </form>
            {#if searchError}
              <p class="error">{searchError}</p>
            {:else if searching}
              <p class="empty">Buscando…</p>
            {:else if searchResults.length > 0}
              <ul>
                {#each searchResults as result}
                  <li>
                    <button
                      class="list-item"
                      onclick={() => goToSearchResult(result)}
                    >
                      {result.excerpt.trim()}
                    </button>
                  </li>
                {/each}
              </ul>
            {:else if searchQuery.trim()}
              <p class="empty">Sin resultados.</p>
            {/if}
          {/if}
        </div>
      {/if}

      {#if showSettings}
        <div class="panel settings-panel">
          <p class="panel-title">Tipografía y color</p>
          <div class="settings-row">
            <button onclick={smallerFont} title="Achicar letra">A-</button>
            <span class="font-size-label">{fontSize}%</span>
            <button onclick={biggerFont} title="Agrandar letra">A+</button>
          </div>
          <div class="settings-row">
            <button
              class:selected={theme === "light"}
              onclick={() => (theme = "light")}
            >
              Claro
            </button>
            <button
              class:selected={theme === "sepia"}
              onclick={() => (theme = "sepia")}
            >
              Sepia
            </button>
            <button
              class:selected={theme === "dark"}
              onclick={() => (theme = "dark")}
            >
              Oscuro
            </button>
            <button
              class:selected={theme === "ghost"}
              onclick={() => (theme = "ghost")}
            >
              ghost
            </button>
          </div>
          <p class="panel-title">Navegación</p>
          <div class="settings-row">
            <button
              class:selected={readingMode === "paginated"}
              onclick={() => setReadingMode("paginated")}
            >
              Hojear
            </button>
            <button
              class:selected={readingMode === "scrolled"}
              onclick={() => setReadingMode("scrolled")}
            >
              Scroll
            </button>
          </div>
          {#if readingMode === "paginated"}
            <p class="empty">
              Tocá el tercio izquierdo/derecho de la página para pasarla, o usá
              los botones de abajo.
            </p>
          {/if}
        </div>
      {/if}
    </div>

    <div class="reader-controls">
      <button onclick={prevPage} disabled={!readerReady}>‹ Anterior</button>
      <button onclick={nextPage} disabled={!readerReady}>Siguiente ›</button>
    </div>
  {/if}
</main>

<style>
  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    height: 100%;
    background: transparent;
    overflow: hidden;
  }

  .widget {
    box-sizing: border-box;
    height: 100vh;
    width: 100vw;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    gap: 0.5rem;
    padding: 1rem;
    text-align: center;
    color: #f6f6f6;
    background: rgba(40, 30, 60, 0.43);
    border-radius: 16px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    -webkit-user-select: none;
    user-select: none;
    cursor: grab;
    overflow: hidden;
  }

  .clock {
    margin: 0;
    font-variant-numeric: tabular-nums;
    font-size: 0.95rem;
    opacity: 0.85;
  }

  .dashboard {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    width: 100%;
    height: 100%;
    min-height: 0;
    text-align: left;
    cursor: default;
  }

  .dashboard-header {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 0.5rem;
    cursor: grab;
  }

  .dashboard-title {
    font-size: 1.1rem;
    font-weight: 700;
  }

  .load-btn {
    width: 100%;
    padding: 0.55rem;
    font-size: 0.85rem;
  }

  .stats-row {
    display: flex;
    gap: 0.4rem;
    cursor: default;
  }

  .stat-tile {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.1rem;
    background: rgba(255, 255, 255, 0.08);
    border-radius: 8px;
    padding: 0.4rem 0.2rem;
  }

  .stat-value {
    font-size: 0.85rem;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .stat-label {
    font-size: 0.6rem;
    opacity: 0.7;
    text-transform: uppercase;
    letter-spacing: 0.02em;
  }

  button {
    border-radius: 8px;
    border: none;
    padding: 0.35rem 0.55rem;
    font-size: 0.8rem;
    font-weight: 600;
    background: #605b767b;
    color: white;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  button.selected {
    background: #ffffff;
    color: #2a1f45;
  }

  .icon-btn {
    padding: 0.3rem 0.5rem;
  }

  .bookmark-icon {
    display: block;
    width: 1.1rem;
    height: 1.1rem;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .icon-btn.selected .bookmark-icon {
    fill: currentColor;
  }

  .error {
    margin: 0;
    font-size: 0.75rem;
    color: #ff8a8a;
    word-break: break-word;
    cursor: default;
    -webkit-user-select: text;
    user-select: text;
  }

  .hint {
    margin-top: auto;
    padding-top: 0.5rem;
    font-size: 0.7rem;
    line-height: 1.3;
    opacity: 0.6;
  }

  .reader-bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    cursor: grab;
  }

  .reader-title {
    flex: 1;
    text-align: left;
    font-size: 0.9rem;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .reader-viewport {
    flex: 1;
    width: 100%;
    min-height: 0;
    border-radius: 10px;
    overflow: hidden;
    position: relative;
    cursor: default;
    -webkit-user-select: text;
    user-select: text;
  }

  .reader-viewport.theme-light {
    background: #faf7f2;
  }
  .reader-viewport.theme-dark {
    background: #1c1b22;
  }
  .reader-viewport.theme-sepia {
    background: #f4ecd8;
  }

  .reader-target {
    height: 100%;
    width: 100%;
  }

  .loading {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #333;
    font-size: 0.8rem;
    margin: 0;
  }

  .reader-controls {
    display: flex;
    gap: 0.4rem;
    width: 100%;
    justify-content: center;
    cursor: default;
    flex-wrap: wrap;
  }

  .panel {
    position: absolute;
    inset: 0;
    background: rgba(20, 15, 30, 0.92);
    color: #f6f6f6;
    padding: 0.75rem;
    overflow-y: auto;
    text-align: left;
  }

  /* Se puede seguir bajando con la rueda del mouse/touch — solo se oculta
     la barra visible, overflow-y sigue en auto/scroll. */
  .panel,
  .dashboard-list {
    scrollbar-width: none; /* Firefox */
    -ms-overflow-style: none; /* Edge viejo */
  }
  .panel::-webkit-scrollbar,
  .dashboard-list::-webkit-scrollbar {
    display: none; /* WebView2 (Chromium) */
  }

  .dashboard-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  .book-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
  }

  .book-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    text-align: left;
    background: rgba(255, 255, 255, 0.08);
    padding: 0.35rem;
    font-weight: 400;
  }

  .book-item:hover {
    background: rgba(255, 255, 255, 0.16);
  }

  .book-cover {
    flex: none;
    width: 2.2rem;
    height: 3rem;
    border-radius: 4px;
    object-fit: cover;
    background: rgba(0, 0, 0, 0.25);
  }

  .book-cover-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.1rem;
  }

  .book-info {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }

  .book-title {
    font-size: 0.82rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .book-author {
    font-size: 0.72rem;
    opacity: 0.7;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .panel-title {
    margin: 0 0 0.5rem;
    font-size: 0.85rem;
    font-weight: 700;
    opacity: 0.85;
  }

  .nav-panel .tab-row {
    display: flex;
    gap: 0.3rem;
    margin-bottom: 0.6rem;
  }

  .nav-panel .tab-row button {
    flex: 1;
    background: rgba(255, 255, 255, 0.1);
  }

  .nav-panel ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
  }

  .list-item {
    display: block;
    width: 100%;
    text-align: left;
    background: transparent;
    color: inherit;
    font-weight: 400;
    font-size: 0.78rem;
    line-height: 1.3;
    padding: 0.35rem 0.4rem;
    border-radius: 6px;
  }

  .list-item:hover {
    background: rgba(255, 255, 255, 0.12);
  }

  .empty {
    font-size: 0.78rem;
    opacity: 0.65;
    margin: 0.5rem 0 0;
  }

  .search-form {
    display: flex;
    gap: 0.4rem;
    margin-bottom: 0.5rem;
  }

  .search-form input {
    flex: 1;
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    padding: 0.3rem 0.5rem;
    font-size: 0.78rem;
    background: rgba(0, 0, 0, 0.25);
    color: #f6f6f6;
  }

  .settings-panel .settings-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.6rem;
  }

  .font-size-label {
    min-width: 3em;
    text-align: center;
    font-variant-numeric: tabular-nums;
    font-size: 0.8rem;
    opacity: 0.85;
  }
</style>
