<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  // Fase 1: mecánica de ventana (shell). Fase 2: núcleo del epub — todavía
  // sin UI de lectura, solo probamos que import_epub extraiga y devuelva
  // metadata de forma segura. El input de ruta es un placeholder de prueba;
  // el selector de archivo "de verdad" llega en la Fase 4.
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

  let epubPath = $state("");
  let importing = $state(false);
  let result = $state<EpubMeta | null>(null);
  let error = $state("");

  async function importEpub(event: Event) {
    event.preventDefault();
    importing = true;
    error = "";
    result = null;
    try {
      const meta = await invoke<EpubMeta>("import_epub", { path: epubPath });
      result = meta;
      console.log("[import_epub] ok:", meta);
    } catch (e) {
      error = String(e);
      console.error("[import_epub] error:", e);
    } finally {
      importing = false;
    }
  }
</script>

<!--
  data-tauri-drag-region en el contenedor raíz hace que toda la superficie
  del widget sea arrastrable, no solo una barra de título (que no existe,
  porque la ventana corre con decorations: false). El form de import queda
  fuera de esa zona para poder clickear/tipear normalmente.
-->
<main class="widget" data-tauri-drag-region>
  <h1 data-tauri-drag-region>epub-widget</h1>
  <p class="clock" data-tauri-drag-region>{hora}</p>

  <form class="import" onsubmit={importEpub}>
    <input
      type="text"
      placeholder="Ruta a un .epub (C:\...)"
      bind:value={epubPath}
      disabled={importing}
    />
    <button type="submit" disabled={importing || !epubPath}>
      {importing ? "Importando…" : "Importar"}
    </button>
  </form>

  {#if error}
    <p class="error">{error}</p>
  {/if}

  {#if result}
    <div class="result">
      <p class="title">{result.title}</p>
      <p class="author">{result.author}</p>
      <p class="path">portada: {result.cover_path ?? "(sin portada)"}</p>
      <p class="path">carpeta: {result.book_dir}</p>
    </div>
  {/if}

  <p class="hint" data-tauri-drag-region>
    Arrastrame desde cualquier punto libre. El ícono de la bandeja me
    muestra/oculta o me cierra de verdad.
  </p>
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
    padding: 1.5rem;
    text-align: center;
    color: #f6f6f6;
    background: rgba(40, 30, 60, 0.9);
    border-radius: 16px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
    -webkit-user-select: none;
    user-select: none;
    cursor: grab;
    overflow-y: auto;
  }

  h1 {
    margin: 0.5rem 0 0;
    font-size: 1.3rem;
  }

  .clock {
    margin: 0;
    font-variant-numeric: tabular-nums;
    font-size: 0.95rem;
    opacity: 0.85;
  }

  .import {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    width: 100%;
    margin-top: 0.75rem;
    cursor: default;
    -webkit-user-select: text;
    user-select: text;
  }

  .import input {
    border-radius: 8px;
    border: 1px solid rgba(255, 255, 255, 0.25);
    padding: 0.4rem 0.6rem;
    font-size: 0.8rem;
    background: rgba(0, 0, 0, 0.25);
    color: #f6f6f6;
  }

  .import button {
    border-radius: 8px;
    border: none;
    padding: 0.4rem 0.6rem;
    font-size: 0.85rem;
    font-weight: 600;
    background: #7c5cff;
    color: white;
    cursor: pointer;
  }

  .import button:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .error {
    margin: 0;
    font-size: 0.75rem;
    color: #ff8a8a;
    word-break: break-word;
  }

  .result {
    width: 100%;
    text-align: left;
    background: rgba(0, 0, 0, 0.2);
    border-radius: 8px;
    padding: 0.5rem 0.6rem;
    cursor: default;
    -webkit-user-select: text;
    user-select: text;
  }

  .result .title {
    margin: 0;
    font-weight: 700;
    font-size: 0.9rem;
  }

  .result .author {
    margin: 0.1rem 0 0.3rem;
    opacity: 0.85;
    font-size: 0.8rem;
  }

  .result .path {
    margin: 0.1rem 0 0;
    font-size: 0.65rem;
    opacity: 0.6;
    word-break: break-all;
  }

  .hint {
    margin-top: auto;
    padding-top: 0.5rem;
    font-size: 0.7rem;
    line-height: 1.3;
    opacity: 0.6;
  }
</style>
