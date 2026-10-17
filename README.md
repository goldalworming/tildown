# TilDown

**TilDown** (Tilde + markdown) is a fast, lightweight markdown notes app in the spirit of Simplenote and Notational Velocity. It opens instantly and you can start typing right away. Your notes are plain `.md` files.

Built from scratch in Rust on **wgpu + winit**. One codebase runs on desktop and the web, with Android planned. There is no WebView, no Electron and no JavaScript editor.

![TilDown, light theme](docs/screenshot.png)
![TilDown, dark theme with diagrams](docs/screenshot-dark.png)

## Features

- **Instant**: the window appears in about 150 ms, uses 0% CPU when idle and about 30 MB of RAM. It stays fast with 10,000 notes: they load in under 1 s and search takes 1–3 ms per keystroke.
- **Plain files**: each note is a `.md` file in `Documents/TilDown`. Saving is atomic (write, fsync, rename), with autosave 1 s after you stop typing. You can sync the folder with Syncthing, Dropbox, iCloud and similar tools.
- **Notational Velocity search**:
  - Type to filter your notes.
  - Enter opens the top match, or creates a new note if nothing matches.
  - Matches are highlighted inside the note. Step through them with F3 or Shift+F3.
- **Editor**: markdown highlighting in the style of iA Writer, list and task continuation, undo/redo, and IME input.
- **Read mode**:
  - Headings, lists, task checkboxes you can click, quotes, code blocks, tables, local images and links.
- **Native diagrams**, with no mermaid.js, Java or Graphviz:
  - **Mermaid**: flowcharts and sequence diagrams.
  - **PlantUML**: sequence, activity, class, object, use case, component, state, mindmap, WBS, gantt, timing, JSON, YAML, nwdiag, salt, EBNF, regex, Chen ER, chart, git, wire, ditaa and more.
  - Click a diagram to open a viewer with zoom, pan and pinch.
- **Trash**: deleted notes can be restored for 30 days.
- **Settings**: light, dark or system theme, text size, line width, and whether notes open in edit or read mode.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Ctrl+N | New note |
| Ctrl+F / Ctrl+L | Search |
| Ctrl+E | Toggle edit / read |
| Ctrl+S | Save now |
| Ctrl+\ | Toggle sidebar |
| F3 / Shift+F3 | Next / previous search match |
| Esc | Close search, dialog or viewer |

## Build

You need [Rust](https://rustup.rs) (stable).

```sh
cargo run --release          # desktop (Windows, macOS, Linux)
cargo test
```

Set `MARKDOWN_DIR=<path>` to use a different notes folder.

### Web

The web build uses WebGPU, falling back to WebGL2. Notes are stored in the browser's OPFS.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli   # same version as in Cargo.lock
sh web/build.sh
python -m http.server -d web 8080
```

## Project layout

```
src/
  doc.rs        editor core: text, cursor, selection, undo
  store.rs      notes folder: load, atomic save, rescan, search, trash
  md.rs         markdown highlighting and read-mode blocks (pulldown-cmark)
  text.rs       fonts (ab_glyph), glyph atlas, layout, hit-testing
  gpu.rs        wgpu quad pipeline
  app.rs        winit shell and the whole UI
  mermaid.rs    Mermaid parser and layout
  plantuml/     PlantUML parsers and layouts
  web.rs        wasm: OPFS mirror, clipboard, links
web/            index.html and build script
docs/PRD.md     product requirements (Indonesian)
```

## Credits

- Fonts: [IBM Plex](https://github.com/IBM/plex) and [Noto Sans](https://fonts.google.com/noto), both under the SIL Open Font License.
- Icons: [Lucide](https://lucide.dev) (ISC).
