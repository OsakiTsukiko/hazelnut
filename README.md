# Hazelnut

A small, minimalist rich-text notebook for the desktop, written in Rust.

- **Home screen.** Create or open a notebook. Your **starred** notebooks are
  listed above the **recent** ones.
- **Notebooks and chapters.** A notebook is one `.hzn` file. Its chapters are
  the pages you see in the sidebar.
- **Inline formatting.** Bold, italic, underline (solid, double, dotted,
  dashed, wavy, zig-zag), strikethrough, text color and highlight color.
- **Plain-text file format.** Notebooks are readable and easy to diff. See
  [FORMAT.md](FORMAT.md).
- **GPU rendering.** Built on [egui](https://github.com/emilk/egui) and
  eframe, using the **wgpu** backend.

## Running

```sh
cargo run --release                 # opens the home screen
cargo run --release -- notes.hzn    # opens notes.hzn, or creates it on first save
```

## Keyboard shortcuts

| Action                 | Shortcut          |
|------------------------|-------------------|
| Bold / Italic / Underline | Ctrl+B / Ctrl+I / Ctrl+U |
| Strikethrough          | Ctrl+Shift+X      |
| Clear formatting       | Ctrl+\            |
| New chapter            | Ctrl+T            |
| Next / previous chapter| Ctrl+PgDn / Ctrl+PgUp |
| Save / Save as         | Ctrl+S / Ctrl+Shift+S |
| Open / New notebook    | Ctrl+O / Ctrl+N   |
| Back to home screen    | Ctrl+W            |

If you pick a style with no text selected, it applies to what you type next.
Right-click a chapter to move or delete it. Star a notebook from the home
screen or with the star next to its file name in the sidebar. You can also drop a `.hzn` file
onto the window to open it.

## Layout

```
src/
  main.rs    window setup (wgpu renderer)
  app.rs     UI: sidebar, toolbar, editor, dialogs
  app/home.rs  home screen: create/open, starred and recent notebooks
  autosave.rs crash-recovery copies (+ tests)
  library.rs recent/starred list, saved to ~/.config/hazelnut/library
  model.rs   Notebook / Chapter / Style
  format.rs  .hzn reader and writer (+ tests)
  edit.rs    keeps per-character styles in sync with edits (+ tests)
  render.rs  styled text → egui layout, underline/strike/selection painting
  theme.rs   palette, fonts, widget styling
assets/
  fonts/     Noto Sans + OFL.txt (SIL Open Font License)
  welcome.hzn
```

## Notes

- **Crash recovery.** While a notebook has unsaved changes, Hazelnut writes a
  hidden copy next to it every few seconds (`.notes.hzn.autosave` for
  `notes.hzn`). The copy is deleted when you save or choose "Don't save". If
  one is still there when you open the notebook (after a crash, say), you're
  asked whether to open the recovered version.

- Undo (Ctrl+Z) is egui's built-in text undo. It restores text but not
  formatting changes.
- The bundled Noto Sans fonts are licensed under the SIL Open Font License
  1.1 (see `assets/fonts/OFL.txt`).

## License

Hazelnut is released under the [MIT License](LICENSE).
Copyright © 2026 OsakiTsukiko.
