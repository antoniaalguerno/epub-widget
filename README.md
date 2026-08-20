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

- [ ]

PS C:\epub-widget> npm run tauri dev

> epub-widget@0.1.0 tauri
> tauri dev

failed to run 'cargo metadata' command to get workspace directory: failed to run command cargo metadata --no-deps --format-version 1: program not found
Error failed to run 'cargo metadata' command to get workspace directory: failed to run command cargo metadata --no-deps --format-version 1: program not found
PS C:\epub-widget> npm run tauri dev

> epub-widget@0.1.0 tauri
> tauri dev

failed to run 'cargo metadata' command to get workspace directory: failed to run command cargo metadata --no-deps --format-version 1: program not found
Error failed to run 'cargo metadata' command to get workspace directory: failedto run command cargo metadata --no-deps --format-version 1: program not found
PS C:\epub-widget>
