# Epub Widget

Widget de escritorio para Windows que muestra un ereader flotante (always-on-top, sin bordes) para archivos `.epub`.

**Stack:** Tauri 2.0 + Svelte 5 (SvelteKit, adapter-static) + TypeScript + epub.js

Ver el plan de desarrollo completo en [plan_accion_ereader_widget.md](plan_accion_ereader_widget.md).

## Desarrollo

Requiere Rust (`rustc`/`cargo`) y las dependencias nativas de Windows para Tauri (WebView2 + MSVC Build Tools). Ver [tauri.app/start/prerequisites](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev
```

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
