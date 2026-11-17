# Hazelnut

A small, minimalist rich-text notebook for the desktop, written in Rust.

- **Home screen.** Create or open a notebook. Your **starred** notebooks are
  listed above the **recent** ones.
- **Notebooks and chapters.** A notebook is one `.hzn` file. Its chapters are
  the pages you see in the sidebar. Drag chapters to reorder them, and file
  them under colored **categories**, which group them in the sidebar.
- **Inline formatting.** Bold, italic, underline (solid, double, dotted,
  dashed, wavy, zig-zag), strikethrough, text color and highlight color.
- **Headings and lists.** Start a line with `#`/`##`/`###`, `- ` or `- [ ] `
  (or use the toolbar) for headings, bullet lists and clickable checklists.
- **Callouts.** Start lines with `> [!] ` for a tinted box; color it from the
  toolbar or with `> [!blue]`, `> [!#d46ab4]` etc.
- **Literal text.** The `Aa` toolbar toggle (Ctrl+Shift+M), also in Settings,
  backslash-escapes formatting characters as they are typed or pasted and stops
  automatic list/callout continuation while enabled. Existing formatting stays
  rendered. Literal characters stay literal after toggling off, saving, reopening
  and exporting to PDF.
- **Code blocks.** Fenced blocks such as ` ```rust ` or ` ```python ` use
  monospace text and syntax colors. Insert one from the `{ }` toolbar button
  and choose its language, or press Ctrl+Alt+K for a plain block. Code stays
  literal and exports with colors to PDF.
- **Code view.** Switch between the normal view and a code view (the `</>` button at the bottom of the toolbar,
  or Ctrl+/) that shows and edits a chapter as plain text with all its
  formatting symbols, exactly as stored in the file.
- **PDF export.** Export the whole notebook (with a contents page) or a
  single chapter as a PDF, formatted for paper.
- **Plain-text file format.** Notebooks are readable and easy to diff. See
  [FORMAT.md](FORMAT.md).
- **Companions.** Pick a pixel-art animal in Settings (fox, rabbits, deer,
  frogs, turtle, capybara, penguin, chickens, ducks, pigs, sheep,
  cats, dogs) to wander along the bottom of the window.
- **GPU rendering.** Built on [egui](https://github.com/emilk/egui) and
  eframe, using the **wgpu** backend.

## Running

```sh
cargo run --release                 # opens the home screen
cargo run --release -- notes.hzn    # opens notes.hzn, or creates it on first save
```

Build and install the release version from the repository root:

```sh
cargo install-release
```

This builds with locked dependencies and replaces the installed release binary
in Cargo's binary directory (usually `~/.cargo/bin`). On Linux, it also creates
`~/.local/bin/hazelnut` and installs a GNOME application launcher. Run `hazelnut`
in a terminal or search for **Hazelnut** in GNOME.

If you already installed the binary, add desktop/terminal integration without
rebuilding:

```sh
cargo install-release --desktop-only
```

Your shell must include `~/.local/bin` or Cargo's binary directory in `PATH`.
The installer reports if neither is available. The GNOME launcher uses the
absolute binary path, so it works independently of your shell's `PATH`.

## Keyboard shortcuts

| Action                 | Shortcut          |
|------------------------|-------------------|
| Bold / Italic / Underline | Ctrl+B / Ctrl+I / Ctrl+U |
| Strikethrough          | Ctrl+Shift+X      |
| Clear text formatting  | Ctrl+\ or Ctrl+Space |
| Insert code block | Ctrl+Alt+K |
| Normal text (no heading / list / callout) | Ctrl+Shift+Space |
| Clear all formatting   | Ctrl+Shift+\      |
| New chapter            | Ctrl+T            |
| Export as PDF          | Ctrl+E            |
| Heading 1 / 2 / 3      | Ctrl+Alt+1 / 2 / 3 |
| Normal text            | Ctrl+Alt+0        |
| Bullet list / checklist| Ctrl+Shift+L / Ctrl+Shift+K |
| Callout on / off       | Ctrl+Shift+Q      |
| Normal / code view     | Ctrl+/            |
| Next / previous chapter| Ctrl+PgDn / Ctrl+PgUp |
| Save / Save as         | Ctrl+S / Ctrl+Shift+S |
| Open / New notebook    | Ctrl+O / Ctrl+N   |
| Back to home screen    | Ctrl+W            |
| Settings               | Ctrl+,            |

If you pick a style with no text selected, it applies to what you type next.
Drag a chapter to reorder it within its category; right-click it to change
its category or delete it. Manage categories with the gear next to "Chapters". Star a notebook from the home
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
  settings.rs preferences, saved to ~/.config/hazelnut/settings
  pets.rs    companion animals: sprite sheets, animation, wandering
  model.rs   Notebook / Chapter / Style
  format.rs  .hzn reader and writer (+ tests)
  edit.rs    keeps per-character styles in sync with edits (+ tests)
  render.rs  styled text → egui layout, underline/strike/selection painting
  theme.rs   palette, fonts, widget styling
assets/
  fonts/     Noto Sans + OFL.txt (SIL Open Font License)
  pets/      pixel-art asset pack by EmanuelleDev (not MIT, see below)
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

Hazelnut's code is released under the [MIT License](LICENSE).
Copyright © 2026 OsakiTsukiko.

### Third-party assets

- **Fonts:** Noto Sans, SIL Open Font License 1.1 (`assets/fonts/OFL.txt`).
- **Pet sprites:** by **EmanuelleDev** —
  [emanuelledev.itch.io](https://emanuelledev.itch.io). These are *not*
  covered by the MIT license. The pack's license allows use in commercial and
  non-commercial projects with credit, but the assets may not be resold or
  redistributed on their own, even if modified.
